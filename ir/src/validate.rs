//! IR validator: rejects malformed / ill-typed IR before lowering.
//!
//! Runs the shared type checker (`sema`) over every subroutine and adds the
//! structural rules it doesn't cover — entry-point presence, redefinition, and
//! (new in Phase 2) the component model: component types, property names and
//! types, and event bindings are all checked against the descriptors
//! introspected from support libraries, so a typo in a form is a compile error
//! rather than a silently missing widget.
//!
//! Collects *all* errors rather than stopping at the first.
//!
//! Every diagnostic carries the source line of the statement it came from, so
//! an editor (and the language server) can put the squiggle in the right place.

use std::collections::{HashMap, HashSet};

use crate::sema::{
    callee, check_args_labeled, property_type, type_of_expr_hinted, type_of_expr_in, Components,
};
use crate::{Component, Expr, Item, Module, Registry, Stmt, StmtKind, Sub, Target, Ty};

#[derive(Debug, Clone, PartialEq)]
pub struct ValidateError {
    pub msg: String,
    /// 1-based source line; 0 when the position is not known. An editor needs
    /// this to put the squiggle in the right place.
    pub line: usize,
}
impl std::fmt::Display for ValidateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Prefix the position when we know it, so a plain `{e}` in the CLI reads
        // the way a compiler diagnostic should. Consumers that want the parts
        // separately (the language server) use the fields.
        if self.line > 0 {
            write!(f, "line {}: {}", self.line, self.msg)
        } else {
            write!(f, "{}", self.msg)
        }
    }
}

