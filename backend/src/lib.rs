//! OpenEPL backend (Phase 0): typed IR -> textual LLVM IR (`.ll`).
//!
//! This is the BlackMoon model, deferred one layer for the spike: we emit
//! standard LLVM IR text and let the `clang` driver assemble it to an object
//! file and link it with the system linker (see `openepl-cli`).  No inkwell /
//! `llvm-config` dependency yet — that's the eventual Q1/D6 upgrade, and this
//! `.ll` boundary is exactly where it slots in.
//!
//! The program entry is `ECodeStart` (PRD §1.4 lean-entry model); the C runtime
//! supplies `main`, which calls `E_Init()` then `ECodeStart()`.

use std::collections::HashMap;
use std::fmt::Write as _;

use openepl_ir::{BinOp, Expr, Item, Module, Stmt, Ty};

/// A runtime command the backend knows how to lower.  v0 hard-codes the two the
/// slice uses; Phase 2 replaces this table with signatures loaded from the
/// support-library ABI (`openepl_get_lib_info`).
struct CmdSig {
    /// LLVM function name in the runtime.
    llvm_name: &'static str,
    /// Parameter types (by slot).
    params: &'static [Ty],
}

fn command_table() -> HashMap<&'static str, CmdSig> {
    let mut m = HashMap::new();
    m.insert("print_int", CmdSig { llvm_name: "oe_print_int", params: &[Ty::Int] });
    m.insert("print_text", CmdSig { llvm_name: "oe_print_text", params: &[Ty::Text] });
    m
}

#[derive(Debug, Clone, PartialEq)]
pub struct LowerError {
    pub msg: String,
}
impl std::fmt::Display for LowerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "lowering error: {}", self.msg)
    }
}
impl std::error::Error for LowerError {}

fn err<T>(msg: impl Into<String>) -> Result<T, LowerError> {
    Err(LowerError { msg: msg.into() })
}

/// Lower a whole module to a `.ll` string.
pub fn lower_module(m: &Module) -> Result<String, LowerError> {
    let cmds = command_table();

    // Collect subs; require exactly one `main` for the spike.
    let subs: Vec<_> = m.subs().collect();
    if !subs.iter().any(|s| s.name == "main") {
        return err("module has no `main` subroutine");
    }
    if subs.len() != 1 {
        return err("v0 supports exactly one subroutine (`main`)");
    }

    let mut lo = Lowerer {
        cmds,
        strings: Vec::new(),
        body: String::new(),
        vars: HashMap::new(),
        tmp: 0,
    };

    for item in &m.items {
        let Item::Sub(sub) = item;
        for stmt in &sub.body {
            lo.stmt(stmt)?;
        }
    }

    Ok(lo.finish(&m.name))
}

/// The LLVM type / operand form a value flows as.
#[derive(Clone)]
enum Val {
    /// An `i32` operand (literal or SSA temp), e.g. `"42"` or `"%t3"`.
    Int(String),
    /// A `ptr` operand — an inline `getelementptr` to a string global.
    Text(String),
}

impl Val {
    fn ty(&self) -> Ty {
        match self {
            Val::Int(_) => Ty::Int,
            Val::Text(_) => Ty::Text,
        }
    }
    fn operand(&self) -> &str {
        match self {
            Val::Int(s) | Val::Text(s) => s,
        }
    }
}

struct Lowerer {
    cmds: HashMap<&'static str, CmdSig>,
    /// Decoded string-literal payloads; index is the global id.
    strings: Vec<String>,
    /// Instruction lines for `ECodeStart` (without the surrounding define).
    body: String,
    /// let-bound locals -> their value form.
    vars: HashMap<String, Val>,
    /// SSA temp counter.
    tmp: usize,
}

impl Lowerer {
    fn fresh(&mut self) -> String {
        let t = format!("%t{}", self.tmp);
        self.tmp += 1;
        t
    }

    fn stmt(&mut self, s: &Stmt) -> Result<(), LowerError> {
        match s {
            Stmt::Let { name, ty, value } => {
                let v = self.eval(value)?;
                if v.ty() != *ty {
                    return err(format!(
                        "type mismatch in `let {name}`: declared {}, expression is {}",
                        ty.as_str(),
                        v.ty().as_str()
                    ));
                }
                self.vars.insert(name.clone(), v);
                Ok(())
            }
            Stmt::Call { cmd, args } => {
                let sig = match self.cmds.get(cmd.as_str()) {
                    Some(s) => CmdSig { llvm_name: s.llvm_name, params: s.params },
                    None => return err(format!("unknown command `{cmd}`")),
                };
                if args.len() != sig.params.len() {
                    return err(format!(
                        "command `{cmd}` expects {} argument(s), got {}",
                        sig.params.len(),
                        args.len()
                    ));
                }
                let mut ll_args = Vec::new();
                for (i, a) in args.iter().enumerate() {
                    let v = self.eval(a)?;
                    if v.ty() != sig.params[i] {
                        return err(format!(
                            "command `{cmd}` argument {} expects {}, got {}",
                            i + 1,
                            sig.params[i].as_str(),
                            v.ty().as_str()
                        ));
                    }
                    ll_args.push(match v {
                        Val::Int(s) => format!("i32 {s}"),
                        Val::Text(s) => format!("ptr {s}"),
                    });
                }
                writeln!(
                    self.body,
                    "  call void @{}({})",
                    sig.llvm_name,
                    ll_args.join(", ")
                )
                .unwrap();
                Ok(())
            }
        }
    }

