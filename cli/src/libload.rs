//! Build-time support-library introspection.
//!
//! For the implicit `core` library and each `use <name>`, the loader:
//!   1. builds an introspection shared object from the library's sources
//!      (impls + the `*_libinfo.c` metadata TU, minus the program entry),
//!   2. `dlopen`s it and calls `openepl_get_lib_info` to read the `LibInfo`
//!      metadata, populating a `Registry` (the authoritative signatures — no
//!      hard-coded Rust table), and
//!   3. records the library's *implementation* sources (everything except the
//!      `*_libinfo.c` metadata TU) for static-linking into the program.
//!
//! The metadata TU never enters the program link line, preserving `--gc-sections`
//! dead-stripping.
//!
//! `dlopen` runs the target library in-process, so this native path assumes
//! host == target (x86_64-linux). Cross-compilation needs a sidecar manifest
//! instead (Phase 4).

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::process::Command;

use openepl_ir::registry::{ComponentDesc, ComponentKind, PropertyDesc};
use openepl_ir::{Registry, Signature, Ty};

extern "C" {
    fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> c_int;
    fn dlerror() -> *mut c_char;
}
const RTLD_LAZY: c_int = 1;

#[repr(C)]
struct CommandDescC {
    name: *const c_char,
    symbol: *const c_char,
    ret_tag: i32,
    argc: i32,
    arg_tags: *const i32,
}

#[repr(C)]
struct PropertyDescC {
    name: *const c_char,
    tag: i32,
    default_value: *const c_char,
    editor: *const c_char,
}

#[repr(C)]
struct EventDescC {
    name: *const c_char,
    // Appended in ABI v3, and appended is what makes them safe to read: a
    // descriptor written as `{ "click" }` zero-fills to a count of 0 and a null
    // table, which is the same "hands its handler nothing" every v2 event meant.
    param_count: i32,
    param_tags: *const i32,
}

#[repr(C)]
struct ComponentDescC {
    name: *const c_char,
    a11y_role: i32,
    property_count: i32,
    properties: *const PropertyDescC,
    event_count: i32,
    events: *const EventDescC,
    kind: i32,
}

#[repr(C)]
struct LibInfoC {
    abi_version: i32,
    name: *const c_char,
    guid: *const c_char,
    ver_major: i32,
    ver_minor: i32,
    ver_build: i32,
    command_count: i32,
    commands: *const CommandDescC,
    component_count: i32,
    components: *const ComponentDescC,
}

const OPENEPL_ABI_VERSION: i32 = 3;

/// The result of resolving a module's libraries.
pub struct LibPlan {
    /// Combined command + component registry across all loaded libraries.
    pub registry: Registry,
    /// Implementation sources to static-link into the program.
    pub impl_sources: Vec<PathBuf>,
    /// Extra compiler/linker configuration contributed by libraries that need
    /// it (the UI stack, for example). Only populated for libraries actually
    /// used, so a console program never links the UI.
    pub build: BuildConfig,
}

/// Compiler/linker additions a library needs, from its `lib.json`.
#[derive(Debug, Default, Clone)]
pub struct BuildConfig {
    pub include_dirs: Vec<PathBuf>,
    pub extra_sources: Vec<PathBuf>,
    pub defines: Vec<String>,
    pub pkg_config: Vec<String>,
    pub link_args: Vec<String>,
    /// Any library needed C++, so the link step must use clang++.
    pub needs_cxx: bool,
}

/// Resolve `core` + each `use`d library under `repo_root`.
pub fn load(repo_root: &Path, uses: &[String]) -> Result<LibPlan, String> {
    load_with(repo_root, uses, true)
}

/// Introspect libraries without requiring what only a *link* needs.
///
/// The metadata translation unit references command implementations by symbol
/// name rather than by pointer, so reading a library's descriptors never needs
/// its vendored dependencies. Asking what commands exist should not require the
/// ability to build a window — which is also what lets the documentation be
/// generated on a machine that has never fetched the UI stack.
pub fn load_metadata(repo_root: &Path, uses: &[String]) -> Result<LibPlan, String> {
    load_with(repo_root, uses, false)
}

