//! OpenEPL backend (Phase 1): typed IR -> textual LLVM IR (`.ll`).
//!
//! BlackMoon model, deferred one layer: emit standard LLVM IR text and let the
//! `clang` driver assemble + link it against the C runtime (PRD D1/§5.2).  The
//! `.ll` boundary is exactly where an in-process `inkwell` backend slots in
//! later (ADR 0001).
//!
//! Assumes the module has already passed `openepl_ir::validate`; it re-derives
//! types locally (tracking a `Val`'s type as it lowers) but does not re-issue
//! user diagnostics.  Program entry is `ECodeStart` (PRD §1.4); the C runtime
//! supplies `main`.

use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;

use openepl_ir::{BinOp, Expr, Item, Module, Registry, Ty};

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

/// The LLVM type spelling for an IR slot type.
fn llvm_ty(t: Ty) -> &'static str {
    match t {
        Ty::Int => "i32",
        Ty::Int64 => "i64",
        Ty::Double => "double",
        Ty::Text => "ptr",
    }
}

/// Lower a whole module to a `.ll` string using the given command registry.
pub fn lower_module(m: &Module, reg: &Registry) -> Result<String, LowerError> {
    let subs: Vec<_> = m.subs().collect();
    if !subs.iter().any(|s| s.name == "main") {
        return err("module has no `main` subroutine");
    }

    let mut lo = Lowerer {
        reg,
        strings: Vec::new(),
        body: String::new(),
        vars: HashMap::new(),
        used: BTreeSet::new(),
        tmp: 0,
    };

    for item in &m.items {
        let Item::Sub(sub) = item;
        if sub.name != "main" {
            return err("v0.1 lowers only `main`");
        }
        for stmt in &sub.body {
            lo.stmt(stmt)?;
        }
    }

    Ok(lo.finish(&m.name))
}

/// A lowered value: its slot type plus its LLVM operand (a literal or `%tN`).
#[derive(Clone)]
struct Val {
    ty: Ty,
    operand: String,
}

impl Val {
    /// The `"<type> <operand>"` form used in call arguments and instructions.
    fn typed(&self) -> String {
        format!("{} {}", llvm_ty(self.ty), self.operand)
    }
}

struct Lowerer<'a> {
    reg: &'a Registry,
    strings: Vec<String>,
    body: String,
    vars: HashMap<String, Val>,
    /// Runtime symbols actually referenced (drives declarations + keeps the
    /// emitted `.ll` lean).
    used: BTreeSet<(&'a str, String)>,
    tmp: usize,
}

impl<'a> Lowerer<'a> {
    fn fresh(&mut self) -> String {
        let t = format!("%t{}", self.tmp);
        self.tmp += 1;
        t
    }

    fn stmt(&mut self, s: &openepl_ir::Stmt) -> Result<(), LowerError> {
        use openepl_ir::Stmt;
        match s {
            Stmt::Let { name, ty, value } => {
                let v = self.eval(value)?;
                if v.ty != *ty {
                    return err(format!(
                        "type mismatch in `let {name}`: declared {}, expression is {}",
                        ty.as_str(),
                        v.ty.as_str()
                    ));
                }
                self.vars.insert(name.clone(), v);
                Ok(())
            }
            Stmt::Call { cmd, args } => {
                self.eval_call(cmd, args)?; // return value (if any) discarded
                Ok(())
            }
        }
    }

    fn eval(&mut self, e: &Expr) -> Result<Val, LowerError> {
        match e {
            Expr::IntLit(v) => {
                // Match sema: fits i32 -> int, else int64.
                if let Ok(v32) = i32::try_from(*v) {
                    Ok(Val { ty: Ty::Int, operand: v32.to_string() })
                } else {
                    Ok(Val { ty: Ty::Int64, operand: v.to_string() })
                }
            }
            Expr::DoubleLit(v) => {
                // Emit the exact IEEE-754 bits; LLVM parses `0x<16 hex>` as a
                // double, avoiding decimal round-trip ambiguity.
                Ok(Val { ty: Ty::Double, operand: format!("0x{:016X}", v.to_bits()) })
            }
            Expr::TextLit(s) => {
                let id = self.strings.len();
                self.strings.push(s.clone());
                let bytes = s.len() + 1;
                Ok(Val {
                    ty: Ty::Text,
                    operand: format!(
                        "getelementptr inbounds ([{bytes} x i8], ptr @.str{id}, i64 0, i64 0)"
                    ),
                })
            }
            Expr::Var(name) => self
                .vars
                .get(name)
                .cloned()
                .ok_or_else(|| LowerError { msg: format!("use of undefined variable `{name}`") }),
            Expr::Bin(op, l, r) => {
                let lv = self.eval(l)?;
                let rv = self.eval(r)?;
                if lv.ty != rv.ty || !lv.ty.is_numeric() {
                    return err("arithmetic requires matching numeric operands");
                }
                let opcode = match (op, lv.ty) {
                    (BinOp::Add, Ty::Double) => "fadd",
                    (BinOp::Sub, Ty::Double) => "fsub",
                    (BinOp::Mul, Ty::Double) => "fmul",
                    (BinOp::Div, Ty::Double) => "fdiv",
                    (BinOp::Add, _) => "add",
                    (BinOp::Sub, _) => "sub",
                    (BinOp::Mul, _) => "mul",
                    (BinOp::Div, _) => "sdiv",
                };
                let t = self.fresh();
                writeln!(
                    self.body,
                    "  {t} = {opcode} {} {}, {}",
                    llvm_ty(lv.ty),
                    lv.operand,
                    rv.operand
                )
                .unwrap();
                Ok(Val { ty: lv.ty, operand: t })
            }
            Expr::Call { cmd, args } => {
                let v = self.eval_call(cmd, args)?;
                v.ok_or_else(|| LowerError {
                    msg: format!("command `{cmd}` returns nothing and cannot be used as a value"),
                })
            }
        }
    }