    fn eval(&mut self, e: &Expr) -> Result<Val, LowerError> {
        match e {
            Expr::IntLit(v) => {
                let v32 = i32::try_from(*v)
                    .map_err(|_| LowerError { msg: format!("integer literal {v} does not fit in 32-bit SDT_INT") })?;
                Ok(Val::Int(v32.to_string()))
            }
            Expr::TextLit(s) => {
                let id = self.strings.len();
                self.strings.push(s.clone());
                let bytes = s.len() + 1; // +1 for the NUL we append in the global
                Ok(Val::Text(format!(
                    "getelementptr inbounds ([{bytes} x i8], ptr @.str{id}, i64 0, i64 0)"
                )))
            }
            Expr::Var(name) => self
                .vars
                .get(name)
                .cloned()
                .ok_or_else(|| LowerError { msg: format!("use of undefined variable `{name}`") }),
            Expr::Bin(op, l, r) => {
                let lv = self.eval(l)?;
                let rv = self.eval(r)?;
                if lv.ty() != Ty::Int || rv.ty() != Ty::Int {
                    return err("arithmetic operators require integer operands");
                }
                let opcode = match op {
                    BinOp::Add => "add",
                    BinOp::Sub => "sub",
                    BinOp::Mul => "mul",
                    BinOp::Div => "sdiv",
                };
                let t = self.fresh();
                writeln!(
                    self.body,
                    "  {t} = {opcode} i32 {}, {}",
                    lv.operand(),
                    rv.operand()
                )
                .unwrap();
                Ok(Val::Int(t))
            }
        }
    }

    fn finish(self, module_name: &str) -> String {
        let mut out = String::new();
        writeln!(out, "; OpenEPL-generated LLVM IR — module `{module_name}` (Phase 0)").unwrap();
        writeln!(out, "; Do not edit; regenerate from the .oir source.\n").unwrap();

        // String globals.
        for (id, s) in self.strings.iter().enumerate() {
            let encoded = encode_llvm_string(s);
            let bytes = s.len() + 1;
            writeln!(
                out,
                "@.str{id} = private unnamed_addr constant [{bytes} x i8] c\"{encoded}\\00\""
            )
            .unwrap();
        }
        if !self.strings.is_empty() {
            out.push('\n');
        }

        // Runtime declarations (only what we reference; keeps the file lean).
        writeln!(out, "declare void @oe_print_int(i32)").unwrap();
        writeln!(out, "declare void @oe_print_text(ptr)\n").unwrap();

        // Entry: ECodeStart (PRD §1.4).  The C runtime provides `main`.
        writeln!(out, "define i32 @ECodeStart() {{").unwrap();
        writeln!(out, "entry:").unwrap();
        out.push_str(&self.body);
        writeln!(out, "  ret i32 0").unwrap();
        writeln!(out, "}}").unwrap();
        out
    }
}

/// Encode a byte string as an LLVM string-constant body (`\XX` hex for anything
/// outside printable ASCII, plus the required escapes for `"` and `\`).
fn encode_llvm_string(s: &str) -> String {
    let mut out = String::new();
    for &b in s.as_bytes() {
        match b {
            b'"' | b'\\' => {
                write!(out, "\\{:02X}", b).unwrap();
            }
            0x20..=0x7E => out.push(b as char),
            _ => {
                write!(out, "\\{:02X}", b).unwrap();
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use openepl_ir::parse;

    #[test]
    fn lowers_arith_and_text() {
        let src = r#"
module demo
sub main
  let x: int = 6 * 7
  let msg: text = "answer:"
  call print_text(msg)
  call print_int(x)
end
"#;
        let m = parse(src).unwrap();
        let ll = lower_module(&m).unwrap();
        assert!(ll.contains("define i32 @ECodeStart()"));
        assert!(ll.contains("mul i32 6, 7"));
        assert!(ll.contains("@oe_print_text"));
        assert!(ll.contains("answer:"));
    }

    #[test]
    fn rejects_type_mismatch() {
        let m = parse("module m\nsub main\n  let x: int = \"nope\"\nend\n").unwrap();
        assert!(lower_module(&m).is_err());
    }

    #[test]
    fn rejects_unknown_command() {
        let m = parse("module m\nsub main\n  call frobnicate(1)\nend\n").unwrap();
        assert!(lower_module(&m).is_err());
    }
}
