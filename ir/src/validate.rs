//! IR validator (PRD §5.1): rejects malformed / ill-typed IR before lowering.
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
//! Line numbers are not yet threaded from the parser onto IR nodes; diagnostics
//! are message-only for now (a documented follow-up).

use std::collections::{HashMap, HashSet};

use crate::sema::{check_args_in, property_type, type_of_expr_in, Components};
use crate::{Component, Expr, Item, Module, Registry, Stmt, Ty};

#[derive(Debug, Clone, PartialEq)]
pub struct ValidateError {
    pub msg: String,
}
impl std::fmt::Display for ValidateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}

/// Validate a whole module.  `Ok(())` means the backend may assume well-formed,
/// well-typed IR.
pub fn validate(m: &Module, reg: &Registry) -> Result<(), Vec<ValidateError>> {
    let mut errs: Vec<ValidateError> = Vec::new();
    let mut push = |msg: String| errs.push(ValidateError { msg });

    let subs: Vec<_> = m.subs().collect();
    let forms: Vec<_> = m.forms().collect();
    let sub_names: HashSet<&str> = subs.iter().map(|s| s.name.as_str()).collect();

    // --- entry point -----------------------------------------------------
    // A GUI module is entered through its form; a console module needs `main`.
    if forms.is_empty() && !sub_names.contains("main") {
        push("module has no `main` subroutine and no `form` (nothing to run)".into());
    }
    if forms.len() > 1 {
        push(format!(
            "v0.2 supports one form per module, found {}",
            forms.len()
        ));
    }

    // --- duplicate names -------------------------------------------------
    let mut seen: HashMap<&str, usize> = HashMap::new();
    for s in &subs {
        *seen.entry(s.name.as_str()).or_insert(0) += 1;
    }
    for (name, n) in seen {
        if n > 1 {
            push(format!("subroutine `{name}` is defined {n} times"));
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
            &sub_names,
            &mut push,
        );
        for child in &form.children {
            if !ids.insert(child.id.as_str()) {
                push(format!(
                    "form `{}`: duplicate component id `{}`",
                    form.name, child.id
                ));
            }
            check_component(reg, form.name.as_str(), child, &sub_names, &mut push);
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
            push(format!("in initializer of `{}`: {e}", g.name));
        }
        match type_of_expr_in(&g.value, &HashMap::new(), reg, &components) {
            Ok(got) if got == g.ty => {}
            Ok(got) => push(format!(
                "`var {}` declared {} but its initializer is {}",
                g.name,
                g.ty.as_str(),
                got.as_str()
            )),
            Err(e) => push(format!("in initializer of `{}`: {e}", g.name)),
        }
        if global_types.insert(g.name.clone(), g.ty).is_some() {
            push(format!(
                "module variable `{}` is declared more than once",
                g.name
            ));
        }
    }

    // Module variables, component ids and subroutine names share ONE namespace.
    // `count = 5` and `count.text = "x"` naming the same thing would be
    // incoherent, so a collision is an error while it is still cheap to say so.
    for name in global_types.keys() {
        if components.contains_key(name) {
            push(format!(
                "`{name}` is both a module variable and a component id"
            ));
        }
        if sub_names.contains(name.as_str()) {
            push(format!(
                "`{name}` is both a module variable and a subroutine"
            ));
        }
    }
    for id in components.keys() {
        if sub_names.contains(id.as_str()) {
            push(format!("`{id}` is both a component id and a subroutine"));
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
        for stmt in &sub.body {
            match stmt {
                Stmt::Let {
                    name,
                    ty,
                    value,
                    mutable,
                } => {
                    match type_of_expr_in(value, &vars, reg, &components) {
                        Ok(got) if got == *ty => {}
                        Ok(got) => push(format!(
                            "in `{}`: `let {name}` declared {} but expression is {}",
                            sub.name,
                            ty.as_str(),
                            got.as_str()
                        )),
                        Err(e) => push(format!("in `{}`: {}", sub.name, e)),
                    }
                    if vars.insert(name.clone(), *ty).is_some() {
                        push(format!(
                            "in `{}`: variable `{name}` is defined more than once",
                            sub.name
                        ));
                    }
                    local_names.insert(name.clone());
                    if *mutable {
                        mutable_locals.insert(name.clone());
                    }
                }
                Stmt::Assign { name, value } => {
                    // Resolve against locals first, then module variables.
                    let target = vars
                        .get(name)
                        .copied()
                        .or_else(|| global_types.get(name).copied());
                    match target {
                        None => push(format!(
                            "in `{}`: assignment to undefined variable `{name}`",
                            sub.name
                        )),
                        Some(expected) => {
                            let is_local = local_names.contains(name);
                            let is_mutable = if is_local {
                                mutable_locals.contains(name)
                            } else {
                                true // module variables are always `var`
                            };
                            if !is_mutable {
                                push(format!(
                                    "in `{}`: `{name}` is immutable — declare it with `var` instead of `let` to allow assignment",
                                    sub.name
                                ));
                            }
                            match type_of_expr_in(value, &vars, reg, &components) {
                                Ok(got) if got == expected => {}
                                Ok(got) => push(format!(
                                    "in `{}`: `{name}` is {}, cannot assign {}",
                                    sub.name,
                                    expected.as_str(),
                                    got.as_str()
                                )),
                                Err(e) => push(format!("in `{}`: {}", sub.name, e)),
                            }
                        }
                    }
                }
                Stmt::Call { cmd, args } => match reg.get(cmd) {
                    None => push(format!("in `{}`: unknown command `{cmd}`", sub.name)),
                    Some(c) => {
                        if let Err(e) =
                            check_args_in(cmd, &c.sig.params, args, &vars, reg, &components)
                        {
                            push(format!("in `{}`: {}", sub.name, e));
                        }
                    }
                },
                Stmt::SetProperty {
                    component,
                    property,
                    value,
                } => match property_type(component, property, reg, &components) {
                    Err(e) => push(format!("in `{}`: {}", sub.name, e)),
                    Ok(expected) => match type_of_expr_in(value, &vars, reg, &components) {
                        Ok(got) if got == expected => {}
                        Ok(got) => push(format!(
                            "in `{}`: `{component}.{property}` expects {}, got {}",
                            sub.name,
                            expected.as_str(),
                            got.as_str()
                        )),
                        Err(e) => push(format!("in `{}`: {}", sub.name, e)),
                    },
                },
            }
        }
    }

    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs)
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
        Expr::Call { args, .. } => {
            for a in args {
                check_initializer(a, globals, reg)?;
            }
            Ok(())
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
    subs: &HashSet<&str>,
    push: &mut impl FnMut(String),
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
    subs: &HashSet<&str>,
    push: &mut impl FnMut(String),
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
        ));
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
            ));
            continue;
        };
        match type_of_expr_in(value, &empty, reg, &Components::new()) {
            Ok(got) if got == prop.ty => {}
            Ok(got) => push(format!(
                "`{where_}`: property `{name}` expects {}, got {}",
                prop.ty.as_str(),
                got.as_str()
            )),
            Err(e) => push(format!("`{where_}`: property `{name}`: {e}")),
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
            ));
        }
        if !subs.contains(handler.as_str()) {
            push(format!(
                "`{where_}`: event `{event}` is bound to `{handler}`, which is not a subroutine in this module"
            ));
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
    fn rejects_mixed_numeric() {
        let m = parse("module m\nsub main\n  let d: double = 1.5\n  let x: double = d + 1\nend\n")
            .unwrap();
        assert!(validate(&m, &reg()).is_err());
    }
}