/// Validate a whole module.  `Ok(())` means the backend may assume well-formed,
/// well-typed IR.
pub fn validate(m: &Module, reg: &Registry) -> Result<(), Vec<ValidateError>> {
    let mut errs: Vec<ValidateError> = Vec::new();
    // Every diagnostic carries a line (0 when the position is not known), so an
    // editor can put the squiggle where the problem is.
    let mut push = |msg: String, line: usize| errs.push(ValidateError { msg, line });

    let subs: Vec<_> = m.subs().collect();
    let forms: Vec<_> = m.forms().collect();
    let sub_names: HashSet<&str> = subs.iter().map(|s| s.name.as_str()).collect();
    let by_name: HashMap<&str, &Sub> = subs.iter().map(|s| (s.name.as_str(), *s)).collect();

    // Subroutines become callable names in the same pass that resolves
    // commands, so `add(1, 2)` and `length("hi")` are looked up the same way.
    // A user sub may not take a command's name: silently shadowing `length`
    // would change what every existing call in the file means.
    let mut with_subs = reg.clone();
    for name in with_subs.register_subs(m) {
        let line = by_name.get(name.as_str()).map_or(0, |s| s.line);
        push(
            format!(
                "subroutine `{name}` has the same name as a library command — \
                 rename the subroutine"
            ),
            line,
        );
    }
    let reg = &with_subs;

    // --- entry point -----------------------------------------------------
    // What counts as a valid entry depends on the target: a GUI module is
    // entered through its form, a console module needs `main`, and a library
    // has no entry at all — it is called by a host.
    let target = m.target();
    if target.is_executable() {
        if forms.is_empty() && !sub_names.contains("main") {
            push("module has no `main` subroutine and no `form` (nothing to run)".into(), 0);
        }
        if target == Target::Console && !forms.is_empty() {
            push(
                "`target console` but the module declares a form — use `target gui`".into(),
                0,
            );
        }
        if target == Target::Gui && forms.is_empty() {
            push("`target gui` but the module declares no form".into(), 0);
        }
    } else {
        if subs.is_empty() {
            push(
                format!(
                    "`target {}` exports nothing — a library needs at least one subroutine",
                    target.as_str()
                ),
                0,
            );
        }
        if !forms.is_empty() {
            push(
                format!(
                    "`target {}` cannot declare a form — build a GUI module as `target gui`",
                    target.as_str()
                ),
                0,
            );
        }
    }
    if forms.len() > 1 {
        push(format!(
            "v0.2 supports one form per module, found {}",
            forms.len()
        ), 0);
    }

    // --- duplicate names -------------------------------------------------
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for s in &subs {
        *seen.entry(s.name.as_str()).or_insert(0) += 1;
    }
    for (name, n) in seen {
        if n > 1 {
            push(format!("subroutine `{name}` is defined {n} times"), 0);
        }
    }

    // --- the no-parameter contract ---------------------------------------
    // An entry point and an event handler are both invoked by generated code
    // that has no arguments to pass and nowhere to put a result. Saying so here
    // is the difference between a compile error and a call through a mismatched
    // function pointer at run time.
    if let Some(main) = by_name.get("main") {
        if !main.is_plain() && target.is_executable() {
            push(
                "`main` is the program entry: it takes no parameters and returns nothing"
                    .into(),
                main.line,
            );
        }
    }

    // --- forms and components -------------------------------------------
    for form in &forms {
        let mut ids: HashSet<&str> = HashSet::new();
        check_component_like(
            reg,
            "form",
            &form.name,
            &form.properties,
            &form.handlers,
            &by_name,
            &mut push,
        );
        for child in &form.children {
            if !ids.insert(child.id.as_str()) {
                push(format!(
                    "form `{}`: duplicate component id `{}`",
                    form.name, child.id
                ), 0);
            }
            check_component(reg, form.name.as_str(), child, &by_name, &mut push);
        }
    }

    // Component ids are module-scoped: every subroutine can address them.
    let mut components: Components = Components::new();
    for form in &forms {
        for child in &form.children {
            components.insert(child.id.clone(), child.type_name.clone());
        }
    }

    // Module-level variables, and their types.
    let globals: Vec<_> = m.globals().collect();
    let mut global_types: HashMap<String, Ty> = HashMap::new();
    for g in &globals {
        // A global's initializer may call commands but must not read another
        // global: order-dependent global initialisation is a swamp, and a clear
        // error now beats a subtle one later.
        if let Err(e) = check_initializer(&g.value, &global_types, reg) {
            push(format!("in initializer of `{}`: {e}", g.name), 0);
        }
        match type_of_expr_hinted(&g.value, Some(g.ty), &HashMap::new(), reg, &components) {
            Ok(got) if got == g.ty => {}
            Ok(got) => push(format!(
                "`var {}` declared {} but its initializer is {}",
                g.name,
                g.ty.as_str(),
                got.as_str()
            ), 0),
            Err(e) => push(format!("in initializer of `{}`: {e}", g.name), 0),
        }
        if global_types.insert(g.name.clone(), g.ty).is_some() {
            push(format!(
                "module variable `{}` is declared more than once",
                g.name
            ), 0);
        }
    }

    // Module variables, component ids and subroutine names share ONE namespace.
    // `count = 5` and `count.text = "x"` naming the same thing would be
    // incoherent, so a collision is an error while it is still cheap to say so.
    for name in global_types.keys() {
        if components.contains_key(name) {
            push(format!(
                "`{name}` is both a module variable and a component id"
            ), 0);
        }
        if sub_names.contains(name.as_str()) {
            push(format!(
                "`{name}` is both a module variable and a subroutine"
            ), 0);
        }
    }
    for id in components.keys() {
        if sub_names.contains(id.as_str()) {
            push(format!("`{id}` is both a component id and a subroutine"), 0);
        }
    }

    // --- subroutine bodies -----------------------------------------------
    for item in &m.items {
        let Item::Sub(sub) = item else { continue };
        // Module variables are in scope in every subroutine.
        let mut vars: HashMap<String, Ty> = global_types.clone();
        let mut mutable_locals: HashSet<String> = HashSet::new();
        // Locals must be tracked apart from module variables: `vars` is seeded
        // with the globals so they resolve, but they are not locals and the
        // immutability rule differs.
        let mut local_names: HashSet<String> = HashSet::new();
        // Parameters are in scope for the whole body and are immutable:
        // reassigning one would make the call site's argument a lie about what
        // the subroutine is working with.
        let mut param_names: HashSet<String> = HashSet::new();
        // Loop variables are immutable for the same reason parameters are —
        // the loop, not the body, decides what comes next — but they earn
        // their own diagnostic, because "declare it with `var`" is not advice
        // an author can act on here.
        let mut loop_vars: HashSet<String> = HashSet::new();
        for (pname, pty) in &sub.params {
            if vars.insert(pname.clone(), *pty).is_some() {
                push(
                    format!(
                        "in `{}`: parameter `{pname}` has the same name as a module variable",
                        sub.name
                    ),
                    sub.line,
                );
            }
            local_names.insert(pname.clone());
            param_names.insert(pname.clone());
        }
        // Walk nested blocks. Locals are **function-scoped** in v0.3: a `let`
        // inside an `if` is visible after it, matching the alloca-at-top
        // lowering. Block scoping is a later refinement.
        let mut stack: Vec<&Vec<Stmt>> = vec![&sub.body];
        while let Some(block) = stack.pop() {
            for stmt in block {
                match &stmt.kind {
                    StmtKind::Let {
                        name,
                        ty,
                        value,
                        mutable,
                    } => {
                        match type_of_expr_hinted(value, Some(*ty), &vars, reg, &components) {
                            Ok(got) if got == *ty => {}
                            Ok(got) => push(format!(
                                "in `{}`: `let {name}` declared {} but expression is {}",
                                sub.name,
                                ty.as_str(),
                                got.as_str()
                            ), stmt.line),
                            Err(e) => push(format!("in `{}`: {}", sub.name, e), stmt.line),
                        }
                        if vars.insert(name.clone(), *ty).is_some() {
                            push(format!(
                                "in `{}`: variable `{name}` is defined more than once",
                                sub.name
                            ), stmt.line);
                        }
                        local_names.insert(name.clone());
                        if *mutable {
                            mutable_locals.insert(name.clone());
                        }
                    }
                    StmtKind::Assign { name, value } => {
                        // Resolve against locals first, then module variables.
                        let target = vars
                            .get(name)
                            .copied()
                            .or_else(|| global_types.get(name).copied());
                        match target {
                            None => push(format!(
                                "in `{}`: assignment to undefined variable `{name}`",
                                sub.name
                            ), stmt.line),
                            Some(expected) => {
                                let is_local = local_names.contains(name);
                                let is_mutable = if is_local {
                                    mutable_locals.contains(name)
                                } else {
                                    true // module variables are always `var`
                                };
                                if !is_mutable {
                                    if loop_vars.contains(name) {
                                        push(format!(
                                            "in `{}`: `{name}` is the loop variable of a `for` and cannot be assigned to",
                                            sub.name
                                        ), stmt.line);
                                    } else if param_names.contains(name) {
                                        push(format!(
                                            "in `{}`: `{name}` is a parameter and cannot be assigned to",
                                            sub.name
                                        ), stmt.line);
                                    } else {
                                        push(format!(
                                        "in `{}`: `{name}` is immutable — declare it with `var` instead of `let` to allow assignment",
                                        sub.name
                                    ), stmt.line);
                                    }
                                }
                                match type_of_expr_hinted(
                                    value, Some(expected), &vars, reg, &components,
                                ) {
                                    Ok(got) if got == expected => {}
                                    Ok(got) => push(format!(
                                        "in `{}`: `{name}` is {}, cannot assign {}",
                                        sub.name,
                                        expected.as_str(),
                                        got.as_str()
                                    ), stmt.line),
                                    Err(e) => push(format!("in `{}`: {}", sub.name, e), stmt.line),
                                }
                            }
                        }
                    }
                    // `call` reaches commands and user subroutines alike; the
                    // two are resolved in one namespace, commands first.
                    StmtKind::Call { cmd, args } => match callee(cmd, reg) {
                        None => push(format!("in `{}`: unknown command `{cmd}`", sub.name), stmt.line),
                        Some((what, sig)) => {
                            if let Err(e) = check_args_labeled(
                                what, cmd, &sig.params, args, &vars, reg, &components,
                            ) {
                                push(format!("in `{}`: {}", sub.name, e), stmt.line);
                            }
                        }
                    },
                    StmtKind::If { arms, otherwise } => {
                        for (cond, body) in arms {
                            check_condition(cond, &vars, reg, &components, &sub.name, stmt.line, &mut push);
                            stack.push(body);
                        }
                        if let Some(body) = otherwise {
                            stack.push(body);
                        }
                    }
                    StmtKind::While { cond, body } => {
                        check_condition(cond, &vars, reg, &components, &sub.name, stmt.line, &mut push);
                        stack.push(body);
                    }
                    StmtKind::For {
                        var,
                        start,
                        limit,
                        step: _,
                        body,
                    } => {
                        for (what, e) in [("start", start), ("limit", limit)] {
                            match type_of_expr_in(e, &vars, reg, &components) {
                                Ok(Ty::Int) => {}
                                Ok(other) => push(format!(
                                    "in `{}`: a `for` counts with `int` values — its {what} value is {}",
                                    sub.name,
                                    other.as_str()
                                ), stmt.line),
                                Err(e) => push(format!("in `{}`: {}", sub.name, e), stmt.line),
                            }
                        }
                        if vars.insert(var.clone(), Ty::Int).is_some() {
                            push(format!(
                                "in `{}`: variable `{var}` is defined more than once",
                                sub.name
                            ), stmt.line);
                        }
                        local_names.insert(var.clone());
                        loop_vars.insert(var.clone());
                        stack.push(body);
                    }
                    // Placement is checked separately: this worklist flattens
                    // the body, so it cannot tell what is inside a loop.
                    StmtKind::Break | StmtKind::Continue => {}
                    StmtKind::Return { value } => match (value, sub.ret) {
                        (None, None) => {}
                        (None, Some(want)) => push(format!(
                            "in `{}`: `return` needs a value — `{}` returns {}",
                            sub.name,
                            sub.name,
                            want.as_str()
                        ), stmt.line),
                        (Some(_), None) => push(format!(
                            "in `{}`: `{}` declares no return type, so `return` cannot carry a value",
                            sub.name, sub.name
                        ), stmt.line),
                        (Some(e), Some(want)) => {
                            match type_of_expr_hinted(e, Some(want), &vars, reg, &components) {
                                Ok(got) if got == want => {}
                                Ok(got) => push(format!(
                                    "in `{}`: `{}` returns {}, but this `return` gives {}",
                                    sub.name,
                                    sub.name,
                                    want.as_str(),
                                    got.as_str()
                                ), stmt.line),
                                Err(e) => push(format!("in `{}`: {}", sub.name, e), stmt.line),
                            }
                        }
                    },
                    // `xs[i] = v` changes the array, not the binding, so it is
                    // allowed on a `let` — see StmtKind::SetIndex.
                    StmtKind::SetIndex { name, index, value } => {
                        let target = vars.get(name).copied();
                        match target {
                            None => push(format!(
                                "in `{}`: assignment to undefined variable `{name}`",
                                sub.name
                            ), stmt.line),
                            Some(Ty::Array(_)) | Some(Ty::Bytes) => {
                                let expected = match target {
                                    Some(Ty::Array(e)) => e.ty(),
                                    // A byte is written as a number, the same
                                    // way it is read.
                                    _ => Ty::Int,
                                };
                                match type_of_expr_in(index, &vars, reg, &components) {
                                    Ok(Ty::Int) => {
                                        if let Expr::IntLit(v) = index {
                                            if *v < 0 {
                                                push(format!(
                                                    "in `{}`: index {v} is before the start of `{name}`",
                                                    sub.name
                                                ), stmt.line);
                                            }
                                        }
                                    }
                                    Ok(other) => push(format!(
                                        "in `{}`: an index counts with `int` values, got {}",
                                        sub.name,
                                        other.as_str()
                                    ), stmt.line),
                                    Err(e) => push(format!("in `{}`: {}", sub.name, e), stmt.line),
                                }
                                match type_of_expr_hinted(
                                    value, Some(expected), &vars, reg, &components,
                                ) {
                                    Ok(got) if got == expected => {}
                                    Ok(got) => push(format!(
                                        "in `{}`: `{name}` holds {} values, cannot store {}",
                                        sub.name,
                                        expected.as_str(),
                                        got.as_str()
                                    ), stmt.line),
                                    Err(e) => push(format!("in `{}`: {}", sub.name, e), stmt.line),
                                }
                            }
                            Some(other) => push(format!(
                                "in `{}`: `{name}` is {} — only an array or a byte-set has elements",
                                sub.name,
                                other.as_str()
                            ), stmt.line),
                        }
                    }
                    StmtKind::SetProperty {
                        component,
                        property,
                        value,
                    } => match property_type(component, property, reg, &components) {
                        Err(e) => push(format!("in `{}`: {}", sub.name, e), stmt.line),
                        Ok(expected) => match type_of_expr_in(value, &vars, reg, &components) {
                            Ok(got) if got == expected => {}
                            Ok(got) => push(format!(
                                "in `{}`: `{component}.{property}` expects {}, got {}",
                                sub.name,
                                expected.as_str(),
                                got.as_str()
                            ), stmt.line),
                            Err(e) => push(format!("in `{}`: {}", sub.name, e), stmt.line),
                        },
                    },
                }
            }
        }

        check_loop_control(&sub.body, false, &sub.name, &mut push);

        // A value-returning subroutine must return on every path. Falling off
        // the end would hand the caller whatever happened to be in the return
        // register — the class of bug that is hardest to see and worst to hit.
        if let Some(want) = sub.ret {
            if !always_returns(&sub.body) {
                push(
                    format!(
                        "`{}` returns {} but can reach its `end` without a `return`",
                        sub.name,
                        want.as_str()
                    ),
                    sub.line,
                );
            }
        }
    }

    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
    }
}

