//! Static type inference/checking for expressions — shared by the validator
//! (user-facing diagnostics) and the backend (op/return-type selection), so the
//! two never disagree about an expression's type.

use std::collections::HashMap;

use crate::registry::PropertyDesc;
use crate::{intern, Elem, Expr, Registry, Signature, Ty};

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
    // A bare name that is a constant (and is not shadowed by a local, a
    // parameter or a module variable in `vars`) folds to its literal BEFORE any
    // shape-dependent rule runs — so a constant behaves in every position
    // exactly as the number it stands for, including the literal-to-`int64`
    // widening below that a raw `0` gets but an `Expr::Var` would miss.
    if let Expr::Var(name) = expr {
        if !vars.contains_key(name) {
            if let Some(c) = reg.const_(name) {
                let value = c.value.clone();
                return type_of_expr_hinted(&value, hint, vars, reg, components);
            }
        }
    }
    match expr {
        Expr::ArrayLit(items) if items.is_empty() => match hint {
            Some(Ty::Array(e)) => Ok(Ty::Array(e)),
            _ => err(
                "`[]` on its own does not say what it holds — declare the type, \
                 as in `var xs: text[] = []`",
            ),
        },
        Expr::DictLit(pairs) if pairs.is_empty() => match hint {
            Some(Ty::Dict(e)) => Ok(Ty::Dict(e)),
            _ => err(
                "`{}` on its own does not say what it holds — declare the type, \
                 as in `var ages: int{} = {}`",
            ),
        },
        // An integer literal takes `int64` when that is what its destination
        // declares, exactly as `[]` takes the list type of its destination.
        // Every `int` literal already fits an `int64` (it is stored as one),
        // and this is the ONLY implicit widening: it fires only for a literal,
        // never a variable or a sub-expression, so `let n: int64 = m` still
        // needs `int_to_int64(m)` and nothing about `int`'s strictness moves.
        // Without it a `ptr` offset or `mem_alloc(64)` could not be written
        // with the number in the source, since those parameters are `int64`.
        Expr::IntLit(_) if hint == Some(Ty::Int64) => Ok(Ty::Int64),
        // A `var r: RECT` with no initializer: the declared type is the only
        // thing that says which record, and it must be a c-record — a heap
        // record is a reference with nothing to point at until it is built, so
        // there is no honest zero value for one.
        Expr::ZeroInit => match hint {
            Some(Ty::Record(rec)) if reg.record(rec).map(|d| d.is_c).unwrap_or(false) => {
                Ok(Ty::Record(rec))
            }
            Some(Ty::Record(rec)) if reg.record(rec).is_some() => err(format!(
                "`{rec}` is a heap record and cannot be left uninitialised — build it with \
                 `{rec}(...)`, or declare it `is c` for a zeroed c-record"
            )),
            Some(t) => err(format!(
                "only a c-record `var` may be written without `= value`; `{}` needs one",
                t.as_str()
            )),
            None => type_of_expr_bare(expr, vars, reg, components),
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
            // The index goes through the hinted path, not the bare one, so a
            // `const` in index position folds to its literal the way it does in
            // every other expression position — `r.rgb[SLOT]` is how a header's
            // own name for a position gets used.
            let it = type_of_expr_in(index, vars, reg, components)?;
            // A dictionary is subscripted by its key, and a key is text. This
            // is `dict_get` spelled the way a subscript reads; a key that is
            // not there answers the sentinel and sets the error slot, which
            // `dict_has` is there to tell apart from a stored zero.
            if let Ty::Dict(v) = bt {
                if it != Ty::Text {
                    return err(format!(
                        "a dictionary is keyed by text, got {}",
                        it.as_str()
                    ));
                }
                return Ok(v.ty());
            }
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
                if v < 1 {
                    return err(format!(
                        "index {v} is before the start of the list — positions count from 1"
                    ));
                }
                if let Expr::ArrayLit(items) = &**base {
                    if v as usize > items.len() {
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
                // One element of a c-record's inline array. The count is part
                // of the type, so a literal index out of range is caught here
                // and there is no run-time check to pay for; a computed index
                // is a plain GEP, like every other `ptr` operation.
                Ty::CArray(a) => {
                    if let Some(v) = literal_index(index, vars, reg) {
                        if v > a.count as i64 {
                            return err(format!(
                                "index {v} is past the end of `{}` — it holds {} element(s)",
                                bt.as_str(),
                                a.count
                            ));
                        }
                    }
                    Ok(a.elem.surface())
                }
                other => err(format!("{} is not something you can index", other.as_str())),
            }
        }
        // `p.x` is a record field when `p` is a record and a component
        // property otherwise. Variables win, because a local is the nearer
        // name: a component id that a local shadows is unreachable either way,
        // and the reverse rule would let adding a component to a form change
        // what an existing subroutine's `p.x` means.
        Expr::GetProperty {
            component,
            property,
        } => match vars.get(component).copied() {
            Some(Ty::Record(rec)) => field_type(rec, property, reg),
            Some(other) => err(format!(
                "`{component}` is {} — only a record has fields",
                other.as_str()
            )),
            None => property_type(component, property, reg, components),
        },
        Expr::Field { base, name } => {
            let bt = type_of_expr_bare(base, vars, reg, components)?;
            match bt {
                Ty::Record(rec) => field_type(rec, name, reg),
                other => err(format!(
                    "`.{name}` reads a field, and {} has none",
                    other.as_str()
                )),
            }
        }
        Expr::RecordLit { name, fields } => {
            // A c-record has no `NAME(field: ...)` constructor: it is a flat
            // value, declared `var r: NAME` and filled field by field, so a
            // constructor call would have to allocate a heap object of the
            // wrong shape. Point the author at the form that works.
            if reg.record(name).map(|d| d.is_c).unwrap_or(false) {
                return err(format!(
                    "`{name}` is a c-record — it has no `{name}(...)` constructor; declare \
                     `var r: {name}` and assign its fields"
                ));
            }
            let Some(def) = reg.record(name).cloned() else {
                let mut known: Vec<&str> = reg.record_names().collect();
                known.sort_unstable();
                return err(format!(
                    "unknown record `{name}`{}",
                    if known.is_empty() {
                        String::new()
                    } else {
                        format!(" (known: {})", known.join(", "))
                    }
                ));
            };
            let mut given: Vec<&str> = Vec::new();
            for (fname, value) in fields {
                let Some((_, want)) = def.field(fname) else {
                    let known: Vec<&str> =
                        def.fields.iter().map(|(n, _)| n.as_str()).collect();
                    return err(format!(
                        "record `{name}` has no field `{fname}` (has: {})",
                        known.join(", ")
                    ));
                };
                if given.contains(&fname.as_str()) {
                    return err(format!("`{name}` gives field `{fname}` twice"));
                }
                given.push(fname);
                let got = type_of_expr_hinted(value, Some(want), vars, reg, components)?;
                if got != want {
                    return err(format!(
                        "record `{name}` field `{fname}` is {}, got {}",
                        want.as_str(),
                        got.as_str()
                    ));
                }
            }
            // Every field, every time. A record with a field left out would
            // have to be readable before it was written, and there is no value
            // a field could hold in the meantime that is not a lie.
            let missing: Vec<&str> = def
                .fields
                .iter()
                .map(|(n, _)| n.as_str())
                .filter(|n| !given.contains(n))
                .collect();
            if !missing.is_empty() {
                return err(format!(
                    "`{name}` is missing field{} {}",
                    if missing.len() == 1 { "" } else { "s" },
                    missing
                        .iter()
                        .map(|n| format!("`{n}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            Ok(Ty::Record(intern(name)))
        }
        Expr::DictLit(pairs) => {
            let mut value_ty: Option<Ty> = None;
            for (i, (key, value)) in pairs.iter().enumerate() {
                let kt = type_of_expr_bare(key, vars, reg, components)?;
                if kt != Ty::Text {
                    return err(format!(
                        "a dictionary is keyed by text; key {} is {}",
                        i + 1,
                        kt.as_str()
                    ));
                }
                let got = type_of_expr_bare(value, vars, reg, components)?;
                match value_ty {
                    None => {
                        if Elem::from_ty(got).is_none() {
                            return err(format!(
                                "a dictionary cannot hold {} values",
                                got.as_str()
                            ));
                        }
                        value_ty = Some(got);
                    }
                    Some(first) if first != got => {
                        return err(format!(
                            "every value in a dictionary has one type: the first is {}, \
                             value {} is {}",
                            first.as_str(),
                            i + 1,
                            got.as_str()
                        ))
                    }
                    Some(_) => {}
                }
            }
            match value_ty.and_then(Elem::from_ty) {
                Some(e) => Ok(Ty::Dict(e)),
                None => err(
                    "`{}` on its own does not say what it holds — declare the type, \
                     as in `var ages: int{} = {}`",
                ),
            }
        }
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
        // `address of NAME` is the address of a subroutine, as a `ptr` C can
        // call. The name must be a subroutine — nothing else has a stable,
        // link-time address to take — and its signature must be C-representable,
        // because the point is a pointer a C caller invokes with the C ABI.
        Expr::AddressOf(name) => {
            // `address of r.pt`, `address of r.rgb` — a path into a c-record's
            // flat storage. Typed by walking the very same rules a
            // read of that place uses: build the reader's expression and ask
            // for its type, so a misspelt field is the one message it always
            // was. Anything the walk reaches inside a c-record has an address.
            if let Some((root, rest)) = name.split_once('.') {
                let Some(Ty::Record(rec)) = vars.get(root).copied() else {
                    return err(format!(
                        "`address of {name}`: `{root}` is not a c-record local — a path is \
                         only meaningful inside one"
                    ));
                };
                if !reg.record(rec).map(|d| d.is_c).unwrap_or(false) {
                    return err(format!(
                        "`address of {name}`: `{root}` is a heap record, whose fields have no \
                         fixed address — declare `{rec}` `is c`"
                    ));
                }
                let mut place = Expr::Var(root.to_string());
                for step in rest.split('.') {
                    place = Expr::Field {
                        base: Box::new(place),
                        name: step.to_string(),
                    };
                }
                type_of_expr_bare(&place, vars, reg, components)?;
                return Ok(Ty::Ptr);
            }
            if let Some(vt) = vars.get(name).copied() {
                // A c-record local has a real address — its flat storage — so
                // `address of r` is the pointer a C API is handed. That is the
                // one variable whose address means something here.
                if let Ty::Record(rec) = vt {
                    if reg.record(rec).map(|d| d.is_c).unwrap_or(false) {
                        return Ok(Ty::Ptr);
                    }
                    return err(format!(
                        "`address of {name}`: `{name}` is a heap record, which has no fixed \
                         address to take — declare its type `is c` for a c-record with a \
                         layout, or pass the record itself"
                    ));
                }
                return err(format!(
                    "`address of {name}` takes the address of a subroutine or a c-record, but \
                     `{name}` is a {} variable — an ordinary variable has no callable address",
                    vt.as_str()
                ));
            }
            if reg.get(name).is_some() {
                return err(format!(
                    "`address of {name}` takes the address of a subroutine, but `{name}` \
                     is a built-in command"
                ));
            }
            if reg.dll(name).is_some() {
                return err(format!(
                    "`address of {name}` takes the address of a subroutine, but `{name}` \
                     is a foreign function — it is already a C function, so pass its name where \
                     a C library gives you a pointer, or wrap it in a sub"
                ));
            }
            let Some(sig) = reg.sub(name) else {
                return err(format!(
                    "`address of {name}`: there is no subroutine `{name}` to take the address of"
                ));
            };
            // Every parameter and the return must be a C-representable scalar,
            // in the same set a `dll` signature allows — a C caller passes and
            // receives these by value, and there is no honest machine layout for
            // an OpenEPL array, record or dictionary crossing that call.
            let bad = sig
                .params
                .iter()
                .enumerate()
                .find(|(_, t)| !is_c_representable(**t));
            if let Some((i, t)) = bad {
                return err(format!(
                    "`address of {name}`: parameter {} of `{name}` is {}, which cannot cross \
                     the C boundary — a callback sub takes int, int64, double, bool, text or ptr",
                    i + 1,
                    t.as_str()
                ));
            }
            if let Some(t) = sig.ret {
                if !is_c_representable(t) {
                    return err(format!(
                        "`address of {name}`: `{name}` returns {}, which cannot cross the C \
                         boundary — a callback sub returns int, int64, double, bool, text, ptr \
                         or nothing",
                        t.as_str()
                    ));
                }
            }
            Ok(Ty::Ptr)
        }
        // `size of TYPE` is an `int64` byte count. A c-record's is its flat
        // `sizeof`; a scalar's is its C width. A type with no by-value C layout
        // — a heap record, an array, a dictionary — has no such number, and its
        // "size" would be a pointer's, which is a trap worth refusing.
        Expr::SizeOf(t) => match t {
            Ty::Record(rec) => match reg.record(rec) {
                Some(def) if def.is_c => Ok(Ty::Int64),
                Some(_) => err(format!(
                    "`size of {rec}`: `{rec}` is a heap record — it has no fixed byte size \
                     (declare it `is c` for one)"
                )),
                None => err(format!("`size of {rec}`: there is no type `{rec}`")),
            },
            other if other.c_size_align().is_some() => Ok(Ty::Int64),
            other => err(format!(
                "`size of {}`: only a c-record or a scalar has a byte size",
                other.as_str()
            )),
        },
        // `ZeroInit` is the initializer of a `var r: RECT` with no `=`. It is
        // legal only against a c-record hint; every other use is the checker's
        // to reject, which it does through the hint being wrong or absent.
        Expr::ZeroInit => err(
            "an uninitialised `var` is only allowed for a c-record — this one has no type"
                .to_string(),
        ),
    }
}

/// Whether a type can be passed to or returned from C by value: the scalar set
/// a `dll` signature and a callback sub share. Kept here, beside the callback
/// checker, so the two lists cannot drift — the parser's `ffi_type` enforces the
/// same set on a `dll` declaration.
fn is_c_representable(t: Ty) -> bool {
    matches!(
        t,
        Ty::Int | Ty::Int64 | Ty::Double | Ty::Bool | Ty::Text | Ty::Ptr
    )
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
    if let Some(s) = reg.sub(name) {
        return Some(("subroutine", s.clone()));
    }
    reg.dll(name).map(|d| ("foreign function", d.sig.clone()))
}

/// The value of an index the checker can see: a literal, or a `const` that
/// stands for one. A constant is a literal with a name, so `r.rgb[LIMIT]` past
/// the end of an inline array is as visible a mistake as `r.rgb[40]` is — and
/// `FOO[SOME_CONST]` is how a C header gets transcribed.
fn literal_index(index: &Expr, vars: &HashMap<String, Ty>, reg: &Registry) -> Option<i64> {
    match index {
        Expr::IntLit(v) => Some(*v),
        // A local of the same name shadows the constant, exactly as it does
        // everywhere else a bare name is read.
        Expr::Var(n) if !vars.contains_key(n) => match reg.const_(n).map(|c| &c.value) {
            Some(Expr::IntLit(v)) => Some(*v),
            _ => None,
        },
        _ => None,
    }
}

/// The declared type of one field of record type `rec`, or a diagnostic.
pub fn field_type(rec: &str, field: &str, reg: &Registry) -> Result<Ty, SemaError> {
    let Some(def) = reg.record(rec) else {
        return err(format!("unknown record `{rec}`"));
    };
    match def.field(field) {
        // `.surface()` maps a c-record `byte` field to `int`: the type the
        // field reads and writes as. It is identity for every other type, so
        // heap-record fields are unaffected.
        Some((_, t)) => Ok(t.surface()),
        None => {
            let known: Vec<&str> = def.fields.iter().map(|(n, _)| n.as_str()).collect();
            err(format!(
                "record `{rec}` has no field `{field}` (has: {})",
                known.join(", ")
            ))
        }
    }
}

/// The declared type of `component.property`, or a diagnostic.
pub fn property_type(
    component: &str,
    property: &str,
    reg: &Registry,
    components: &Components,
) -> Result<Ty, SemaError> {
    property_desc(component, property, reg, components).map(|p| p.ty)
}

/// The whole descriptor of `component.property`, or a diagnostic.
///
/// The type is what most callers want; the validator also needs the editor
/// hint, because a property the inspector edits as a colour is one whose
/// literal can be checked for being a colour.
pub fn property_desc<'r>(
    component: &str,
    property: &str,
    reg: &'r Registry,
    components: &Components,
) -> Result<&'r PropertyDesc, SemaError> {
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
        Some(p) => Ok(p),
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
    // What the array or dictionary argument turned out to hold; every later
    // `AnyElem` is measured against it, which is how mixing element types is
    // caught.
    let mut elem: Option<Elem> = None;
    // Which collection the element type came from, so the diagnostic names the
    // thing the author wrote rather than a category.
    let mut holder = "array";
    for (i, (a, expected)) in args.iter().zip(&sig.params).enumerate() {
        let hint = match expected {
            Ty::AnyArray | Ty::AnyDict => None,
            Ty::AnyElem => elem.map(Elem::ty),
            t => Some(*t),
        };
        let got = type_of_expr_hinted(a, hint, vars, reg, components)?;
        match expected {
            Ty::AnyArray => match got.elem() {
                // A command declared over `AnyArray` sees its elements as 64
                // raw bits and asks the array's tag what they mean. For a
                // record that answer is "a pointer", which orders by address
                // and prints as nonsense — so the four commands that read
                // element VALUES are refused here rather than allowed to
                // produce a confident wrong answer.
                //
                // Named, because the ABI has no "these elements can be
                // compared" marker to test instead. The namespace is flat and a
                // library may not redefine a core command, so these four names
                // are these four commands.
                Some(Elem::Record(rec))
                    if matches!(cmd, "sort" | "join" | "contains" | "index_of") =>
                {
                    return err(format!(
                        "`{cmd}` compares or prints what a list holds, and a `{rec}` \
                         record has no order and no spelling — read the field you mean \
                         into a list of its own"
                    ))
                }
                Some(e) => elem = Some(e),
                None => {
                    return err(format!(
                        "{what} `{cmd}` argument {} expects an array, got {}",
                        i + 1,
                        got.as_str()
                    ))
                }
            },
            Ty::AnyDict => match got.value() {
                Some(e) => {
                    elem = Some(e);
                    holder = "dictionary";
                }
                None => {
                    return err(format!(
                        "{what} `{cmd}` argument {} expects a dictionary, got {}",
                        i + 1,
                        got.as_str()
                    ))
                }
            },
            Ty::AnyElem => {
                let Some(e) = elem else {
                    return err(format!(
                        "{what} `{cmd}` argument {} has no {holder} to take its type from",
                        i + 1
                    ));
                };
                if got != e.ty() {
                    return err(format!(
                        "{what} `{cmd}` argument {} must match what the {holder} holds \
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
            Ty::AnyDict => a.value(),
            _ => None,
        });
    Some(match (ret, elem) {
        (Ty::AnyArray, Some(e)) => Ty::Array(e),
        (Ty::AnyDict, Some(e)) => Ty::Dict(e),
        (Ty::AnyElem, Some(e)) => e.ty(),
        (t, _) => t,
    })
}
