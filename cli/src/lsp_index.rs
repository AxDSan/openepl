//! A token-level symbol index for the language server.
//!
//! Deliberately built from the **lexer**, not the AST. While you are typing, a
//! file usually does not parse but almost always lexes — and go-to-definition,
//! completion and hover have to keep working in exactly that state. An
//! AST-derived index would go dark at the moment it is most needed.
//!
//! This is sound for OpenEPL v0.3 because the validator already enforces a
//! single module-level namespace: a name cannot be both a subroutine and a
//! module variable and a component id. The one genuine ambiguity is **local
//! shadowing** — a `let x` inside a subroutine must not resolve to a module
//! `var x` — so declarations are tracked per enclosing subroutine.

use std::collections::HashMap;

use openepl_ir::lexer::{lex, Spanned, Tok};

/// What kind of thing a name refers to. Drives both the icon a client shows in
/// completion and whether go-to-definition has anywhere to go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymKind {
    Sub,
    Global,
    Local,
    Component,
    /// A component *type* (`Button`), as opposed to an instance.
    ComponentType,
    /// Provided by a support library — defined in C, so it has no definition
    /// site in this file.
    Command,
    Property,
}

/// One occurrence of an identifier in the source.
#[derive(Debug, Clone)]
pub struct Occurrence {
    pub name: String,
    /// 1-based line.
    pub line: usize,
    /// 1-based **byte** column. Callers must convert to UTF-16 for LSP.
    pub col: usize,
    /// Whether this occurrence declares the name.
    pub is_definition: bool,
    pub kind: SymKind,
    /// Enclosing subroutine, if any. `None` at module level or in a form.
    pub scope: Option<String>,
}

impl Occurrence {
    /// Byte column just past the identifier — the end of its range.
    pub fn end_col(&self) -> usize {
        self.col + self.name.len()
    }
}

/// The identifier at `i`, with its span — or `None` if that token is not an
/// identifier (which, mid-edit, it very often is not).
fn ident_at(toks: &[Spanned], i: usize) -> Option<(String, &Spanned)> {
    match toks.get(i) {
        Some(sp) => match &sp.tok {
            Tok::Ident(name) => Some((name.clone(), sp)),
            _ => None,
        },
        None => None,
    }
}

/// Everything the server knows about the names in one document.
#[derive(Debug, Default)]
pub struct Index {
    pub occurrences: Vec<Occurrence>,
    /// Component id -> component type, for resolving `id.property`.
    pub component_types: HashMap<String, String>,
    /// Locals declared in each subroutine, so shadowing resolves correctly.
    locals_by_sub: HashMap<String, Vec<String>>,
}