/// Whether every path through `body` reaches a `return`.
///
/// An `if` counts only when it has an `else` **and** every branch returns: a
/// missing `else` is exactly the path that falls through. A `while` never
/// counts — its condition may be false the first time it is tested.
fn always_returns(body: &[Stmt]) -> bool {
    body.iter().any(|s| match &s.kind {
        StmtKind::Return { .. } => true,
        StmtKind::If {
            arms,
            otherwise: Some(else_body),
        } => arms.iter().all(|(_, b)| always_returns(b)) && always_returns(else_body),
        _ => false,
    })
}

/// `break` and `continue` mean nothing outside a loop, and a jump to nowhere
/// is worth catching here rather than in the backend. The body walk in
/// `validate` is a flat worklist and cannot carry nesting depth, so this is a
/// separate recursion.
fn check_loop_control(
    body: &[Stmt],
    in_loop: bool,
    sub: &str,
    push: &mut impl FnMut(String, usize),
) {
    for stmt in body {
        match &stmt.kind {
            StmtKind::Break | StmtKind::Continue if !in_loop => {
                let word = if matches!(stmt.kind, StmtKind::Break) {
                    "break"
                } else {
                    "continue"
                };
                push(
                    format!("in `{sub}`: `{word}` is only meaningful inside a `while` or `for`"),
                    stmt.line,
                );
            }
            StmtKind::While { body, .. } | StmtKind::For { body, .. } => {
                check_loop_control(body, true, sub, push)
            }
            StmtKind::If { arms, otherwise } => {
                for (_, b) in arms {
                    check_loop_control(b, in_loop, sub, push);
                }
                if let Some(b) = otherwise {
                    check_loop_control(b, in_loop, sub, push);
                }
            }
            _ => {}
        }
    }
}

