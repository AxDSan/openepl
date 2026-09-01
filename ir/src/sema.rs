//! Static type inference/checking for expressions — shared by the validator
//! (user-facing diagnostics) and the backend (op/return-type selection), so the
//! two never disagree about an expression's type.

use std::collections::HashMap;

use crate::{Elem, Expr, Registry, Signature, Ty};

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
    type_of_expr_hinted(expr, None, vars, reg, components)
}

/// As `type_of_expr_in`, but told what type the surrounding context wants.
///
/// Only `[]` needs this: an empty list has no element to infer from, and the
/// alternative — a construction command per element type — would make the
/// commonest line in a program (`var lines: text[] = []`) the ugliest. The hint
/// is never used to *coerce*, only to give an otherwise-typeless literal the
/// type its destination already declares, so nothing else changes meaning.
pub fn type_of_expr_hinted(
    expr: &Expr,
    hint: Option<Ty>,
    vars: &HashMap<String, Ty>,
    reg: &Registry,
    components: &Components,
) -> Result<Ty, SemaError> {
    match expr {
        Expr::ArrayLit(items) if items.is_empty() => match hint {
            Some(Ty::Array(e)) => Ok(Ty::Array(e)),
            _ => err(
                "`[]` on its own does not say what it holds — declare the type, \
                 as in `var xs: text[] = []`",
            ),
        },
        _ => type_of_expr_bare(expr, vars, reg, components),
    }
}

fn type_of_expr_bare(
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
            if sig.ret.is_none() {
                return err(format!(
                    "{what} `{cmd}` returns nothing and cannot be used in an expression"
                ));
            }
            match check_call(what, cmd, &sig, args, vars, reg, components)? {
                Some(t) => Ok(t),
                None => err(format!(
                    "{what} `{cmd}` returns nothing and cannot be used in an expression"
                )),
            }
        }
        Expr::ArrayLit(items) => {
            let mut ty: Option<Ty> = None;
            for (i, item) in items.iter().enumerate() {
                let got = type_of_expr_bare(item, vars, reg, components)?;
                match ty {
                    None => {
                        if Elem::from_ty(got).is_none() {
                            return err(format!(
                                "a list cannot hold {} values",
                                got.as_str()
                            ));
                        }
                        ty = Some(got);
                    }
                    Some(first) if first != got => {
                        return err(format!(
                            "every element of a list has one type: element 1 is {}, \
                             element {} is {}",
                            first.as_str(),
                            i + 1,
                            got.as_str()
                        ))
                    }
                    Some(_) => {}
                }
            }
            // The empty case is handled by `type_of_expr_hinted`; reaching here
            // means nothing said what it should hold.
            match ty.and_then(Elem::from_ty) {
                Some(e) => Ok(Ty::Array(e)),
                None => err(
                    "`[]` on its own does not say what it holds — declare the type, \
                     as in `var xs: text[] = []`",
                ),
            }
        }
        Expr::Index { base, index } => {
            let bt = type_of_expr_bare(base, vars, reg, components)?;
            let it = type_of_expr_bare(index, vars, reg, components)?;
            if it != Ty::Int {
                return err(format!(
                    "an index counts with `int` values, got {}",
                    it.as_str()
                ));
            }
            // A constant index that is out of range is a bug the program does
            // not need to run to reveal. Only the cases visible here are
            // caught; the rest is the run-time bounds check.
            if let Expr::IntLit(v) = **index {
                if v < 0 {
                    return err(format!("index {v} is before the start of the list"));
                }
                if let Expr::ArrayLit(items) = &**base {
                    if v as usize >= items.len() {
                        return err(format!(
                            "index {v} is past the end of a list of {} element(s)",
                            items.len()
                        ));
                    }
                }
            }
            match bt {
                Ty::Array(e) => Ok(e.ty()),
                // One byte reads as a number, because that is what a byte is
                // once it is out of the byte-set.
                Ty::Bytes => Ok(Ty::Int),
                other => err(format!("{} is not something you can index", other.as_str())),
            }
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
    let sig = Signature {
        params: params.to_vec(),
        ret: None,
    };
    check_call(what, cmd, &sig, args, vars, reg, components).map(|_| ())
}

/// Check an argument list and report what the call's result type actually is.
///
/// The result is not always `sig.ret`: the array commands are declared over
/// `AnyArray`/`AnyElem`, so what `append` gives back depends on what it was
/// given. Resolving that here, once, is what keeps the validator and the
/// backend from disagreeing about it.
#[allow(clippy::too_many_arguments)]
pub fn check_call(
    what: &str,
    cmd: &str,
    sig: &Signature,
    args: &[Expr],
    vars: &HashMap<String, Ty>,
    reg: &Registry,
    components: &Components,
) -> Result<Option<Ty>, SemaError> {
    if args.len() != sig.params.len() {
        return err(format!(
            "{what} `{cmd}` expects {} argument(s), got {}",
            sig.params.len(),
            args.len()
        ));
    }
    let mut arg_tys = Vec::with_capacity(args.len());
    // What the array argument turned out to hold; every later `AnyElem` is
    // measured against it, which is how mixing element types is caught.
    let mut elem: Option<Elem> = None;
    for (i, (a, expected)) in args.iter().zip(&sig.params).enumerate() {
        let hint = match expected {
            Ty::AnyArray => None,
            Ty::AnyElem => elem.map(Elem::ty),
            t => Some(*t),
        };
        let got = type_of_expr_hinted(a, hint, vars, reg, components)?;
        match expected {
            Ty::AnyArray => match got.elem() {
                Some(e) => elem = Some(e),
                None => {
                    return err(format!(
                        "{what} `{cmd}` argument {} expects an array, got {}",
                        i + 1,
                        got.as_str()
                    ))
                }
            },
            Ty::AnyElem => {
                let Some(e) = elem else {
                    return err(format!(
                        "{what} `{cmd}` argument {} has no array to take its type from",
                        i + 1
                    ));
                };
                if got != e.ty() {
                    return err(format!(
                        "{what} `{cmd}` argument {} must match what the array holds \
                         ({}), got {}",
                        i + 1,
                        e.as_str(),
                        got.as_str()
                    ));
                }
            }
            t => {
                if got != *t {
                    return err(format!(
                        "{what} `{cmd}` argument {} expects {}, got {}",
                        i + 1,
                        t.as_str(),
                        got.as_str()
                    ));
                }
            }
        }
        arg_tys.push(got);
    }
    Ok(resolve_ret(sig, &arg_tys))
}

/// The concrete type a call yields, given what its arguments turned out to be.
///
/// `None` for a void command. Shared with the backend, which knows its
/// arguments' types but not the expressions they came from.
pub fn resolve_ret(sig: &Signature, arg_tys: &[Ty]) -> Option<Ty> {
    let ret = sig.ret?;
    let elem = sig
        .params
        .iter()
        .zip(arg_tys)
        .find_map(|(p, a)| match p {
            Ty::AnyArray => a.elem(),
            _ => None,
        });
    Some(match (ret, elem) {
        (Ty::AnyArray, Some(e)) => Ty::Array(e),
        (Ty::AnyElem, Some(e)) => e.ty(),
        (t, _) => t,
    })
}