impl Index {
    /// Build the index. Never fails: a file that does not lex yields whatever
    /// prefix did lex, which is still better than nothing while typing.
    pub fn build(src: &str) -> Index {
        let toks = match lex(src) {
            Ok(t) => t,
            Err(_) => return Index::default(),
        };
        let mut ix = Index::default();
        let mut scope: Option<String> = None;
        // Depth inside a `form` or a component. A component declaration opens
        // its own block, so a single boolean would let the component's `end`
        // close the form and every component after the first would go
        // unindexed. A module-level component opens a block here too, and its
        // `end` must not be read as the end of a subroutine.
        let mut form_depth = 0usize;

        let mut i = 0;
        while i < toks.len() {
            match &toks[i].tok {
                Tok::Sub => {
                    if let Some((name, sp)) = ident_at(&toks, i + 1) {
                        ix.push(name.clone(), sp, true, SymKind::Sub, None);
                        scope = Some(name.clone());
                        i += 2;
                        // Parameters are declarations, scoped to this
                        // subroutine — they shadow module variables exactly the
                        // way a `let` does, and completion inside the body is
                        // close to useless without them.
                        if matches!(toks.get(i).map(|t| &t.tok), Some(Tok::LParen)) {
                            i += 1;
                            while let Some((pname, psp)) = ident_at(&toks, i) {
                                ix.locals_by_sub
                                    .entry(name.clone())
                                    .or_default()
                                    .push(pname.clone());
                                ix.push(pname, psp, true, SymKind::Local, Some(name.clone()));
                                // `: type` — the type name is a keyword, not a
                                // reference to anything, so it is not indexed.
                                i += 1;
                                if matches!(toks.get(i).map(|t| &t.tok), Some(Tok::Colon)) {
                                    i += 2;
                                }
                                match toks.get(i).map(|t| &t.tok) {
                                    Some(Tok::Comma) => i += 1,
                                    _ => break,
                                }
                            }
                            if matches!(toks.get(i).map(|t| &t.tok), Some(Tok::RParen)) {
                                i += 1;
                            }
                        }
                        // A return type is `: type` after the parameters; skip
                        // it for the same reason.
                        if matches!(toks.get(i).map(|t| &t.tok), Some(Tok::Colon)) {
                            i += 2;
                        }
                        continue;
                    }
                }
                Tok::End => {
                    if form_depth > 0 {
                        form_depth -= 1;
                    } else {
                        scope = None;
                    }
                }
                Tok::Form => {
                    form_depth = 1;
                    if let Some((name, sp)) = ident_at(&toks, i + 1) {
                        ix.push(name, sp, true, SymKind::Component, None);
                        i += 2;
                        continue;
                    }
                }
                // `for i = ...` declares a local for the rest of the
                // subroutine, exactly as a `let` would.
                Tok::For => {
                    if let Some((name, sp)) = ident_at(&toks, i + 1) {
                        if let Some(sc) = &scope {
                            ix.locals_by_sub
                                .entry(sc.clone())
                                .or_default()
                                .push(name.clone());
                            ix.push(name, sp, true, SymKind::Local, Some(sc.clone()));
                        }
                        i += 2;
                        continue;
                    }
                }
                Tok::Let | Tok::Var => {
                    if let Some((name, sp)) = ident_at(&toks, i + 1) {
                        // `var` at module level is a global; anything inside a
                        // subroutine is a local, and locals shadow.
                        let kind = if scope.is_some() {
                            SymKind::Local
                        } else {
                            SymKind::Global
                        };
                        if let Some(s) = &scope {
                            ix.locals_by_sub
                                .entry(s.clone())
                                .or_default()
                                .push(name.clone());
                        }
                        ix.push(name, sp, true, kind, scope.clone());
                        i += 2;
                        continue;
                    }
                }
                Tok::Ident(name) => {
                    let sp = &toks[i];
                    // `type id` declares a component and opens a block of its
                    // own: directly inside a form for a visual one, at module
                    // level for a non-visual one. `target console` is the one
                    // other place two identifiers meet out here, and it names
                    // nothing an editor can jump to.
                    let declares_component = scope.is_none()
                        && (form_depth == 1 || (form_depth == 0 && name != "target"));
                    if declares_component {
                        if let Some((id, id_sp)) = ident_at(&toks, i + 1) {
                            ix.push(name.clone(), sp, false, SymKind::ComponentType, None);
                            ix.component_types.insert(id.clone(), name.clone());
                            ix.push(id, id_sp, true, SymKind::Component, None);
                            form_depth += 1;
                            i += 2;
                            continue;
                        }
                    }
                    // A name straight after `.` is a property or event, never a
                    // variable — resolving it as one would jump to the wrong
                    // place.
                    let after_dot = i > 0 && toks[i - 1].tok == Tok::Dot;
                    // `call foo(` and `foo(` are commands.
                    let is_call = (i > 0 && toks[i - 1].tok == Tok::Call)
                        || toks.get(i + 1).map(|t| &t.tok) == Some(&Tok::LParen);

                    let kind = if after_dot {
                        SymKind::Property
                    } else if is_call {
                        SymKind::Command
                    } else {
                        SymKind::Local // refined by `resolve`
                    };
                    ix.push(name.clone(), sp, false, kind, scope.clone());
                }
                _ => {}
            }
            i += 1;
        }
        ix
    }

    fn push(
        &mut self,
        name: String,
        sp: &Spanned,
        is_definition: bool,
        kind: SymKind,
        scope: Option<String>,
    ) {
        self.occurrences.push(Occurrence {
            name,
            line: sp.line,
            col: sp.col,
            is_definition,
            kind,
            scope,
        });
    }

    /// The occurrence at a position, if the cursor is on an identifier.
    ///
    /// The cursor counts as "on" a name when it sits anywhere from its first
    /// character to just past its last: editors put the caret after the word
    /// you just typed, and refusing to answer there would feel broken.
    pub fn at(&self, line: usize, col: usize) -> Option<&Occurrence> {
        self.occurrences
            .iter()
            .find(|o| o.line == line && col >= o.col && col <= o.end_col())
    }

    /// The declaration site for the name at `from`, honouring shadowing.
    pub fn definition_of(&self, from: &Occurrence) -> Option<&Occurrence> {
        // A local in the same subroutine wins over anything module-level: that
        // is what shadowing means, and getting it backwards sends the user to
        // the wrong file position with full confidence.
        if let Some(scope) = &from.scope {
            if self
                .locals_by_sub
                .get(scope)
                .is_some_and(|v| v.contains(&from.name))
            {
                return self.occurrences.iter().find(|o| {
                    o.is_definition && o.name == from.name && o.scope.as_ref() == Some(scope)
                });
            }
        }
        self.occurrences
            .iter()
            .find(|o| o.is_definition && o.name == from.name && o.scope.is_none())
    }

