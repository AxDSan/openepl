//! The one pass that needs both the program and the registry.
//!
//! Three pieces of 0.9.0 sugar cannot be settled by the parser, because each
//! asks a question only the registry can answer:
//!
//!  * `let n = length(xs)` — what type is that? (a command's return type)
//!  * `connect(host: "h")` — which slot is `host`? (a subroutine's parameters)
//!  * `point{..p, y: 9}` — which fields did the author leave out? (a record)
//!
//! So the parser records the question — [`StmtKind::LetInfer`],
//! [`Expr::Labeled`], [`Expr::RecordUpdate`] — and this pass answers it,
//! rewriting each into something the language already had. Everything after it
//! (the checker, the backend) sees only the older, plainer forms, which is why
//! none of this sugar can introduce a meaning the language did not have: a
//! named argument becomes a positional one, a default becomes the expression
//! written at the call, a record update becomes the field-by-field literal.
//!
//! It runs inside `validate` and inside `lower_module`, so no caller has to
//! remember it.

use std::collections::HashMap;

use crate::sema::{callee, type_of_expr_in, Components};
use crate::{
    foreach_elem_types, intern, Expr, Item, Module, Registry, Signature, Span, Stmt, StmtKind, Ty,
};

/// Something the sugar asked for that the program cannot mean.
#[derive(Debug, Clone, PartialEq)]
pub struct DesugarError {
    pub msg: String,
    pub span: Span,
}

/// Rewrite every registry-dependent piece of sugar in `m`.
///
/// The module comes back whole either way: on an error the offending node is
/// left as close to what was written as it can be, so the checker that runs
/// next still reports everything else it would have.
pub fn desugar(m: &Module, reg: &Registry) -> (Module, Vec<DesugarError>) {
    let mut out = m.clone();
    let mut d = Desugar {
        reg,
        components: component_types(m),
        globals: m.globals().map(|g| (g.name.clone(), g.ty)).collect(),
        default_depth: 0,
        hidden: 0,
        errs: Vec::new(),
    };
    for item in &mut out.items {
        match item {
            Item::Sub(sub) => {
                let mut vars = d.globals.clone();
                for (name, ty) in &sub.params {
                    vars.insert(name.clone(), *ty);
                }
                // A default is evaluated at the call, so it is checked here as
                // an expression of its own — see `validate`, which refuses one
                // that reads a name.
                for def in sub.defaults.iter_mut().flatten() {
                    let mut empty = HashMap::new();
                    d.expr(def, &mut empty, Span::line(sub.line));
                }
                sub.body = d.block(&sub.body, &mut vars);
            }
            // A module variable's initializer is an expression like any other:
            // `var port: int = default_port(kind: 1)` has to resolve too.
            Item::Var(g) => {
                let mut empty = HashMap::new();
                d.expr(&mut g.value, &mut empty, Span::default());
            }
            _ => {}
        }
    }
    let errs = d.errs;
    (out, errs)
}

/// Component ids and their types, exactly as the checker collects them: a
/// property read (`ok_button.text`) has to type for `let t = ok_button.text` to
/// work at all.
fn component_types(m: &Module) -> Components {
    let mut components = Components::new();
    for form in m.forms() {
        for child in &form.children {
            components.insert(child.id.clone(), child.type_name.clone());
        }
    }
    for c in m.items.iter().filter_map(|i| match i {
        Item::Component(c) => Some(c),
        _ => None,
    }) {
        components.insert(c.id.clone(), c.type_name.clone());
    }
    components
}

struct Desugar<'r> {
    reg: &'r Registry,
    components: Components,
    globals: HashMap<String, Ty>,
    /// How deep the pass is inside a default it is expanding. A default is
    /// itself an expression with sugar in it, so it is rewritten too — and
    /// `sub f(a: int = f())` would otherwise expand for ever.
    default_depth: usize,
    /// Counts the hidden bindings this pass makes, so two `if some` in one
    /// subroutine cannot collide — locals are function-scoped.
    hidden: usize,
    errs: Vec<DesugarError>,
}

