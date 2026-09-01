//! Static type inference/checking for expressions — shared by the validator
//! (user-facing diagnostics) and the backend (op/return-type selection), so the
//! two never disagree about an expression's type.

use std::collections::HashMap;

use crate::{Expr, Registry, Signature, Ty};

/// Maps a form's component ids to their component type names. Component ids are
/// module-scoped (they are not locals), so every subroutine can see them.
pub type Components = HashMap<String, String>;

#[derive(Debug, Clone, PartialEq)]
pub struct SemaError {
    pub msg: String,
}
impl std::fmt::Display for SemaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}

fn err<T>(msg: impl Into<String>) -> Result<T, SemaError> {
    Err(SemaError { msg: msg.into() })
}

/// The static type of an integer literal: `int` if it fits in 32 bits, else
/// `int64`.  Keeps common `let x: int = 5` ergonomic while big constants still
/// typecheck.
pub fn int_literal_type(v: i64) -> Ty {
    if i32::try_from(v).is_ok() {
        Ty::Int
    } else {
        Ty::Int64
    }
}

/// Infer the type of `expr` under a variable environment and command registry.
pub fn type_of_expr(
    expr: &Expr,
    vars: &HashMap<String, Ty>,
    reg: &Registry,
) -> Result<Ty, SemaError> {
    type_of_expr_in(expr, vars, reg, &Components::new())
}

/// As `type_of_expr`, but with component ids in scope so `id.property` resolves.
pub fn type_of_expr_in(
    expr: &Expr,
    vars: &HashMap<String, Ty>,
    reg: &Registry,
    components: &Components,
) -> Result<Ty, SemaError> {
    match expr {
        Expr::IntLit(v) => Ok(int_literal_type(*v)),
        Expr::DoubleLit(_) => Ok(Ty::Double),
        Expr::TextLit(_) => Ok(Ty::Text),
        Expr::Var(name) => vars.get(name).copied().ok_or_else(|| SemaError {
            msg: format!("use of undefined variable `{name}`"),
        }),
        Expr::Bin(op, l, r) => {
            let lt = type_of_expr_in(l, vars, reg, components)?;
            let rt = type_of_expr_in(r, vars, reg, components)?;
            // `+` joins two texts. It is the same operation `concat` performs
            // — this spelling exists so that building a message does not nest
            // three calls deep.
            if lt == Ty::Text && rt == Ty::Text && *op == crate::BinOp::Add {
                return Ok(Ty::Text);
            }
            if !lt.is_numeric() {
                if lt == Ty::Text {
                    return err(format!(
                        "text supports `+` (joining) but not `{}`",
                        op.symbol()
                    ));
                }
                return err(format!(
                    "operator `{}` needs numeric operands, left is {}",
                    op.symbol(),
                    lt.as_str()
                ));
            }
            if lt != rt {
                return err(format!(
                    "operator `{}` needs matching operand types (no implicit conversion): {} vs {}",
                    op.symbol(),
                    lt.as_str(),
                    rt.as_str()
                ));
            }
            Ok(lt)
        }
        Expr::Call { cmd, args } => {
            let (what, sig) = callee(cmd, reg).ok_or_else(|| SemaError {
                msg: format!("unknown command `{cmd}`"),
            })?;
            let ret = sig.ret.ok_or_else(|| SemaError {
                msg: format!("{what} `{cmd}` returns nothing and cannot be used in an expression"),
            })?;
            check_args_labeled(what, cmd, &sig.params, args, vars, reg, components)?;
            Ok(ret)
        }
        Expr::GetProperty {
            component,
            property,
        } => property_type(component, property, reg, components),
        Expr::BoolLit(_) => Ok(Ty::Bool),
        Expr::Cmp(op, l, r) => {
            let lt = type_of_expr_in(l, vars, reg, components)?;
            let rt = type_of_expr_in(r, vars, reg, components)?;
            if lt != rt {
                return err(format!(
                    "cannot compare {} with {} using `{}` (no implicit conversion)",
                    lt.as_str(),
                    rt.as_str(),
                    op.symbol()
                ));
            }
            if op.is_ordering() && !lt.is_numeric() {
                return err(format!(
                    "`{}` compares numbers; {} values support only `=` and `<>`",
                    op.symbol(),
                    lt.as_str()
                ));
            }
            Ok(Ty::Bool)
        }
        Expr::Logical(op, l, r) => {
            let word = match op {
                crate::LogicalOp::And => "and",
                crate::LogicalOp::Or => "or",
            };
            for (side, e) in [("left", l), ("right", r)] {
                let t = type_of_expr_in(e, vars, reg, components)?;
                if t != Ty::Bool {
                    return err(format!(
                        "`{word}` needs a truth value; its {side} side is {}",
                        t.as_str()
                    ));
                }
            }
            Ok(Ty::Bool)
        }
        Expr::Neg(e) => {
            let t = type_of_expr_in(e, vars, reg, components)?;
            if !t.is_numeric() {
                return err(format!("`-` negates numbers, got {}", t.as_str()));
            }
            Ok(t)
        }
        Expr::Not(e) => {
            let t = type_of_expr_in(e, vars, reg, components)?;
            if t != Ty::Bool {
                return err(format!("`not` needs a truth value, got {}", t.as_str()));
            }
            Ok(Ty::Bool)
        }
    }
}

