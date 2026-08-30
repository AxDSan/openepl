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

    // --- subroutine bodies -----------------------------------------------
    for item in &m.items {
        let Item::Sub(sub) = item else { continue };
        let mut vars: HashMap<String, Ty> = HashMap::new();
        for stmt in &sub.body {
            match stmt {
                Stmt::Let { name, ty, value } => {
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
    fn rejects_mixed_numeric() {
        let m = parse("module m\nsub main\n  let d: double = 1.5\n  let x: double = d + 1\nend\n")
            .unwrap();
        assert!(validate(&m, &reg()).is_err());
    }
}
