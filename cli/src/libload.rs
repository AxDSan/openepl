//! Build-time support-library introspection (PRD §5.4).
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
//! dead-stripping (PRD D3/G8; ADR 0003).
//!
//! `dlopen` runs the target library in-process, so this native path assumes
//! host == target (x86_64-linux). Cross-compilation needs a sidecar manifest
//! instead (Phase 4).

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::process::Command;

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
struct LibInfoC {
    abi_version: i32,
    name: *const c_char,
    guid: *const c_char,
    ver_major: i32,
    ver_minor: i32,
    ver_build: i32,
    command_count: i32,
    commands: *const CommandDescC,
}

const OPENEPL_ABI_VERSION: i32 = 1;

/// The result of resolving a module's libraries.
pub struct LibPlan {
    /// Combined command registry across all loaded libraries.
    pub registry: Registry,
    /// Implementation sources to static-link into the program.
    pub impl_sources: Vec<PathBuf>,
}

/// Resolve `core` + each `use`d library under `repo_root`.
pub fn load(repo_root: &Path, uses: &[String]) -> Result<LibPlan, String> {
    let mut registry = Registry::new();
    let mut impl_sources = Vec::new();

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
        let sources = c_sources(dir)?;
        // Introspection .so: everything except the program entry.
        let so_srcs: Vec<&PathBuf> = sources
            .iter()
            .filter(|p| filename(p) != "oe_start.c")
            .collect();
        // Impl sources to link: everything except the metadata TU.
        for p in &sources {
            if !filename(p).ends_with("_libinfo.c") {
                impl_sources.push(p.clone());
            }
        }

        let so_path = build_introspection_so(repo_root, name, &so_srcs)?;
        introspect_into(&so_path, name, &mut registry)?;
    }

    Ok(LibPlan {
        registry,
        impl_sources,
    })
}

fn filename(p: &Path) -> String {
    p.file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string()
}

/// All `.c` files directly in `dir`, sorted for determinism.
fn c_sources(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))? {
        let p = entry.map_err(|e| e.to_string())?.path();
        if p.extension().and_then(|s| s.to_str()) == Some("c") {
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
    sources: &[&PathBuf],
) -> Result<PathBuf, String> {
    let build_dir = std::env::temp_dir().join("openepl-build");
    std::fs::create_dir_all(&build_dir).map_err(|e| e.to_string())?;
    let so_path = build_dir.join(format!("lib{name}_introspect_{}.so", std::process::id()));

    let mut cmd = Command::new("clang");
    cmd.arg("-shared")
        .arg("-fPIC")
        .arg("-I")
        .arg(repo_root.join("abi"))
        .arg("-I")
        .arg(repo_root.join("runtime")); // for openepl_core.h
    for s in sources {
        cmd.arg(s);
    }
    cmd.arg("-lm").arg("-o").arg(&so_path);

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
        if info.abi_version != OPENEPL_ABI_VERSION {
            dlclose(handle);
            return Err(format!(
                "library `{name}` ABI version {} != {OPENEPL_ABI_VERSION}",
                info.abi_version
            ));
        }

        let result = register_commands(info, name, registry);
        dlclose(handle);
        result
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
    /// silently drift from the C source of truth (ADR 0003).
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
    }
}