impl Desugar<'_> {
    fn err(&mut self, msg: String, span: Span) {
        self.errs.push(DesugarError { msg, span });
    }

    /// Rewrite a block, tracking what each name means as it goes.
    ///
    /// Locals are function-scoped, so one map serves the whole body; the walk
    /// is depth-first in source order, which is the order a reader gives the
    /// names their meaning in.
    fn block(&mut self, stmts: &[Stmt], vars: &mut HashMap<String, Ty>) -> Vec<Stmt> {
        let mut out = Vec::with_capacity(stmts.len());
        for stmt in stmts {
            self.stmt(stmt, vars, &mut out);
        }
        out
    }

    fn stmt(&mut self, stmt: &Stmt, vars: &mut HashMap<String, Ty>, out: &mut Vec<Stmt>) {
        let span = stmt.span;
        // `if some` is the one statement that becomes *more than one*, so it is
        // expanded here, ahead of the in-place rewrites below.
        if let StmtKind::IfSome { .. } = &stmt.kind {
            self.if_some(stmt, vars, out);
            return;
        }
        let mut s = stmt.clone();
        match &mut s.kind {
            StmtKind::LetInfer {
                name,
                value,
                mutable,
            } => {
                self.expr(value, vars, span);
                // A c-record has no constructor — it is flat storage, not a
                // heap object — so `var r = RECT{left: 1}` is not a value being
                // bound at all; it is a zeroed local and a run of field writes.
                // Naming the type is the whole of what the literal adds here.
                if let Expr::RecordLit { name: rec, .. } = value {
                    if self.reg.record(rec).map(|d| d.is_c).unwrap_or(false) {
                        let ty = Ty::Record(intern(rec));
                        let (name, value, mutable) = (name.clone(), value.clone(), *mutable);
                        vars.insert(name.clone(), ty);
                        self.emit_c_record(&name, ty, &value, mutable, span, out);
                        return;
                    }
                }
                let ty = match type_of_expr_in(value, vars, self.reg, &self.components) {
                    Ok(ty) => ty,
                    Err(e) => {
                        self.err(
                            format!(
                                "the type of `{name}` cannot be read off its value ({e}) — \
                                 write it out: `{}{name}: TYPE = ...`",
                                if *mutable { "var " } else { "let " }
                            ),
                            span,
                        );
                        Ty::Int
                    }
                };
                vars.insert(name.clone(), ty);
                s.kind = StmtKind::Let {
                    name: name.clone(),
                    ty,
                    value: value.clone(),
                    mutable: *mutable,
                };
                out.push(s);
                return;
            }
            StmtKind::Let {
                name, ty, value, ..
            } => {
                self.expr(value, vars, span);
                let (name, ty) = (name.clone(), *ty);
                vars.insert(name.clone(), ty);
                if matches!(value, Expr::RecordLit { .. })
                    && self.c_record_of(ty).is_some()
                {
                    let value = value.clone();
                    let mutable = matches!(&s.kind, StmtKind::Let { mutable: true, .. });
                    self.emit_c_record(&name, ty, &value, mutable, span, out);
                    return;
                }
            }
            StmtKind::Assign { value, .. } => self.expr(value, vars, span),
            StmtKind::Call { cmd, args } => {
                let cmd = cmd.clone();
                self.args(args, vars, span);
                *args = self.resolve(&cmd, std::mem::take(args), span);
            }
            StmtKind::CallThrough { callee, args, .. } => {
                self.expr(callee, vars, span);
                self.args(args, vars, span);
                self.no_labels(args, span);
            }
            StmtKind::SetIndex { index, value, .. } => {
                self.expr(index, vars, span);
                self.expr(value, vars, span);
            }
            StmtKind::SetProperty { value, .. } => self.expr(value, vars, span),
            StmtKind::SetPlace { place, value } => {
                self.expr(place, vars, span);
                self.expr(value, vars, span);
            }
            StmtKind::If { arms, otherwise } => {
                for (cond, body) in arms.iter_mut() {
                    self.expr(cond, vars, span);
                    *body = self.block(body, vars);
                }
                if let Some(body) = otherwise {
                    *body = self.block(body, vars);
                }
            }
            StmtKind::While { cond, body } => {
                self.expr(cond, vars, span);
                *body = self.block(body, vars);
            }
            StmtKind::Match {
                scrutinee,
                arms,
                otherwise,
            } => {
                self.expr(scrutinee, vars, span);
                for (values, body) in arms.iter_mut() {
                    for v in values.iter_mut() {
                        self.expr(v, vars, span);
                    }
                    *body = self.block(body, vars);
                }
                if let Some(body) = otherwise {
                    *body = self.block(body, vars);
                }
            }
            StmtKind::For {
                var,
                start,
                limit,
                body,
                ..
            } => {
                self.expr(start, vars, span);
                self.expr(limit, vars, span);
                vars.insert(var.clone(), Ty::Int);
                *body = self.block(body, vars);
            }
            StmtKind::ForEach {
                elem,
                value,
                index,
                coll,
                body,
            } => {
                self.expr(coll, vars, span);
                // The collection's type decides the bindings' types; when it
                // does not type at all the checker will say so, and the body is
                // still walked so its own sugar is resolved.
                if let Ok(cty) = type_of_expr_in(coll, vars, self.reg, &self.components) {
                    if let Some((elem_ty, value_ty)) = foreach_elem_types(cty) {
                        vars.insert(elem.clone(), elem_ty);
                        if let Some(v) = value {
                            vars.insert(v.clone(), value_ty.unwrap_or(elem_ty));
                        }
                        if let Some(i) = index {
                            vars.insert(i.clone(), Ty::Int);
                        }
                    }
                }
                *body = self.block(body, vars);
            }
            StmtKind::Return { value } => {
                if let Some(v) = value {
                    self.expr(v, vars, span);
                }
            }
            // The statement a `defer` holds is walked in place: it is one
            // statement, so the pass that resolves its sugar returns one, and
            // the `defer` still stands where it was written for the checker.
            StmtKind::Defer(inner) => {
                let held = (**inner).clone();
                let mut one = Vec::new();
                self.stmt(&held, vars, &mut one);
                if let Some(first) = one.into_iter().next() {
                    *inner = Box::new(first);
                }
            }
            // Expanded above, before this match runs.
            StmtKind::IfSome { .. } => {}
            StmtKind::Break | StmtKind::Continue => {}
        }
        out.push(s);
    }

    /// The c-record `ty` names, if it names one.
    fn c_record_of(&self, ty: Ty) -> Option<&'static str> {
        match ty {
            Ty::Record(n) => self.reg.record(n).filter(|d| d.is_c).map(|_| n),
            _ => None,
        }
    }

    /// `var r: RECT = RECT{left: 1}` -> `var r: RECT` (zeroed) plus one field
    /// write per field the literal gave.
    ///
    /// This is the only statement position a c-record literal is allowed in,
    /// and the rewrite is why: a c-record is stack storage with no value form,
    /// so the literal has to become the writes the author would have typed. The
    /// fields left out keep the zero the declaration already gave them.
    fn emit_c_record(
        &mut self,
        name: &str,
        ty: Ty,
        value: &Expr,
        mutable: bool,
        span: Span,
        out: &mut Vec<Stmt>,
    ) {
        let Expr::RecordLit { name: rec, fields } = value else {
            return;
        };
        if Ty::Record(intern(rec)) != ty {
            self.err(
                format!("`{name}` is {} and the value is a `{rec}`", ty.as_str()),
                span,
            );
            return;
        }
        let mut decl = Stmt::new(
            StmtKind::Let {
                name: name.to_string(),
                ty,
                value: Expr::ZeroInit,
                // A c-record's fields are written after the declaration, so the
                // binding has to be one that may be written through.
                mutable,
            },
            span.line,
        );
        decl.span = span;
        out.push(decl);
        for (field, v) in fields {
            let mut st = Stmt::new(
                StmtKind::SetProperty {
                    component: name.to_string(),
                    property: field.clone(),
                    value: v.clone(),
                },
                span.line,
            );
            st.span = span;
            out.push(st);
        }
    }

    /// Rewrite each argument in place, leaving a `Labeled` wrapper alone — the
    /// call itself is what knows whether a label means anything.
    fn args(&mut self, args: &mut [Expr], vars: &mut HashMap<String, Ty>, span: Span) {
        for a in args.iter_mut() {
            match a {
                Expr::Labeled { value, .. } => self.expr(value, vars, span),
                other => self.expr(other, vars, span),
            }
        }
    }

    fn no_labels(&mut self, args: &[Expr], span: Span) {
        for a in args {
            if let Expr::Labeled { name, .. } = a {
                self.err(
                    format!(
                        "`{name}:` names a parameter, and an indirect call has none — \
                         pass the arguments in order"
                    ),
                    span,
                );
            }
        }
    }

    /// `if some EXPR as NAME` → the `if` it stands for.
    ///
    /// Two statements come out of it, or one: the optional is bound to a hidden
    /// local first, unless `EXPR` already *is* an optional local and there is
    /// nothing to copy. Then an ordinary `if` whose condition is the hidden
    /// truth value and whose arm opens by binding `NAME` to the value — a plain
    /// `T`, which is what makes the body's uses of it legal.
    fn if_some(&mut self, stmt: &Stmt, vars: &mut HashMap<String, Ty>, out: &mut Vec<Stmt>) {
        let span = stmt.span;
        let StmtKind::IfSome {
            value,
            bind,
            body,
            otherwise,
        } = &stmt.kind
        else {
            return;
        };
        let mut value = value.clone();
        self.expr(&mut value, vars, span);

        // What the value is decides how it is read: an optional already in a
        // local can be tested where it stands; anything else is a value that
        // may have failed to arrive, so it is bound to an optional first.
        let known = type_of_expr_in(&value, vars, self.reg, &self.components);
        let elem = match known {
            Ok(Ty::Optional(e)) => Some(e),
            Ok(other) => match crate::Elem::from_ty(other) {
                Some(e) => Some(e),
                None => {
                    self.err(
                        format!(
                            "`if some` unwraps a value that may be absent, and {} is always                              there",
                            other.as_str()
                        ),
                        span,
                    );
                    None
                }
            },
            // The expression does not type; the checker says why. Expand into
            // an `if` over it anyway so the body is still walked and checked.
            Err(_) => None,
        };
        let Some(elem) = elem else {
            let mut body_vars = vars.clone();
            let body = self.block(body, &mut body_vars);
            let otherwise = otherwise.as_ref().map(|b| self.block(b, vars));
            let mut s = stmt.clone();
            s.kind = StmtKind::If {
                arms: vec![(value, body)],
                otherwise,
            };
            out.push(s);
            return;
        };

        let src = if matches!(known, Ok(Ty::Optional(_))) && matches!(value, Expr::Var(_)) {
            let Expr::Var(n) = &value else { unreachable!() };
            n.clone()
        } else {
            self.hidden += 1;
            let name = format!("$some${}", self.hidden);
            let mut held = stmt.clone();
            held.kind = StmtKind::Let {
                name: name.clone(),
                ty: Ty::Optional(elem),
                value,
                mutable: false,
            };
            vars.insert(name.clone(), Ty::Optional(elem));
            out.push(held);
            name
        };

        // The binding is a local of the enclosing subroutine like any other —
        // locals are function-scoped here, so it goes into the same map the
        // checker will build, and a second `if some ... as v` in one
        // subroutine collides exactly as a second `let v` would.
        vars.insert(bind.clone(), elem.ty());
        let mut arm = vec![Stmt::new(
            StmtKind::Let {
                name: bind.clone(),
                ty: elem.ty(),
                value: Expr::Unwrap(Box::new(Expr::Var(src.clone()))),
                mutable: false,
            },
            stmt.line,
        )];
        arm.extend(self.block(body, vars));
        let otherwise = otherwise.as_ref().map(|b| self.block(b, vars));
        let mut s = stmt.clone();
        s.kind = StmtKind::If {
            arms: vec![(Expr::HasValue(Box::new(Expr::Var(src))), arm)],
            otherwise,
        };
        out.push(s);
    }

    /// Rewrite one expression in place.
    fn expr(&mut self, e: &mut Expr, vars: &mut HashMap<String, Ty>, span: Span) {
        match e {
            Expr::Call { cmd, args } => {
                let cmd = cmd.clone();
                self.args(args, vars, span);
                *args = self.resolve(&cmd, std::mem::take(args), span);
            }
            Expr::CallThrough { callee, args, .. } => {
                self.expr(callee, vars, span);
                self.args(args, vars, span);
                self.no_labels(args, span);
            }
            // A comprehension is the one expression that binds names, so its
            // body and its `where` are walked with those names in scope — and
            // the element type only the registry can work out is written into
            // the node here, which is what lets the backend build the array
            // before it has run the loop that fills it.
            Expr::Comprehension {
                body,
                elem,
                value,
                index,
                coll,
                cond,
                holds,
            } => {
                self.expr(coll, vars, span);
                match crate::sema::comprehension_scope(
                    coll,
                    elem,
                    value.as_deref(),
                    index.as_deref(),
                    vars,
                    self.reg,
                    &self.components,
                ) {
                    Ok(mut inner) => {
                        if let Some(c) = cond {
                            self.expr(c, &mut inner, span);
                        }
                        self.expr(body, &mut inner, span);
                        match crate::sema::type_of_expr_in(
                            body,
                            &inner,
                            self.reg,
                            &self.components,
                        )
                        .ok()
                        .and_then(crate::Elem::from_ty)
                        {
                            Some(el) => *holds = Some(el),
                            // The checker runs next and says why; leaving
                            // `holds` empty keeps this from being reported here
                            // as well.
                            None => {}
                        }
                    }
                    // The collection does not type, or cannot be walked. The
                    // checker reports it; the body is still walked so its own
                    // sugar resolves and does not produce a second complaint.
                    Err(_) => {
                        if let Some(c) = cond {
                            self.expr(c, vars, span);
                        }
                        self.expr(body, vars, span);
                    }
                }
            }
            // `none` carries no sub-expression, and the two halves of an
            // optional are made here — nothing inside them is written by an
            // author, so there is no sugar in them to resolve.
            Expr::NoneLit | Expr::HasValue(_) | Expr::Unwrap(_) => {}
            Expr::RecordLit { name, fields } => {
                for (_, v) in fields.iter_mut() {
                    self.expr(v, vars, span);
                }
                // `f(a: 1, b: 2)` and `point(x: 1, y: 2)` are the same three
                // tokens; which one it is depends on whether the name is a
                // record. A record wins, because the spelling was a record's
                // before it was a call's.
                if self.reg.record(name).is_some() {
                    return;
                }
                let Some(_) = callee(name, self.reg) else {
                    let mut known: Vec<&str> = self.reg.record_names().collect();
                    known.sort_unstable();
                    self.err(
                        format!(
                            "`{name}` is neither a record nor something that can be called\
                             {}",
                            if known.is_empty() {
                                String::new()
                            } else {
                                format!(" (records here: {})", known.join(", "))
                            }
                        ),
                        span,
                    );
                    return;
                };
                let name = name.clone();
                let labelled: Vec<Expr> = std::mem::take(fields)
                    .into_iter()
                    .map(|(n, v)| Expr::Labeled {
                        name: n,
                        value: Box::new(v),
                    })
                    .collect();
                // Through `resolve`, not straight to `fill`: a *command* has no
                // parameter names, and it is `resolve` that says so instead of
                // offering an empty list of them.
                let args = self.resolve(&name, labelled, span);
                *e = Expr::Call { cmd: name, args };
            }
            Expr::RecordUpdate { name, base, fields } => {
                self.expr(base, vars, span);
                for (_, v) in fields.iter_mut() {
                    self.expr(v, vars, span);
                }
                *e = self.expand_update(name, base, fields, vars, span);
            }
            Expr::Labeled { name, value } => {
                self.expr(value, vars, span);
                self.err(
                    format!("`{name}:` names an argument, and this is not an argument list"),
                    span,
                );
            }
            Expr::Bin(_, l, r)
            | Expr::Cmp(_, l, r)
            | Expr::Logical(_, l, r)
            | Expr::Bit(_, l, r) => {
                self.expr(l, vars, span);
                self.expr(r, vars, span);
            }
            Expr::Chain { lo, mid, hi, .. } => {
                self.expr(lo, vars, span);
                self.expr(mid, vars, span);
                self.expr(hi, vars, span);
            }
            Expr::In {
                needle, haystack, ..
            } => {
                self.expr(needle, vars, span);
                self.expr(haystack, vars, span);
            }
            Expr::ToText { value, .. } => self.expr(value, vars, span),
            Expr::IfElse { cond, then, els } => {
                self.expr(cond, vars, span);
                self.expr(then, vars, span);
                self.expr(els, vars, span);
            }
            Expr::Otherwise { value, fallback } => {
                self.expr(value, vars, span);
                self.expr(fallback, vars, span);
            }
            Expr::Not(e) | Expr::BitNot(e) | Expr::Neg(e) => self.expr(e, vars, span),
            Expr::Index { base, index } => {
                self.expr(base, vars, span);
                self.expr(index, vars, span);
            }
            Expr::Slice { base, from, to } => {
                self.expr(base, vars, span);
                if let Some(f) = from {
                    self.expr(f, vars, span);
                }
                if let Some(t) = to {
                    self.expr(t, vars, span);
                }
            }
            Expr::ArrayLit(items) => {
                for i in items.iter_mut() {
                    self.expr(i, vars, span);
                }
            }
            Expr::DictLit(pairs) => {
                for (k, v) in pairs.iter_mut() {
                    self.expr(k, vars, span);
                    self.expr(v, vars, span);
                }
            }
            Expr::Field { base, .. } => self.expr(base, vars, span),
            // `none` is a soft word, like `each` and `then`: it is the empty
            // optional only where no variable of that name exists, so a program
            // that already had a `var none: int[] = []` keeps it. The name is
            // looked up exactly as reading it would look it up — locals and
            // module variables in `vars`, then the constants — so the word
            // means the literal in precisely the places reading it would have
            // been an error.
            Expr::Var(n)
                if n == "none"
                    && !vars.contains_key(n)
                    && !self.reg.is_const(n)
                    && !self.components.contains_key(n) =>
            {
                *e = Expr::NoneLit;
            }
            Expr::IntLit(_)
            | Expr::BitsLit(_)
            | Expr::DoubleLit(_)
            | Expr::TextLit(_)
            | Expr::BoolLit(_)
            | Expr::Var(_)
            | Expr::GetProperty { .. }
            | Expr::AddressOf(_)
            | Expr::SizeOf(_)
            | Expr::ZeroInit => {}
        }
    }

    /// `point{..p, y: 9}` -> `point(x: p.x, y: 9)`.
    ///
    /// Every field the author did not name is read from the base, which is why
    /// the base has to be a *place*: it is spelled once per field, and a call
    /// spelled three times is three calls.
    fn expand_update(
        &mut self,
        name: &str,
        base: &Expr,
        fields: &[(String, Expr)],
        vars: &mut HashMap<String, Ty>,
        span: Span,
    ) -> Expr {
        let plain = Expr::RecordLit {
            name: name.to_string(),
            fields: fields.to_vec(),
        };
        let Some(def) = self.reg.record(name).cloned() else {
            self.err(format!("unknown record `{name}`"), span);
            return plain;
        };
        if def.is_c {
            self.err(
                format!(
                    "`{name}` is a c-record — it is flat storage with no value form, so \
                     there is nothing to copy from; write its fields"
                ),
                span,
            );
            return plain;
        }
        if !is_place(base) {
            self.err(
                format!(
                    "the `...` in `{name}{{...}}` copies one field at a time, so it reads \
                     what follows it once per field — name a variable there, not an \
                     expression that does work"
                ),
                span,
            );
            return plain;
        }
        match type_of_expr_in(base, vars, self.reg, &self.components) {
            Ok(got) if got == Ty::Record(intern(name)) => {}
            Ok(got) => {
                self.err(
                    format!(
                        "`{name}{{...}}` copies a `{name}`, and what follows `...` is {}",
                        got.as_str()
                    ),
                    span,
                );
                return plain;
            }
            Err(e) => {
                self.err(format!("in `{name}{{...}}`: {e}"), span);
                return plain;
            }
        }
        let mut out: Vec<(String, Expr)> = Vec::with_capacity(def.fields.len());
        for (fname, _) in &def.fields {
            match fields.iter().find(|(n, _)| n == fname) {
                Some((_, v)) => out.push((fname.clone(), v.clone())),
                None => out.push((
                    fname.clone(),
                    Expr::Field {
                        base: Box::new(base.clone()),
                        name: fname.clone(),
                    },
                )),
            }
        }
        for (given, _) in fields {
            if !def.fields.iter().any(|(n, _)| n == given) {
                let known: Vec<&str> = def.fields.iter().map(|(n, _)| n.as_str()).collect();
                self.err(
                    format!(
                        "record `{name}` has no field `{given}` (has: {})",
                        known.join(", ")
                    ),
                    span,
                );
            }
        }
        Expr::RecordLit {
            name: name.to_string(),
            fields: out,
        }
    }

    /// Match a call's arguments to `cmd`'s parameters, if `cmd` names something
    /// with parameters to match against.
    fn resolve(&mut self, cmd: &str, args: Vec<Expr>, span: Span) -> Vec<Expr> {
        let labelled = args.iter().any(|a| matches!(a, Expr::Labeled { .. }));
        let Some((what, sig)) = callee(cmd, self.reg) else {
            if labelled {
                // A record whose fields are not all named arrives here, because
                // a record literal is the one call-shaped thing that requires
                // every name. Say that, rather than "unknown subroutine".
                if self.reg.record(cmd).is_some() {
                    self.err(
                        format!(
                            "`{cmd}` is a record, and a record names every field it is \
                             given — the argument with no name has no field to go to"
                        ),
                        span,
                    );
                } else {
                    self.err(format!("unknown subroutine or command `{cmd}`"), span);
                }
            }
            return args;
        };
        // The overwhelmingly common call: as many arguments as parameters, none
        // named. Nothing to rewrite, and no message to invent.
        if !labelled && args.len() == sig.params.len() {
            return args;
        }
        if !labelled && sig.defaults.iter().all(|d| d.is_none()) {
            // No sugar is in play: the plain arity diagnostic the checker
            // already gives is the right one.
            return args;
        }
        if labelled && sig.names.is_empty() {
            self.err(
                format!(
                    "{what} `{cmd}` takes its arguments in order — it has no parameter names"
                ),
                span,
            );
            return args;
        }
        self.fill(cmd, &sig, args, span)
    }

    /// The slot-filling itself: positional arguments in order, named ones into
    /// the slot they name, then each empty slot from its default.
    fn fill(&mut self, cmd: &str, sig: &Signature, args: Vec<Expr>, span: Span) -> Vec<Expr> {
        let n = sig.params.len();
        let mut slots: Vec<Option<Expr>> = vec![None; n];
        // Arguments past the last parameter, kept so the checker still reports
        // the arity rather than seeing a call that silently lost one.
        let mut extra: Vec<Expr> = Vec::new();
        let mut next = 0usize;
        let mut seen_named = false;
        let mut out_of_order = false;
        for a in args {
            match a {
                Expr::Labeled { name, value } => {
                    seen_named = true;
                    let Some(i) = sig.param_index(&name) else {
                        self.err(
                            format!(
                                "`{cmd}` has no parameter `{name}` (it has: {})",
                                sig.names.join(", ")
                            ),
                            span,
                        );
                        continue;
                    };
                    if slots[i - 1].is_some() {
                        self.err(format!("`{cmd}` is given `{name}` twice"), span);
                        continue;
                    }
                    slots[i - 1] = Some(*value);
                }
                positional => {
                    // Reported once, and then placed as if it had come first:
                    // the order is the mistake, and re-reporting the arity on
                    // top of it would bury the sentence that says so.
                    if seen_named && !out_of_order {
                        out_of_order = true;
                        self.err(
                            format!(
                                "in `{cmd}`: an argument with no name follows one with a \
                                 name — the named arguments come last"
                            ),
                            span,
                        );
                    }
                    while next < n && slots[next].is_some() {
                        next += 1;
                    }
                    if next < n {
                        slots[next] = Some(positional);
                        next += 1;
                    } else {
                        extra.push(positional);
                    }
                }
            }
        }
        let mut out = Vec::with_capacity(n);
        for (i, slot) in slots.into_iter().enumerate() {
            let from_default = slot.is_none();
            match slot.or_else(|| sig.defaults.get(i).cloned().flatten()) {
                Some(mut v) => {
                    // The default came from the declaration as written, so it
                    // still holds whatever sugar the author put in it: rewrite
                    // the copy exactly as if it had been typed at this call,
                    // because that is what it now is.
                    if from_default {
                        if self.default_depth > 16 {
                            self.err(
                                format!(
                                    "`{cmd}` has a default that ends up calling `{cmd}` \
                                     again, so filling it in never finishes"
                                ),
                                span,
                            );
                            return out;
                        }
                        self.default_depth += 1;
                        let mut none = HashMap::new();
                        self.expr(&mut v, &mut none, span);
                        self.default_depth -= 1;
                    }
                    out.push(v)
                }
                None => self.err(
                    format!(
                        "`{cmd}` was not given `{}`, and it has no default",
                        sig.names.get(i).map(String::as_str).unwrap_or("an argument")
                    ),
                    span,
                ),
            }
        }
        out.extend(extra);
        out
    }
}