    /// Every occurrence of the name at `from`, shadowing-aware.
    pub fn references_to(&self, from: &Occurrence) -> Vec<&Occurrence> {
        let local_scope = from.scope.as_ref().filter(|s| {
            self.locals_by_sub
                .get(*s)
                .is_some_and(|v| v.contains(&from.name))
        });
        self.occurrences
            .iter()
            .filter(|o| o.name == from.name && o.kind != SymKind::Property)
            .filter(|o| match local_scope {
                // A local's references are confined to its subroutine.
                Some(s) => o.scope.as_ref() == Some(s),
                // A module-level name is referenced anywhere it is *not*
                // shadowed by a local of the same name.
                None => o.scope.as_ref().is_none_or(|s| {
                    !self
                        .locals_by_sub
                        .get(s)
                        .is_some_and(|v| v.contains(&from.name))
                }),
            })
            .collect()
    }

    /// Names visible for completion at `scope`: module-level names plus that
    /// subroutine's locals.
    pub fn names_in_scope(&self, scope: Option<&str>) -> Vec<(&str, SymKind)> {
        let mut out: Vec<(&str, SymKind)> = Vec::new();
        for o in self.occurrences.iter().filter(|o| o.is_definition) {
            let visible = match (&o.scope, scope) {
                (None, _) => true,
                (Some(s), Some(cur)) => s == cur,
                (Some(_), None) => false,
            };
            if visible && !out.iter().any(|(n, _)| *n == o.name) {
                out.push((o.name.as_str(), o.kind));
            }
        }
        out
    }