    /// Lower a command call; returns the result `Val` if the command is non-void.
    fn eval_call(&mut self, cmd: &str, args: &[Expr]) -> Result<Option<Val>, LowerError> {
        let command = self
            .reg
            .get(cmd)
            .ok_or_else(|| LowerError { msg: format!("unknown command `{cmd}`") })?;
        let sig = command.sig.clone();
        let symbol = command.symbol;

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
            if v.ty != sig.params[i] {
                return err(format!(
                    "command `{cmd}` argument {} expects {}, got {}",
                    i + 1,
                    sig.params[i].as_str(),
                    v.ty.as_str()
                ));
            }
            ll_args.push(v.typed());
        }

        // Record the declaration signature for the prologue.
        let param_tys: Vec<&str> = sig.params.iter().map(|t| llvm_ty(*t)).collect();
        let ret_ty = sig.ret.map(llvm_ty).unwrap_or("void");
        let decl = format!("declare {ret_ty} @{symbol}({})", param_tys.join(", "));
        self.used.insert((symbol, decl));

        match sig.ret {
            None => {
                writeln!(self.body, "  call void @{symbol}({})", ll_args.join(", ")).unwrap();
                Ok(None)
            }
            Some(rt) => {
                let t = self.fresh();
                writeln!(
                    self.body,
                    "  {t} = call {} @{symbol}({})",
                    llvm_ty(rt),
                    ll_args.join(", ")
                )
                .unwrap();
                Ok(Some(Val { ty: rt, operand: t }))
            }
        }
    }

    fn finish(self, module_name: &str) -> String {
        let mut out = String::new();
        writeln!(out, "; OpenEPL-generated LLVM IR — module `{module_name}` (Phase 1)").unwrap();
        writeln!(out, "; Do not edit; regenerate from the .oir source.\n").unwrap();

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

        // Runtime declarations — only referenced symbols (one per used command).
        for (_sym, decl) in &self.used {
            writeln!(out, "{decl}").unwrap();
        }
        if !self.used.is_empty() {
            out.push('\n');
        }

        writeln!(out, "define i32 @ECodeStart() {{").unwrap();
        writeln!(out, "entry:").unwrap();
        out.push_str(&self.body);
        writeln!(out, "  ret i32 0").unwrap();
        writeln!(out, "}}").unwrap();
        out
    }
}

/// Encode a byte string as an LLVM string-constant body.
fn encode_llvm_string(s: &str) -> String {
    let mut out = String::new();
    for &b in s.as_bytes() {
        match b {
            b'"' | b'\\' => write!(out, "\\{:02X}", b).unwrap(),
            0x20..=0x7E => out.push(b as char),
            _ => write!(out, "\\{:02X}", b).unwrap(),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use openepl_ir::parse;

    fn lower(src: &str) -> Result<String, LowerError> {
        let m = parse(src).unwrap();
        lower_module(&m, &Registry::core())
    }

    #[test]
    fn lowers_call_expr_and_double() {
        let ll = lower(
            "module m\nsub main\n  let r: double = sqrt(2.0)\n  call print_double(r)\n  let n: int = length(\"hi\")\n  call print_int(n)\nend\n",
        )
        .unwrap();
        assert!(ll.contains("call double @oe_sqrt(double 0x"));
        assert!(ll.contains("call i32 @oe_length(ptr"));
        assert!(ll.contains("declare double @oe_sqrt(double)"));
    }

    #[test]
    fn int64_arithmetic() {
        let ll = lower(
            "module m\nsub main\n  let a: int64 = int_to_int64(5)\n  let b: int64 = a + a\n  call print_int64(b)\nend\n",
        )
        .unwrap();
        assert!(ll.contains("add i64"));
    }

    #[test]
    fn only_used_commands_declared() {
        let ll = lower("module m\nsub main\n  call print_int(1)\nend\n").unwrap();
        assert!(ll.contains("declare void @oe_print_int(i32)"));
        assert!(!ll.contains("oe_sqrt"), "unused command leaked into IR");
    }
}