/// Whether `e` names storage rather than computing something: a variable, or a
/// field or element path from one. Reading one twice reads the same thing
/// twice, which is what lets a record update spell it once per field.
fn is_place(e: &Expr) -> bool {
    match e {
        Expr::Var(_) => true,
        Expr::Field { base, .. } => is_place(base),
        Expr::GetProperty { .. } => true,
        Expr::Index { base, index } => is_place(base) && matches!(**index, Expr::IntLit(_)),
        _ => false,
    }
}

// --- `defer` ------------------------------------------------------------

/// Copy every `defer`red statement to the exits of the block it was written in.
///
/// This is the whole of `defer`: there is no run-time stack of pending calls,
/// only the statement, written out again at each way the block can be left.
/// Within one block the copies unwind in reverse order of declaration, so a
/// second `defer` — set up while the first one's cleanup was already standing —
/// is undone first, which is the order the pairing was created in.
///
/// A block is left three ways, and each is covered:
///
///  * falling off the end — the copies land after the last statement;
///  * `return` anywhere below the `defer` — the copies land in front of it,
///    together with every enclosing block's, innermost first;
///  * `break` / `continue` that leaves the block — likewise, except that a
///    jump inside a *nested* loop belongs to that loop and leaves nothing.
///
/// `return EXPR` binds `EXPR` to a hidden local **before** the cleanup runs, so
/// the classic pairing — `defer call file_close(h)` above `return
/// file_read_text(h)` — reads the handle while it is still open. That is why
/// the sub's return type is a parameter: the value needs a type to be bound to,
/// and `return` in a sub that returns `T` returns a `T`.
pub fn expand_defer(body: &[Stmt], ret_ty: Option<Ty>) -> Vec<Stmt> {
    let mut x = DeferExp { ret_ty, hidden: 0 };
    x.block(body, &[], &[])
}