    /// Which subroutine contains `line`, for scoping completion.
    pub fn scope_at_line(&self, line: usize) -> Option<&str> {
        self.occurrences
            .iter()
            .rfind(|o| o.is_definition && o.kind == SymKind::Sub && o.line <= line)
            .map(|o| o.name.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_definitions_and_uses() {
        let ix = Index::build("module m\nvar g: int = 1\nsub main\n  g = 2\nend\n");
        let def = ix
            .occurrences
            .iter()
            .find(|o| o.name == "g" && o.is_definition)
            .unwrap();
        assert_eq!((def.line, def.kind), (2, SymKind::Global));
        assert_eq!(ix.references_to(def).len(), 2, "declaration plus one use");
    }

    /// The trap that makes a naive index wrong: a local `x` in one subroutine
    /// is a different variable from a module `x`.
    #[test]
    fn locals_shadow_globals() {
        let src = "module m\nvar x: int = 1\nsub a\n  let x: int = 9\n  x = 3\nend\nsub b\n  x = 4\nend\n";
        let ix = Index::build(src);

        // The `x` on line 5 is a's local: two occurrences, both inside `a`.
        let in_a = ix.at(5, 3).unwrap();
        let refs = ix.references_to(in_a);
        assert_eq!(refs.len(), 2, "local x: its `let` and its assignment");
        assert!(refs.iter().all(|o| o.scope.as_deref() == Some("a")));

        let def = ix.definition_of(in_a).unwrap();
        assert_eq!(def.line, 4, "resolves to the local, not the global on line 2");

        // The `x` on line 8 is the global, and must not pick up a's local.
        let in_b = ix.at(8, 3).unwrap();
        assert_eq!(ix.definition_of(in_b).unwrap().line, 2);
        let grefs = ix.references_to(in_b);
        assert!(
            grefs.iter().all(|o| o.scope.as_deref() != Some("a")),
            "the global's references must exclude the shadowed subroutine"
        );
    }

    #[test]
    fn form_components_are_indexed_with_their_types() {
        let src = "module m\nform Main\n  button ok\n  end\n  label title\n  end\nend\n";
        let ix = Index::build(src);
        assert_eq!(ix.component_types.get("ok").map(String::as_str), Some("button"));
        assert_eq!(
            ix.component_types.get("title").map(String::as_str),
            Some("label"),
            "a component after the first must not be swallowed by the previous `end`"
        );
    }

    /// A non-visual component is declared at module level, and an editor has
    /// to find it there: rename and go-to-definition are the whole reason the
    /// index exists, and a timer's id is addressed from handlers exactly as a
    /// button's is.
    #[test]
    fn module_level_components_are_indexed_with_their_types() {
        let src = "module m\ntarget console\n\ntimer ticker\n  interval = 10\nend\n\n\
                   sub main\n  ticker.interval = 20\nend\n";
        let ix = Index::build(src);
        assert_eq!(
            ix.component_types.get("ticker").map(String::as_str),
            Some("timer")
        );
        // `target console` is two identifiers in the one other place they meet
        // at module level, and names nothing to jump to.
        assert!(
            !ix.component_types.contains_key("console"),
            "`target console` was read as a component declaration"
        );
        // The component's `end` closed its own block, not `main`.
        let assign = ix.occurrences.iter().find(|o| o.line == 9).unwrap();
        assert_eq!(assign.name, "ticker");
    }

    /// A name after `.` is a property. Treating it as a variable would send
    /// go-to-definition somewhere unrelated.
    #[test]
    fn property_names_are_not_variables() {
        let ix = Index::build(
            "module m\nform Main\n  button ok\n  end\nend\nsub go\n  ok.text = \"x\"\nend\n",
        );
        let text = ix.occurrences.iter().find(|o| o.name == "text").unwrap();
        assert_eq!(text.kind, SymKind::Property);
    }

    /// Without parameters in the index, completion inside a subroutine body
    /// cannot offer the very names the body is written in terms of, and
    /// go-to-definition on one has nowhere to go.
    #[test]
    fn parameters_are_locals_of_their_subroutine() {
        let src = "module m\nvar n: int = 1\nsub add(a: int, b: int): int\n  return a + b\nend\n";
        let ix = Index::build(src);

        let a = ix
            .occurrences
            .iter()
            .find(|o| o.name == "a" && o.is_definition)
            .unwrap();
        assert_eq!((a.line, a.kind), (3, SymKind::Local));
        assert_eq!(a.scope.as_deref(), Some("add"));

        // The use on line 4 resolves back to the parameter.
        let use_a = ix.at(4, 10).unwrap();
        assert_eq!(use_a.name, "a");
        assert_eq!(ix.definition_of(use_a).unwrap().line, 3);

        // Both parameters are offered inside the subroutine and nowhere else.
        let inside: Vec<&str> = ix
            .names_in_scope(Some("add"))
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert!(inside.contains(&"a") && inside.contains(&"b"), "{inside:?}");
        let outside: Vec<&str> = ix
            .names_in_scope(None)
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert!(!outside.contains(&"a"), "{outside:?}");
        // The subroutine itself stays a module-level name to complete and jump to.
        assert!(outside.contains(&"add"), "{outside:?}");
        // The types in the parameter list and the return type are keywords, not
        // names: indexing them would make go-to-definition on `int` land in a
        // parameter list.
        assert!(
            !ix.occurrences.iter().any(|o| o.name == "int" && o.line == 3),
            "the `sub` header's types must not be indexed as names"
        );
    }

    /// A parameter shadows a module variable of the same name, exactly as a
    /// `let` does.
    /// A `for` loop's variable is a local of its subroutine: completion must
    /// offer it inside, and go-to-definition on a use must land on the header.
    #[test]
    fn loop_variables_are_locals_of_their_subroutine() {
        let src = "module m\nsub main\n  for i = 1 to 3\n    call print_int(i)\n  end\nend\n";
        let ix = Index::build(src);

        let def = ix
            .occurrences
            .iter()
            .find(|o| o.name == "i" && o.is_definition)
            .expect("the loop variable is defined");
        assert_eq!((def.line, def.kind), (3, SymKind::Local));
        assert_eq!(def.scope.as_deref(), Some("main"));

        let inside: Vec<&str> = ix
            .names_in_scope(Some("main"))
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert!(inside.contains(&"i"), "{inside:?}");
        let outside: Vec<&str> = ix.names_in_scope(None).into_iter().map(|(n, _)| n).collect();
        assert!(!outside.contains(&"i"), "{outside:?}");
    }

    #[test]
    fn parameters_shadow_globals() {
        let ix = Index::build("module m\nvar n: int = 1\nsub f(n: int): int\n  return n\nend\n");
        let use_n = ix.at(4, 10).unwrap();
        assert_eq!(use_n.name, "n");
        assert_eq!(
            ix.definition_of(use_n).unwrap().line,
            3,
            "resolves to the parameter, not the module variable on line 2"
        );
    }

    /// An unparseable file still lexes, and the index must still work — this is
    /// the state the file is in while you are typing.
    #[test]
    fn works_on_a_file_that_does_not_parse() {
        let ix = Index::build("module m\nsub main\n  let x: int =\nend\n");
        assert!(
            ix.occurrences.iter().any(|o| o.name == "x" && o.is_definition),
            "half-typed code must still index"
        );
    }
}