/// A loop or branch condition must be a truth value.
fn check_condition(
    cond: &Expr,
    vars: &HashMap<String, Ty>,
    reg: &Registry,
    components: &Components,
    sub: &str,
    line: usize,
    push: &mut impl FnMut(String, usize),
) {
    match type_of_expr_in(cond, vars, reg, components) {
        Ok(Ty::Bool) => {}
        Ok(other) => push(
            format!("in `{sub}`: condition must be a truth value, found {}", other.as_str()),
            line,
        ),
        Err(e) => push(format!("in `{sub}`: {e}"), line),
    }
}

/// A module variable's initializer may call commands but must not read another
/// module variable (see the note where this is called).
fn check_initializer(
    e: &Expr,
    globals: &HashMap<String, Ty>,
    reg: &Registry,
) -> Result<(), String> {
    match e {
        Expr::Var(name) if globals.contains_key(name) => Err(format!(
            "cannot read module variable `{name}` here; module variable initializers may use literals and command calls only"
        )),
        Expr::Var(name) => Err(format!("unknown variable `{name}`")),
        Expr::Bin(_, l, r) => {
            check_initializer(l, globals, reg)?;
            check_initializer(r, globals, reg)
        }
        Expr::Neg(e) | Expr::Not(e) => check_initializer(e, globals, reg),
        Expr::Cmp(_, l, r) | Expr::Logical(_, l, r) => {
            check_initializer(l, globals, reg)?;
            check_initializer(r, globals, reg)
        }
        // A subroutine body may read module variables, so calling one here
        // would reintroduce exactly the ordering hazard this rule exists to
        // prevent — just through one more level of indirection.
        Expr::Call { cmd, .. } if reg.is_sub(cmd) => Err(format!(
            "cannot call subroutine `{cmd}` here; module variable initializers may use literals and command calls only"
        )),
        Expr::Call { args, .. } => {
            for a in args {
                check_initializer(a, globals, reg)?;
            }
            Ok(())
        }
        Expr::ArrayLit(items) => {
            for a in items {
                check_initializer(a, globals, reg)?;
            }
            Ok(())
        }
        Expr::Index { base, index } => {
            check_initializer(base, globals, reg)?;
            check_initializer(index, globals, reg)
        }
        Expr::GetProperty { .. } => {
            Err("cannot read a component property before the form exists".to_string())
        }
        _ => Ok(()),
    }
}