struct DeferExp {
    ret_ty: Option<Ty>,
    hidden: usize,
}

impl DeferExp {
    /// `on_return` / `on_jump` are the cleanups the *enclosing* blocks owe,
    /// already in run order; this block's own are pushed in front of them.
    fn block(&mut self, stmts: &[Stmt], on_return: &[Stmt], on_jump: &[Stmt]) -> Vec<Stmt> {
        // Declaration order; every exit emits the reverse.
        let mut mine: Vec<Stmt> = Vec::new();
        let mut out: Vec<Stmt> = Vec::new();
        for s in stmts {
            // The cleanup owed at this point, innermost first.
            let unwind = |mine: &Vec<Stmt>, outer: &[Stmt]| -> Vec<Stmt> {
                let mut v: Vec<Stmt> = mine.iter().rev().cloned().collect();
                v.extend_from_slice(outer);
                v
            };
            match &s.kind {
                StmtKind::Defer(inner) => mine.push((**inner).clone()),
                StmtKind::Return { value } => {
                    let cleanup = unwind(&mine, on_return);
                    if cleanup.is_empty() {
                        out.push(s.clone());
                        continue;
                    }
                    // The value is computed first: the cleanup may close the
                    // very thing the expression reads, or assign to a variable
                    // it names.
                    match (value, self.ret_ty) {
                        (Some(e), Some(ty)) => {
                            self.hidden += 1;
                            let tmp = format!("$defer${}", self.hidden);
                            out.push(Stmt::new(
                                StmtKind::Let {
                                    name: tmp.clone(),
                                    ty,
                                    value: e.clone(),
                                    mutable: false,
                                },
                                s.line,
                            ));
                            out.extend(cleanup);
                            let mut r = s.clone();
                            r.kind = StmtKind::Return {
                                value: Some(Expr::Var(tmp)),
                            };
                            out.push(r);
                        }
                        // No value, or a mismatch the checker has already
                        // reported: nothing to preserve, so run the cleanup.
                        _ => {
                            out.extend(cleanup);
                            out.push(s.clone());
                        }
                    }
                }
                StmtKind::Break | StmtKind::Continue => {
                    out.extend(unwind(&mine, on_jump));
                    out.push(s.clone());
                }
                _ => {
                    let ret = unwind(&mine, on_return);
                    let jump = unwind(&mine, on_jump);
                    out.push(self.nested(s, &ret, &jump));
                }
            }
        }
        out.extend(mine.into_iter().rev());
        out
    }

    /// Rewrite the blocks a compound statement holds. A loop's body swallows
    /// `break` and `continue`, so nothing it contains owes the enclosing block a
    /// jump cleanup — only a `return` still climbs out.
    fn nested(&mut self, s: &Stmt, on_return: &[Stmt], on_jump: &[Stmt]) -> Stmt {
        let mut out = s.clone();
        match &mut out.kind {
            StmtKind::If { arms, otherwise } => {
                for (_, body) in arms.iter_mut() {
                    *body = self.block(body, on_return, on_jump);
                }
                if let Some(b) = otherwise {
                    *b = self.block(b, on_return, on_jump);
                }
            }
            StmtKind::Match {
                arms, otherwise, ..
            } => {
                for (_, body) in arms.iter_mut() {
                    *body = self.block(body, on_return, on_jump);
                }
                if let Some(b) = otherwise {
                    *b = self.block(b, on_return, on_jump);
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::ForEach { body, .. } => {
                *body = self.block(body, on_return, &[]);
            }
            _ => {}
        }
        out
    }
}
