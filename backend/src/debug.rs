//! Debug information: the LLVM metadata that makes a built program one a
//! debugger can step through.
//!
//! The backend writes LLVM IR as text rather than through LLVM's own API, so
//! `-g` on the clang line buys nothing — there is no front end for it to ask.
//! The metadata has to be written into the IR by hand, and this is where it is
//! written.
//!
//! What is emitted here is a *line table* and nothing more: which machine
//! address corresponds to which line of which `.oir` file. That is what
//! stepping and breakpoints need. Local variables need `!DILocalVariable` and
//! `#dbg_declare` beside each `alloca`, and they are a later change; the
//! compile unit says `LineTablesOnly` so that LLVM knows this is deliberate
//! rather than a half-written type graph.
//!
//! Only user subroutines carry debug information. The functions the compiler
//! synthesises — the entry point, a library's initialiser, the export
//! wrappers, the event thunks — deliberately carry none, because a function
//! that *has* debug information must give every call inside it a location, and
//! there is no line in anyone's source to give.

use std::collections::HashMap;
use std::fmt::Write as _;

/// The four nodes every module needs, before anything specific to it.
const CU: usize = 0;
const FILE: usize = 1;
const DWARF_VERSION: usize = 2;
const DEBUG_INFO_VERSION: usize = 3;
/// The shared `!DISubroutineType`, which a line table does not describe.
const SUB_TYPE: usize = 4;
const SUB_TYPE_LIST: usize = 5;
/// The first node number free for subprograms and locations.
const FIRST_FREE: usize = 6;

/// Builds the metadata block a module ends with.
pub(crate) struct DebugInfo {
    filename: String,
    directory: String,
    producer: String,
    /// Node text by number, from `FIRST_FREE` upwards.
    nodes: Vec<String>,
    /// `(scope, line, column)` -> node, so a statement lowered into several
    /// instructions names one location rather than one per instruction.
    locations: HashMap<(usize, usize, usize), usize>,
}

impl DebugInfo {
    /// `path` is the source as the user named it. It is split into a file and
    /// a directory because DWARF stores them separately, and a debugger that
    /// is handed a bare name cannot find the file to show.
    pub(crate) fn new(path: &str, producer: &str) -> Self {
        let (directory, filename) = match path.rfind('/') {
            Some(i) => (path[..i].to_string(), path[i + 1..].to_string()),
            None => (".".to_string(), path.to_string()),
        };
        DebugInfo {
            filename,
            directory,
            producer: producer.to_string(),
            nodes: Vec::new(),
            locations: HashMap::new(),
        }
    }

    fn push(&mut self, text: String) -> usize {
        self.nodes.push(text);
        FIRST_FREE + self.nodes.len() - 1
    }

    /// Declare a subroutine and return the node to name on its `define` line.
    ///
    /// `line` is where the `sub` keyword is. `scopeLine` is the same: OpenEPL
    /// has no separate opening brace for a debugger to step to.
    pub(crate) fn subprogram(&mut self, name: &str, symbol: &str, line: usize) -> usize {
        let line = line.max(1);
        self.push(format!(
            "distinct !DISubprogram(name: \"{name}\", linkageName: \"{symbol}\", \
             scope: !{FILE}, file: !{FILE}, line: {line}, type: !{SUB_TYPE}, \
             scopeLine: {line}, spFlags: DISPFlagDefinition, unit: !{CU})"
        ))
    }

    /// The node for a source position inside `scope`.
    ///
    /// A line of 0 means the position was lost somewhere between the parser
    /// and here. Rather than emit `line: 0` — which a debugger reads as "no
    /// line", stepping straight past it — the caller is expected to pass a
    /// line it does have; this only guards against a zero slipping through.
    pub(crate) fn location(&mut self, scope: usize, line: usize, column: usize) -> usize {
        let line = line.max(1);
        let key = (scope, line, column);
        if let Some(n) = self.locations.get(&key) {
            return *n;
        }
        let n = self.push(format!(
            "!DILocation(line: {line}, column: {column}, scope: !{scope})"
        ));
        self.locations.insert(key, n);
        n
    }

    /// Whether anything was declared. A module with no user subroutines gets
    /// no metadata block at all rather than an empty compile unit.
    pub(crate) fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The block appended to the module, after every function.
    pub(crate) fn render(&self) -> String {
        let mut out = String::new();
        out.push('\n');
        writeln!(out, "!llvm.dbg.cu = !{{!{CU}}}").unwrap();
        writeln!(
            out,
            "!llvm.module.flags = !{{!{DWARF_VERSION}, !{DEBUG_INFO_VERSION}}}"
        )
        .unwrap();
        // DW_LANG_C99 is a deliberate lie of convenience: DWARF has no
        // language code for OpenEPL, and every debugger knows what to do with
        // C99's basic types and scoping. Nothing here depends on it.
        writeln!(
            out,
            "!{CU} = distinct !DICompileUnit(language: DW_LANG_C99, file: !{FILE}, \
             producer: \"{}\", isOptimized: false, runtimeVersion: 0, \
             emissionKind: LineTablesOnly)",
            self.producer
        )
        .unwrap();
        writeln!(
            out,
            "!{FILE} = !DIFile(filename: \"{}\", directory: \"{}\")",
            escape(&self.filename),
            escape(&self.directory)
        )
        .unwrap();
        writeln!(out, "!{DWARF_VERSION} = !{{i32 7, !\"Dwarf Version\", i32 5}}").unwrap();
        writeln!(
            out,
            "!{DEBUG_INFO_VERSION} = !{{i32 2, !\"Debug Info Version\", i32 3}}"
        )
        .unwrap();
        writeln!(out, "!{SUB_TYPE} = !DISubroutineType(types: !{SUB_TYPE_LIST})").unwrap();
        writeln!(out, "!{SUB_TYPE_LIST} = !{{null}}").unwrap();
        for (i, node) in self.nodes.iter().enumerate() {
            writeln!(out, "!{} = {node}", FIRST_FREE + i).unwrap();
        }
        out
    }
}

/// A path as an LLVM metadata string. Backslashes and quotes are the two
/// characters that would end the string early — on Windows the first is what
/// every path is made of.
fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\22")
}