fn load_with(repo_root: &Path, uses: &[String], require_impl: bool) -> Result<LibPlan, String> {
    let mut registry = Registry::new();
    let mut impl_sources = Vec::new();
    let mut build = BuildConfig::default();

    // (library name, directory) — core is implicit and first.
    let mut libs: Vec<(String, PathBuf)> = vec![("core".into(), repo_root.join("runtime"))];
    for u in uses {
        libs.push((u.clone(), repo_root.join("libs").join(u)));
    }

    for (name, dir) in &libs {
        if !dir.is_dir() {
            return Err(format!(
                "library `{name}`: directory not found: {}",
                dir.display()
            ));
        }
        let manifest = Manifest::load(dir, repo_root)?;
        if require_impl {
            manifest
                .check_requirements()
                .map_err(|e| format!("library `{name}`: {e}"))?;
        }

        let mut sources = c_sources(dir)?;
        sources.extend(manifest.extra_sources.iter().cloned());

        // Introspection .so: the metadata TU alone. It references command
        // implementations by symbol NAME, not by pointer, so it has no
        // dependency on them — which keeps this .so tiny, quick to build, and
        // free of the implementation's link requirements (a static RmlUi, for
        // instance, is not position-independent and could not go in a .so).
        let so_srcs: Vec<PathBuf> = sources
            .iter()
            .filter(|p| filename(p).ends_with("_libinfo.c"))
            .cloned()
            .collect();
        if so_srcs.is_empty() {
            return Err(format!(
                "library `{name}`: no *_libinfo.c metadata source found in {}",
                dir.display()
            ));
        }
        // Impl sources to link: everything except the metadata TU, which must
        // never reach a shipped program.
        for p in &sources {
            if !filename(p).ends_with("_libinfo.c") {
                impl_sources.push(p.clone());
            }
        }

        build.merge(&manifest);

        let so_path = build_introspection_so(repo_root, name, &so_srcs, &manifest)?;
        introspect_into(&so_path, name, &mut registry)?;
    }

    Ok(LibPlan {
        registry,
        impl_sources,
        build,
    })
}

fn filename(p: &Path) -> String {
    p.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}

/// All C/C++ sources directly in `dir`, sorted for determinism.
fn c_sources(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))? {
        let p = entry.map_err(|e| e.to_string())?.path();
        if matches!(
            p.extension().and_then(|s| s.to_str()),
            Some("c") | Some("cpp") | Some("cc") | Some("cxx")
        ) {
            out.push(p);
        }
    }
    out.sort();
    Ok(out)
}

/// Build `lib<name>.so` for introspection; returns its path.
fn build_introspection_so(
    repo_root: &Path,
    name: &str,
    sources: &[PathBuf],
    manifest: &Manifest,
) -> Result<PathBuf, String> {
    let build_dir = std::env::temp_dir().join("openepl-build");
    std::fs::create_dir_all(&build_dir).map_err(|e| e.to_string())?;
    let so_path = build_dir.join(format!("lib{name}_introspect_{}.so", std::process::id()));

    // Always plain C: the metadata TU has no implementation dependencies.
    let mut cmd = Command::new("clang");
    cmd.arg("-shared")
        .arg("-fPIC")
        .arg("-I")
        .arg(repo_root.join("abi"))
        .arg("-I")
        .arg(repo_root.join("runtime")); // for openepl_core.h
    for d in &manifest.include_dirs {
        cmd.arg("-I").arg(d);
    }
    for d in &manifest.defines {
        cmd.arg(format!("-D{d}"));
    }
    for s in sources {
        cmd.arg(s);
    }
    cmd.arg("-o").arg(&so_path);

    let status = cmd.status().map_err(|e| format!("invoke clang: {e}"))?;
    if !status.success() {
        return Err(format!("building introspection library `{name}` failed"));
    }
    Ok(so_path)
}

/// dlopen the `.so`, read its `LibInfo`, and register its commands.
fn introspect_into(so_path: &Path, name: &str, registry: &mut Registry) -> Result<(), String> {
    // SAFETY: we read a C `LibInfo` of the frozen ABI layout from a library we
    // just built; all pointers are copied into owned Rust values before dlclose.
    unsafe {
        let cpath = CString::new(so_path.to_string_lossy().as_bytes()).unwrap();
        let handle = dlopen(cpath.as_ptr(), RTLD_LAZY);
        if handle.is_null() {
            let e = CStr::from_ptr(dlerror()).to_string_lossy().into_owned();
            return Err(format!("dlopen `{name}`: {e}"));
        }
        let sym = CString::new("openepl_get_lib_info").unwrap();
        let getfn = dlsym(handle, sym.as_ptr());
        if getfn.is_null() {
            dlclose(handle);
            return Err(format!(
                "library `{name}` does not export openepl_get_lib_info"
            ));
        }
        let getfn: extern "C" fn() -> *const LibInfoC = std::mem::transmute(getfn);
        let info = getfn();
        if info.is_null() {
            dlclose(handle);
            return Err(format!("library `{name}` returned a null LibInfo"));
        }
        let info = &*info;
        // Copied before dlclose: the format argument would otherwise be read
        // back out of a library that is no longer mapped, so the one path that
        // exists to report a bad ABI would segfault instead of reporting it.
        let found = info.abi_version;
        if found != OPENEPL_ABI_VERSION {
            dlclose(handle);
            return Err(format!(
                "library `{name}` ABI version {found} != {OPENEPL_ABI_VERSION}"
            ));
        }

        let result = register_commands(info, name, registry);
        dlclose(handle);
        result
    }
}

