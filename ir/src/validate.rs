//! IR validator (PRD §5.1): rejects malformed / ill-typed IR before lowering.
//!
//! Runs the shared type checker (`sema`) over every subroutine and adds the
//! structural rules the type checker doesn't cover (entry subroutine present,
//! no redefinition, `let` type agreement).  Collects *all* errors rather than
//! stopping at the first, so a single run reports every problem.
//!
//! Line numbers are not yet threaded from the parser onto IR nodes; diagnostics
//! are message-only for now (a documented Phase 1 follow-up).

use std::collections::HashMap;

use crate::sema::{check_args, type_of_expr};
use crate::{Item, Module, Registry, Stmt, Ty};

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
    let push = |errs: &mut Vec<ValidateError>, msg: String| errs.push(ValidateError { msg });

    let subs: Vec<_> = m.subs().collect();
    if !subs.iter().any(|s| s.name == "main") {
        push(
            &mut errs,
            "module has no `main` subroutine (the program entry)".into(),
        );
    }
    if subs.len() > 1 {
        push(
            &mut errs,
            "v0.1 supports a single subroutine (`main`); user subroutines arrive with control flow"
                .into(),
        );
    }
    // Duplicate subroutine names.
    let mut seen_sub: HashMap<&str, usize> = HashMap::new();
    for s in &subs {
        *seen_sub.entry(s.name.as_str()).or_insert(0) += 1;
    }
    for (name, n) in seen_sub {
        if n > 1 {
            push(
                &mut errs,
                format!("subroutine `{name}` is defined {n} times"),
            );
        }
    }

    for item in &m.items {
        let Item::Sub(sub) = item;
        let mut vars: HashMap<String, Ty> = HashMap::new();
        for stmt in &sub.body {
            match stmt {
                Stmt::Let { name, ty, value } => {
                    match type_of_expr(value, &vars, reg) {
                        Ok(got) if got == *ty => {}
                        Ok(got) => push(
                            &mut errs,
                            format!(
                                "in `{}`: `let {name}` declared {} but expression is {}",
                                sub.name,
                                ty.as_str(),
                                got.as_str()
                            ),
                        ),
                        Err(e) => push(&mut errs, format!("in `{}`: {}", sub.name, e)),
                    }
                    // Bind the variable regardless, so later statements that use
                    // it don't cascade "undefined" noise.
                    if vars.insert(name.clone(), *ty).is_some() {
                        push(
                            &mut errs,
                            format!(
                                "in `{}`: variable `{name}` is defined more than once",
                                sub.name
                            ),
                        );
                    }
                }
                Stmt::Call { cmd, args } => match reg.get(cmd) {
                    None => push(
                        &mut errs,
                        format!("in `{}`: unknown command `{cmd}`", sub.name),
                    ),
                    Some(c) => {
                        if let Err(e) = check_args(cmd, &c.sig.params, args, &vars, reg) {
                            push(&mut errs, format!("in `{}`: {}", sub.name, e));
                        }
                    }
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
    fn rejects_missing_main() {
        let m = parse("module m\nsub other\n  call print_int(1)\nend\n").unwrap();
        let e = validate(&m, &reg()).unwrap_err();
        assert!(e.iter().any(|e| e.msg.contains("no `main`")));
    }

    #[test]
    fn rejects_type_mismatch_and_unknown_cmd() {
        let m =
            parse("module m\nsub main\n  let x: int = \"nope\"\n  call frob(1)\nend\n").unwrap();
        let e = validate(&m, &reg()).unwrap_err();
        assert!(e.len() >= 2, "expected both errors, got {e:?}");
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
        // `d + 1` mixes double and int -> error (no implicit conversion).
        assert!(validate(&m, &reg()).is_err());
    }
}