fn check_component(
    reg: &Registry,
    form: &str,
    c: &Component,
    subs: &HashMap<&str, &Sub>,
    push: &mut impl FnMut(String, usize),
) {
    let where_ = format!("{}.{}", form, c.id);
    check_component_like(
        reg,
        &c.type_name,
        &where_,
        &c.properties,
        &c.handlers,
        subs,
        push,
    );
}

/// Shared checking for a form root or a child component: the type must exist,
/// every property must be declared by that type with a matching value type, and
/// every event must exist and bind to a real subroutine.
fn check_component_like(
    reg: &Registry,
    type_name: &str,
    where_: &str,
    properties: &[(String, Expr)],
    handlers: &[(String, String)],
    subs: &HashMap<&str, &Sub>,
    push: &mut impl FnMut(String, usize),
) {
    let Some(desc) = reg.component(type_name) else {
        let mut known: Vec<&str> = reg.component_names().collect();
        known.sort_unstable();
        push(format!(
            "`{where_}`: unknown component type `{type_name}`{}",
            if known.is_empty() {
                " (no component library is in scope — add `use ui`)".to_string()
            } else {
                format!(" (known: {})", known.join(", "))
            }
        ), 0);
        return;
    };

    let empty = HashMap::new();
    for (name, value) in properties {
        let Some(prop) = desc.property(name) else {
            let mut known: Vec<&str> = desc.properties.iter().map(|p| p.name.as_str()).collect();
            known.sort_unstable();
            push(format!(
                "`{where_}`: component `{type_name}` has no property `{name}` (has: {})",
                known.join(", ")
            ), 0);
            continue;
        };
        match type_of_expr_in(value, &empty, reg, &Components::new()) {
            Ok(got) if got == prop.ty => {}
            Ok(got) => push(format!(
                "`{where_}`: property `{name}` expects {}, got {}",
                prop.ty.as_str(),
                got.as_str()
            ), 0),
            Err(e) => push(format!("`{where_}`: property `{name}`: {e}"), 0),
        }
    }

    for (event, handler) in handlers {
        if !desc.has_event(event) {
            let known = desc.events.join(", ");
            push(format!(
                "`{where_}`: component `{type_name}` has no event `{event}`{}",
                if known.is_empty() {
                    String::new()
                } else {
                    format!(" (has: {known})")
                }
            ), 0);
        }
        match subs.get(handler.as_str()) {
            None => push(format!(
                "`{where_}`: event `{event}` is bound to `{handler}`, which is not a subroutine in this module"
            ), 0),
            // A handler is called by the UI layer, which has no arguments to
            // supply and discards nothing: it must be a plain sub.
            Some(sub) if !sub.is_plain() => push(format!(
                "`{where_}`: event `{event}` is bound to `{handler}`, which takes parameters or returns a value — an event handler takes no parameters and returns nothing"
            ), sub.line),
            Some(_) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    fn reg() -> Registry {
        Registry::core()
    }

    #[test]
    fn accepts_valid() {
        let m = parse(
            "module m\nsub main\n  let s: text = uppercase(\"hi\")\n  let n: int = length(s)\n  call print_int(n)\nend\n",
        )
        .unwrap();
        assert!(validate(&m, &reg()).is_ok());
    }

    #[test]
    fn rejects_missing_entry() {
        let m = parse("module m\nsub other\n  call print_int(1)\nend\n").unwrap();
        let e = validate(&m, &reg()).unwrap_err();
        assert!(e.iter().any(|e| e.msg.contains("no `main`")));
    }

    #[test]
    fn allows_multiple_subs() {
        // Handlers are subroutines, so multi-sub modules must validate.
        let m = parse(
            "module m\nsub main\n  call print_int(1)\nend\nsub helper\n  call print_int(2)\nend\n",
        )
        .unwrap();
        assert!(validate(&m, &reg()).is_ok(), "{:?}", validate(&m, &reg()));
    }

    #[test]
    fn rejects_type_mismatch_and_unknown_cmd() {
        let m =
            parse("module m\nsub main\n  let x: int = \"nope\"\n  call frob(1)\nend\n").unwrap();
        assert!(validate(&m, &reg()).unwrap_err().len() >= 2);
    }

    #[test]
    fn rejects_void_in_expression() {
        let m = parse("module m\nsub main\n  let x: int = print_int(1)\nend\n").unwrap();
        assert!(validate(&m, &reg()).is_err());
    }

    #[test]
    fn rejects_reassigning_a_let() {
        let m = parse("module m\nsub main\n  let x: int = 1\n  x = 2\nend\n").unwrap();
        let e = validate(&m, &reg()).unwrap_err();
        assert!(
            e.iter()
                .any(|e| e.msg.contains("immutable") && e.msg.contains("var")),
            "the error should name the fix: {e:?}"
        );
    }

    /// A diagnostic without a position is useless to an editor: it either has
    /// no squiggle or squiggles the wrong line. Pin the exact lines so the
    /// language server can trust them.
    #[test]
    fn reports_the_line_of_the_offending_statement() {
        //            1         2          3                4        5             6
        let src = "module m\nsub main\n  let x: int = 1\n  x = 2\n  call nope()\nend\n";
        let m = parse(src).unwrap();
        let e = validate(&m, &reg()).unwrap_err();

        let immutable = e.iter().find(|e| e.msg.contains("immutable")).unwrap();
        assert_eq!(immutable.line, 4, "the assignment is on line 4: {e:?}");

        let unknown = e.iter().find(|e| e.msg.contains("unknown command")).unwrap();
        assert_eq!(unknown.line, 5, "the call is on line 5: {e:?}");

        // And the position must survive into the rendered message.
        assert!(
            immutable.to_string().starts_with("line 4:"),
            "Display should carry the position: {immutable}"
        );
    }

    #[test]
    fn accepts_assigning_a_var() {
        let m = parse("module m\nsub main\n  var x: int = 1\n  x = 2\nend\n").unwrap();
        assert!(validate(&m, &reg()).is_ok());
    }

    #[test]
    fn rejects_global_reading_another_global() {
        let m =
            parse("module m\nvar a: int = 1\nvar b: int = a\nsub main\n  call print_int(b)\nend\n")
                .unwrap();
        assert!(validate(&m, &reg()).is_err());
    }

    #[test]
    fn rejects_assigning_undefined() {
        let m = parse("module m\nsub main\n  nope = 1\nend\n").unwrap();
        assert!(validate(&m, &reg()).is_err());
    }

    #[test]
    fn globals_are_visible_in_subs() {
        let m =
            parse("module m\nvar n: int = 5\nsub main\n  n = n + 1\n  call print_int(n)\nend\n")
                .unwrap();
        assert!(validate(&m, &reg()).is_ok(), "{:?}", validate(&m, &reg()));
    }

    #[test]
    fn rejects_non_bool_condition() {
        let m = parse("module m\nsub main\n  if 5\n    call print_int(1)\n  end\nend\n").unwrap();
        let e = validate(&m, &reg()).unwrap_err();
        assert!(e.iter().any(|e| e.msg.contains("truth value")), "{e:?}");
    }

    #[test]
    fn rejects_ordering_on_text() {
        let m = parse(
            "module m\nsub main\n  let t: text = \"a\"\n  if t < \"b\"\n    call print_int(1)\n  end\nend\n",
        )
        .unwrap();
        assert!(validate(&m, &reg()).is_err());
    }

    #[test]
    fn accepts_text_equality_and_while() {
        let m = parse(
            "module m\nsub main\n  var n: int = 0\n  while n < 3\n    n = n + 1\n  end\n  if \"a\" = \"a\"\n    call print_int(n)\n  end\nend\n",
        )
        .unwrap();
        assert!(validate(&m, &reg()).is_ok(), "{:?}", validate(&m, &reg()));
    }

    // --- subroutine parameters, returns and calls -------------------------

    fn errors(src: &str) -> Vec<ValidateError> {
        validate(&parse(src).unwrap(), &reg()).unwrap_err()
    }
    fn accepts(src: &str) {
        let m = parse(src).unwrap();
        assert!(validate(&m, &reg()).is_ok(), "{:?}", validate(&m, &reg()));
    }

    #[test]
    fn accepts_a_sub_with_parameters_and_a_return() {
        accepts(
            "module m\nsub add(a: int, b: int): int\n  return a + b\nend\nsub main\n  call print_int(add(1, 2))\nend\n",
        );
    }

    #[test]
    fn accepts_recursion() {
        accepts(
            "module m\nsub fib(n: int): int\n  if n < 2\n    return n\n  end\n  return fib(n - 1) + fib(n - 2)\nend\nsub main\n  call print_int(fib(10))\nend\n",
        );
    }

    #[test]
    fn rejects_wrong_argument_count_with_a_line() {
        //            1         2                      3                4        5
        let src = "module m\nsub add(a: int, b: int): int\n  return a + b\nend\nsub main\n  call print_int(add(1))\nend\n";
        let e = errors(src);
        let bad = e
            .iter()
            .find(|e| e.msg.contains("expects 2 argument(s), got 1"))
            .unwrap_or_else(|| panic!("{e:?}"));
        assert!(bad.msg.contains("subroutine `add`"), "{bad:?}");
        assert_eq!(bad.line, 6, "the call is on line 6: {e:?}");
    }

    #[test]
    fn rejects_wrong_argument_type() {
        let e = errors(
            "module m\nsub add(a: int, b: int): int\n  return a + b\nend\nsub main\n  call print_int(add(1, \"two\"))\nend\n",
        );
        assert!(
            e.iter()
                .any(|e| e.msg.contains("argument 2 expects int, got text")),
            "{e:?}"
        );
    }

    #[test]
    fn rejects_a_missing_return() {
        let e = errors(
            "module m\nsub pick(n: int): int\n  if n > 0\n    return 1\n  end\nend\nsub main\n  call print_int(pick(1))\nend\n",
        );
        let bad = e
            .iter()
            .find(|e| e.msg.contains("without a `return`"))
            .unwrap_or_else(|| panic!("{e:?}"));
        assert_eq!(bad.line, 2, "the subroutine header is on line 2");
    }

    /// An `if`/`else` where both sides return is a complete path — refusing it
    /// would force a pointless trailing `return`.
    #[test]
    fn an_if_else_that_both_return_is_enough() {
        accepts(
            "module m\nsub sign(n: int): int\n  if n > 0\n    return 1\n  else\n    return 0\n  end\nend\nsub main\n  call print_int(sign(3))\nend\n",
        );
    }

    /// A `while` may never run, so it cannot be the thing that returns.
    #[test]
    fn a_while_does_not_count_as_returning() {
        let e = errors(
            "module m\nsub f(n: int): int\n  while n > 0\n    return 1\n  end\nend\nsub main\n  call print_int(f(1))\nend\n",
        );
        assert!(e.iter().any(|e| e.msg.contains("without a `return`")), "{e:?}");
    }

    #[test]
    fn rejects_a_return_of_the_wrong_type() {
        let e = errors(
            "module m\nsub f(): int\n  return \"nope\"\nend\nsub main\n  call print_int(f())\nend\n",
        );
        assert!(
            e.iter().any(|e| e.msg.contains("returns int, but this `return` gives text")),
            "{e:?}"
        );
    }

    #[test]
    fn rejects_a_value_from_a_sub_that_returns_nothing() {
        let e = errors("module m\nsub main\n  return 1\nend\n");
        assert!(
            e.iter().any(|e| e.msg.contains("declares no return type")),
            "{e:?}"
        );
    }

    #[test]
    fn rejects_a_void_sub_used_as_a_value() {
        let e = errors("module m\nsub greet(who: text)\n  call print_text(who)\nend\nsub main\n  let x: int = greet(\"a\")\nend\n");
        assert!(
            e.iter()
                .any(|e| e.msg.contains("subroutine `greet` returns nothing")),
            "{e:?}"
        );
    }

    /// A user sub must not silently take a library command's name: every
    /// existing call to `length` would quietly mean something else.
    #[test]
    fn rejects_a_sub_that_shadows_a_command() {
        let e = errors("module m\nsub length(s: text): int\n  return 0\nend\nsub main\n  call print_int(length(\"a\"))\nend\n");
        let bad = e
            .iter()
            .find(|e| e.msg.contains("same name as a library command"))
            .unwrap_or_else(|| panic!("{e:?}"));
        assert_eq!(bad.line, 2);
    }

    #[test]
    fn rejects_assigning_to_a_parameter() {
        let e = errors("module m\nsub f(a: int): int\n  a = 2\n  return a\nend\nsub main\n  call print_int(f(1))\nend\n");
        assert!(
            e.iter().any(|e| e.msg.contains("is a parameter and cannot be assigned to")),
            "{e:?}"
        );
    }

    #[test]
    fn rejects_an_entry_point_with_parameters() {
        let e = errors("module m\nsub main(a: int)\n  call print_int(a)\nend\n");
        assert!(
            e.iter().any(|e| e.msg.contains("`main` is the program entry")),
            "{e:?}"
        );
    }

    #[test]
    fn rejects_calling_a_sub_from_a_global_initializer() {
        let e = errors("module m\nvar n: int = f()\nsub f(): int\n  return 1\nend\nsub main\n  call print_int(n)\nend\n");
        assert!(
            e.iter().any(|e| e.msg.contains("cannot call subroutine `f` here")),
            "{e:?}"
        );
    }

    // --- loops, negation and the text `+` -------------------------------

    #[test]
    fn accepts_for_break_and_continue() {
        accepts("module m\nsub main\n  for i = 1 to 10\n    if i = 3\n      continue\n    end\n    if i = 8\n      break\n    end\n    call print_int(i)\n  end\nend\n");
    }

    #[test]
    fn rejects_break_outside_a_loop() {
        let e = errors("module m\nsub main\n  break\nend\n");
        assert!(
            e.iter()
                .any(|e| e.msg.contains("`break` is only meaningful inside") && e.line == 3),
            "{e:?}"
        );
    }

    /// A `break` in an `if` that is itself inside a loop is fine; the same
    /// `if` outside one is not. The placement check must follow branches
    /// without treating them as loops.
    #[test]
    fn a_break_inside_an_if_inherits_the_enclosing_loop() {
        accepts("module m\nsub main\n  while true\n    if true\n      break\n    end\n  end\nend\n");
        let e = errors("module m\nsub main\n  if true\n    continue\n  end\nend\n");
        assert!(
            e.iter().any(|e| e.msg.contains("`continue` is only meaningful")),
            "{e:?}"
        );
    }

    #[test]
    fn rejects_assigning_to_a_loop_variable() {
        let e = errors("module m\nsub main\n  for i = 1 to 3\n    i = 9\n  end\nend\n");
        assert!(
            e.iter()
                .any(|e| e.msg.contains("is the loop variable of a `for`")),
            "{e:?}"
        );
    }

    #[test]
    fn rejects_a_non_integer_loop_bound() {
        let e = errors("module m\nsub main\n  for i = 1 to 2.5\n  end\nend\n");
        assert!(
            e.iter()
                .any(|e| e.msg.contains("counts with `int` values") && e.msg.contains("limit")),
            "{e:?}"
        );
    }

    /// Locals are function-scoped, so two loops in one subroutine cannot share
    /// a variable name. Pinned deliberately: it is the existing scoping rule
    /// applied to loop variables, not an accident of `for`.
    #[test]
    fn two_loops_cannot_share_a_variable_name() {
        let e = errors("module m\nsub main\n  for i = 1 to 3\n  end\n  for i = 1 to 3\n  end\nend\n");
        assert!(
            e.iter().any(|e| e.msg.contains("`i` is defined more than once")),
            "{e:?}"
        );
    }

    #[test]
    fn a_loop_variable_is_readable_and_typed_int() {
        accepts("module m\nsub main\n  for i = 1 to 3\n    call print_int(i + 1)\n  end\nend\n");
        let e = errors("module m\nsub main\n  for i = 1 to 3\n    call print_text(i)\n  end\nend\n");
        assert!(!e.is_empty());
    }

    #[test]
    fn text_joins_with_plus_but_only_with_plus() {
        accepts("module m\nsub main\n  let s: text = \"a\" + \"b\"\n  call print_text(s)\nend\n");
        let e = errors("module m\nsub main\n  let s: text = \"a\" - \"b\"\n  call print_text(s)\nend\n");
        assert!(
            e.iter().any(|e| e.msg.contains("text supports `+` (joining)")),
            "{e:?}"
        );
    }

    #[test]
    fn plus_does_not_mix_text_with_numbers() {
        let e = errors("module m\nsub main\n  let s: text = \"a\" + 1\n  call print_text(s)\nend\n");
        assert!(!e.is_empty(), "text + int must not compile");
    }

    #[test]
    fn negation_needs_a_number() {
        accepts("module m\nsub main\n  var n: int = 3\n  call print_int(-n)\nend\n");
        let e = errors("module m\nsub main\n  let s: text = \"a\"\n  call print_text(-s)\nend\n");
        assert!(
            e.iter().any(|e| e.msg.contains("`-` negates numbers")),
            "{e:?}"
        );
    }

    /// A module variable's initializer may not read another one — including
    /// through a negation, which the recursion has to look inside.
    #[test]
    fn a_global_initializer_cannot_negate_another_global() {
        let e = errors("module m\nvar a: int = 1\nvar b: int = -a\nsub main\n  call print_int(b)\nend\n");
        assert!(
            e.iter().any(|e| e.msg.contains("cannot read module variable `a`")),
            "{e:?}"
        );
    }

    /// The aggregate rules, each with the line of the statement that broke it.
    #[test]
    fn array_diagnostics() {
        for (src, want) in [
            ("let xs: int[] = [1, \"two\"]", "element 2 is text"),
            ("let xs: int[] = [1]\n  let a: int = xs[-1]", "before the start"),
            ("let a: int = [1, 2][5]", "past the end"),
            ("let xs: int[] = [1]\n  call print_int(xs)", "expects int, got int[]"),
            ("let xs: int[] = [1]\n  let n: int = count(xs, 1)", "expects 1 argument"),
            ("let n: int = count(5)", "expects an array"),
            ("let xs: text[] = []\n  let n: int = index_of(xs, 1)", "must match what the array holds"),
            ("let xs: int[] = []\n  let a: int = xs[\"one\"]", "an index counts with `int`"),
            ("let a: int = 1\n  a[0] = 2", "only an array or a byte-set has elements"),
            ("let xs: int[] = []\n  xs[0] = \"no\"", "holds int values, cannot store text"),
        ] {
            let m = parse(&format!("module m\nsub main\n  {src}\nend\n")).unwrap();
            let errs = validate(&m, &reg()).unwrap_err();
            assert!(
                errs.iter().any(|e| e.msg.contains(want) && e.line > 0),
                "expected {want:?} with a line, got {errs:?}"
            );
        }
    }

    /// An empty list has no element to infer from, so the declaration is the
    /// only thing that can say what it holds — and without one it must be an
    /// error rather than a guess.
    #[test]
    fn an_empty_list_needs_a_declared_type() {
        let ok = parse("module m\nsub main\n  var xs: text[] = []\n  call print_int(count(xs))\nend\n")
            .unwrap();
        assert!(validate(&ok, &reg()).is_ok(), "{:?}", validate(&ok, &reg()));

        let bad = parse("module m\nsub main\n  call print_int(count([]))\nend\n").unwrap();
        let errs = validate(&bad, &reg()).unwrap_err();
        assert!(
            errs.iter().any(|e| e.msg.contains("does not say what it holds")),
            "{errs:?}"
        );
    }

    /// `append` is declared over "any array" and "whatever it holds", so the
    /// checker has to give the call back the concrete type it was handed.
    #[test]
    fn a_generic_command_yields_a_concrete_type() {
        let m = parse(
            "module m\nsub main\n  var xs: text[] = []\n  xs = append(xs, \"a\")\n  \
             call print_text(join(xs, \",\"))\nend\n",
        )
        .unwrap();
        assert!(validate(&m, &reg()).is_ok(), "{:?}", validate(&m, &reg()));

        let bad = parse(
            "module m\nsub main\n  var xs: text[] = []\n  var ns: int[] = append(xs, \"a\")\nend\n",
        )
        .unwrap();
        assert!(validate(&bad, &reg()).is_err());
    }

    /// Elements change; the binding does not. `let` promises the name keeps
    /// meaning the same array, which is why writing one element is allowed.
    #[test]
    fn an_element_of_a_let_array_may_be_assigned() {
        let m = parse("module m\nsub main\n  let xs: int[] = [1]\n  xs[0] = 2\nend\n").unwrap();
        assert!(validate(&m, &reg()).is_ok(), "{:?}", validate(&m, &reg()));
    }

    #[test]
    fn a_byte_set_is_indexed_as_numbers() {
        let m = parse(
            "module m\nsub main\n  var b: bytes = bytes_new(2)\n  b[0] = 65\n  \
             call print_int(b[0])\nend\n",
        )
        .unwrap();
        assert!(validate(&m, &reg()).is_ok(), "{:?}", validate(&m, &reg()));
    }

    #[test]
    fn rejects_mixed_numeric() {
        let m = parse("module m\nsub main\n  let d: double = 1.5\n  let x: double = d + 1\nend\n")
            .unwrap();
        assert!(validate(&m, &reg()).is_err());
    }
}