/// An optional C string field. NULL is the descriptor's way of saying "none",
/// which is the empty string on the Rust side rather than an `Option` nobody
/// would ever match on.
///
/// SAFETY: `p` is NULL or a NUL-terminated string owned by the loaded library.
unsafe fn cstr_or_empty(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        CStr::from_ptr(p).to_string_lossy().into_owned()
    }
}

/// SAFETY: `info` is a valid `LibInfoC`; called only from `introspect_into`.
unsafe fn register_commands(
    info: &LibInfoC,
    lib: &str,
    registry: &mut Registry,
) -> Result<(), String> {
    let count = info.command_count.max(0) as isize;
    for i in 0..count {
        let desc = &*info.commands.offset(i);
        let cmd_name = CStr::from_ptr(desc.name).to_string_lossy().into_owned();
        let symbol = CStr::from_ptr(desc.symbol).to_string_lossy().into_owned();

        let ret = if desc.ret_tag == 0 {
            None
        } else {
            Some(Ty::from_sdt_tag(desc.ret_tag).ok_or_else(|| {
                format!(
                    "command `{cmd_name}` in `{lib}`: unsupported return tag {}",
                    desc.ret_tag
                )
            })?)
        };

        let argc = desc.argc.max(0) as isize;
        let mut params = Vec::new();
        for a in 0..argc {
            let tag = *desc.arg_tags.offset(a);
            params.push(Ty::from_sdt_tag(tag).ok_or_else(|| {
                format!("command `{cmd_name}` in `{lib}`: unsupported arg tag {tag}")
            })?);
        }

        if !registry.insert(cmd_name.clone(), Signature { params, ret }, symbol) {
            return Err(format!(
                "command `{cmd_name}` (from `{lib}`) collides with an already-registered command"
            ));
        }
    }

    // Visual components come through the same LibInfo mechanism.
    let ccount = info.component_count.max(0) as isize;
    for i in 0..ccount {
        let cd = &*info.components.offset(i);
        let name = CStr::from_ptr(cd.name).to_string_lossy().into_owned();

        let mut properties = Vec::new();
        for pi in 0..cd.property_count.max(0) as isize {
            let pd = &*cd.properties.offset(pi);
            let pname = CStr::from_ptr(pd.name).to_string_lossy().into_owned();
            let ty = Ty::from_sdt_tag(pd.tag).ok_or_else(|| {
                format!(
                    "component `{name}` in `{lib}`: property `{pname}` has unsupported tag {}",
                    pd.tag
                )
            })?;
            properties.push(PropertyDesc {
                name: pname,
                ty,
                editor: cstr_or_empty(pd.editor),
            });
        }

        let mut events = Vec::new();
        // The parameters cannot be recorded yet: they are keyed by component
        // name, and the component is not in the registry until below.
        let mut event_params: Vec<(String, Vec<Ty>)> = Vec::new();
        for ei in 0..cd.event_count.max(0) as isize {
            let ed = &*cd.events.offset(ei);
            let ename = CStr::from_ptr(ed.name).to_string_lossy().into_owned();
            let mut params = Vec::new();
            for pi in 0..ed.param_count.max(0) as isize {
                let tag = *ed.param_tags.offset(pi);
                params.push(Ty::from_sdt_tag(tag).ok_or_else(|| {
                    format!(
                        "component `{name}` in `{lib}`: event `{ename}` has unsupported parameter tag {tag}"
                    )
                })?);
            }
            if !params.is_empty() {
                event_params.push((ename.clone(), params));
            }
            events.push(ename);
        }

        if !registry.insert_component(ComponentDesc {
            name: name.clone(),
            a11y_role: cd.a11y_role,
            kind: if cd.kind == 1 {
                ComponentKind::NonVisual
            } else {
                ComponentKind::Visual
            },
            library: lib.to_string(),
            properties,
            events,
        }) {
            return Err(format!(
                "component `{name}` (from `{lib}`) collides with an already-registered component"
            ));
        }
        for (event, params) in event_params {
            registry.set_event_params(&name, &event, params);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .to_path_buf()
    }

    /// The dlopen'd `core` LibInfo must match the hard-coded `Registry::core()`
    /// used by the ir/backend unit tests — so the fast in-memory table can never
    /// silently drift from the C source of truth.
    #[test]
    fn core_libinfo_matches_hardcoded_registry() {
        let plan = load(&repo_root(), &[]).expect("introspect core");
        let hard = Registry::core();
        assert_eq!(
            plan.registry.len(),
            hard.len(),
            "core command count drifted"
        );
        for (name, cmd) in hard.iter() {
            let loaded = plan
                .registry
                .get(name)
                .unwrap_or_else(|| panic!("core LibInfo missing command `{name}`"));
            assert_eq!(
                loaded, cmd,
                "command `{name}` signature/symbol drifted from LibInfo"
            );
        }
        // Components are deliberately absent from the hard-coded table: they
        // reach the compiler only through this dlopen path, so mirroring them
        // in Rust would create a second source of truth that nothing reads.
        // Core's own `timer` is covered end-to-end in `cli/tests/build.rs`.
        assert_eq!(
            hard.component_names().count(),
            0,
            "the hard-coded table must not grow components"
        );
    }
}

/// A library's `lib.json`: the extra build configuration it needs.
///
/// Parsed with a small purpose-built reader rather than pulling in a JSON crate
/// — the schema is fixed and tiny (flat string arrays plus one bool), and the
/// compiler has no other runtime dependencies.
#[derive(Debug, Default, Clone)]
pub struct Manifest {
    pub cxx: bool,
    pub include_dirs: Vec<PathBuf>,
    pub extra_sources: Vec<PathBuf>,
    pub defines: Vec<String>,
    pub pkg_config: Vec<String>,
    pub link_args: Vec<String>,
    pub requires: Vec<PathBuf>,
    pub requires_hint: String,
    /// The paths that decide whether the optional dependency is here.
    pub optional_requires: Vec<PathBuf>,
    /// Every `optional_requires` path exists, so the `optional_*` build
    /// configuration has been folded into the fields above and the feature
    /// macro is defined.
    pub optional_enabled: bool,
}

impl Manifest {
    /// Load `<dir>/lib.json` if present; an absent manifest is not an error
    /// (plain C libraries like `core` need no configuration).
    fn load(dir: &Path, repo_root: &Path) -> Result<Manifest, String> {
        let path = dir.join("lib.json");
        if !path.is_file() {
            return Ok(Manifest::default());
        }
        let text =
            std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;

        let mut m = Manifest {
            cxx: json_bool(&text, "cxx"),
            defines: json_array(&text, "defines"),
            pkg_config: json_array(&text, "pkg_config"),
            link_args: json_array(&text, "link_args"),
            requires_hint: json_string(&text, "requires_hint"),
            ..Default::default()
        };
        m.include_dirs = json_array(&text, "include_dirs")
            .iter()
            .map(|p| repo_root.join(p))
            .collect();
        m.extra_sources = json_array(&text, "extra_sources")
            .iter()
            .map(|p| repo_root.join(p))
            .collect();
        m.requires = json_array(&text, "requires")
            .iter()
            .map(|p| repo_root.join(p))
            .collect();
        m.link_args = m
            .link_args
            .iter()
            .map(|a| absolutise(repo_root, a))
            .collect();
        m.take_optional(&text, repo_root);
        Ok(m)
    }

    /// Fold in the `optional_*` configuration when the dependency it names is
    /// actually there.
    ///
    /// `requires` is the right shape for a dependency the whole library is: no
    /// RmlUi means no window means no build, and failing with a hint beats
    /// failing with a wall of linker errors. It is the wrong shape for a
    /// dependency that gates ONE capability — declaring mbedTLS in `requires`
    /// would make every build of a program that merely mentions `net` fail on a
    /// machine that has never fetched a TLS stack, for a program that may not
    /// speak https at all.
    ///
    /// So: present, and the library compiles with the include dirs, sources,
    /// defines and link args it needs plus `optional_feature` defined, which is
    /// what the C tests with `#ifdef`. Absent, and none of it is there and the
    /// build proceeds — the capability fails at run time, loudly, in the
    /// library's own words.
    ///
    /// The fold happens here rather than at the link so that every consumer of
    /// a `Manifest` — the introspection `.so`, the program link, an archive —
    /// sees one library configuration and cannot disagree about it.
    fn take_optional(&mut self, text: &str, repo_root: &Path) {
        self.optional_requires = json_array(text, "optional_requires")
            .iter()
            .map(|p| repo_root.join(p))
            .collect();
        // An empty list is not "nothing is missing, so enable it": a library
        // with no optional dependency declared has no optional configuration to
        // fold, and a feature macro defined for a dependency nobody named would
        // compile code against headers that are not there.
        self.optional_enabled =
            !self.optional_requires.is_empty() && self.optional_requires.iter().all(|p| p.exists());
        if !self.optional_enabled {
            return;
        }

        self.include_dirs.extend(
            json_array(text, "optional_include_dirs")
                .iter()
                .map(|p| repo_root.join(p)),
        );
        self.extra_sources.extend(
            json_array(text, "optional_extra_sources")
                .iter()
                .map(|p| repo_root.join(p)),
        );
        self.defines.extend(json_array(text, "optional_defines"));
        self.link_args.extend(
            json_array(text, "optional_link_args")
                .iter()
                .map(|a| absolutise(repo_root, a)),
        );
        let feature = json_string(text, "optional_feature");
        if !feature.is_empty() {
            self.defines.push(feature);
        }
    }

    /// Fail with the library's own hint if a prerequisite (e.g. a vendored
    /// dependency) is missing — a clear message beats a wall of linker errors.
    fn check_requirements(&self) -> Result<(), String> {
        for r in &self.requires {
            if !r.exists() {
                return Err(if self.requires_hint.is_empty() {
                    format!("missing prerequisite: {}", r.display())
                } else {
                    format!("{} (missing: {})", self.requires_hint, r.display())
                });
            }
        }
        Ok(())
    }
}

impl BuildConfig {
    fn merge(&mut self, m: &Manifest) {
        self.needs_cxx |= m.cxx;
        self.include_dirs.extend(m.include_dirs.iter().cloned());
        self.extra_sources.extend(m.extra_sources.iter().cloned());
        self.defines.extend(m.defines.iter().cloned());
        self.pkg_config.extend(m.pkg_config.iter().cloned());
        self.link_args.extend(m.link_args.iter().cloned()); // already absolutised
    }
}

/// Resolve a repo-relative `-L` path in a link argument to an absolute one.
fn absolutise(repo_root: &Path, arg: &str) -> String {
    if let Some(rest) = arg.strip_prefix("-L") {
        if !rest.starts_with('/') {
            return format!("-L{}", repo_root.join(rest).display());
        }
    }
    arg.to_string()
}

/// Run `pkg-config <mode> <packages...>` and return the flags.
pub fn pkg_config_flags(packages: &[String], mode: &str) -> Result<Vec<String>, String> {
    if packages.is_empty() {
        return Ok(Vec::new());
    }
    let out = Command::new("pkg-config")
        .arg(mode)
        .args(packages)
        .output()
        .map_err(|e| format!("pkg-config: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "pkg-config {mode} {} failed: {}",
            packages.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .map(str::to_string)
        .collect())
}

// --- minimal JSON readers for the fixed lib.json schema -------------------

fn json_field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let start = text.find(&needle)? + needle.len();
    let rest = text[start..].trim_start();
    rest.strip_prefix(':').map(|r| r.trim_start())
}

fn json_bool(text: &str, key: &str) -> bool {
    json_field(text, key).is_some_and(|r| r.starts_with("true"))
}

fn json_string(text: &str, key: &str) -> String {
    json_field(text, key)
        .and_then(|r| r.strip_prefix('"'))
        .and_then(|r| r.find('"').map(|e| r[..e].to_string()))
        .unwrap_or_default()
}

fn json_array(text: &str, key: &str) -> Vec<String> {
    let Some(rest) = json_field(text, key) else {
        return Vec::new();
    };
    let Some(rest) = rest.strip_prefix('[') else {
        return Vec::new();
    };
    let Some(end) = rest.find(']') else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut chars = rest[..end].chars().peekable();
    while let Some(c) = chars.next() {
        if c != '"' {
            continue;
        }
        let mut item = String::new();
        for c in chars.by_ref() {
            if c == '"' {
                break;
            }
            item.push(c);
        }
        out.push(item);
    }
    out
}