/// Resolve a called name to what it is and what it takes.
///
/// Library commands are looked up first, so a user subroutine can never change
/// the meaning of an existing command name — the validator has already rejected
/// that collision by the time anything is lowered.
pub fn callee(name: &str, reg: &Registry) -> Option<(&'static str, Signature)> {
    if let Some(c) = reg.get(name) {
        return Some(("command", c.sig.clone()));
    }
    reg.sub(name).map(|s| ("subroutine", s.clone()))
}

/// The declared type of `component.property`, or a diagnostic.
pub fn property_type(
    component: &str,
    property: &str,
    reg: &Registry,
    components: &Components,
) -> Result<Ty, SemaError> {
    let Some(type_name) = components.get(component) else {
        return err(format!(
            "unknown component `{component}` (component ids come from the form)"
        ));
    };
    let Some(desc) = reg.component(type_name) else {
        return err(format!(
            "component `{component}` has unknown type `{type_name}`"
        ));
    };
    match desc.property(property) {
        Some(p) => Ok(p.ty),
        None => {
            let mut known: Vec<&str> = desc.properties.iter().map(|p| p.name.as_str()).collect();
            known.sort_unstable();
            err(format!(
                "`{type_name}` has no property `{property}` (has: {})",
                known.join(", ")
            ))
        }
    }
}

/// Check an argument list against expected parameter types.
pub fn check_args(
    cmd: &str,
    params: &[Ty],
    args: &[Expr],
    vars: &HashMap<String, Ty>,
    reg: &Registry,
) -> Result<(), SemaError> {
    check_args_in(cmd, params, args, vars, reg, &Components::new())
}

pub fn check_args_in(
    cmd: &str,
    params: &[Ty],
    args: &[Expr],
    vars: &HashMap<String, Ty>,
    reg: &Registry,
    components: &Components,
) -> Result<(), SemaError> {
    check_args_labeled("command", cmd, params, args, vars, reg, components)
}

/// As `check_args_in`, but says whether the callee is a command or a
/// subroutine — "subroutine `add` expects 2 argument(s)" is the message the
/// author of `add` needs to read.
#[allow(clippy::too_many_arguments)]
pub fn check_args_labeled(
    what: &str,
    cmd: &str,
    params: &[Ty],
    args: &[Expr],
    vars: &HashMap<String, Ty>,
    reg: &Registry,
    components: &Components,
) -> Result<(), SemaError> {
    if args.len() != params.len() {
        return err(format!(
            "{what} `{cmd}` expects {} argument(s), got {}",
            params.len(),
            args.len()
        ));
    }
    for (i, (a, expected)) in args.iter().zip(params).enumerate() {
        let got = type_of_expr_in(a, vars, reg, components)?;
        if got != *expected {
            return err(format!(
                "{what} `{cmd}` argument {} expects {}, got {}",
                i + 1,
                expected.as_str(),
                got.as_str()
            ));
        }
    }
    Ok(())
}
