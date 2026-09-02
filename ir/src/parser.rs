//! Recursive-descent parser: `.oir` text -> `Module`.
//!
//! Grammar (v0):
//! ```text
//! module  := "module" IDENT NEWLINE item*
//! item    := sub
//! sub     := "sub" IDENT params? (":" type)? NEWLINE stmt* "end" NEWLINE
//! params  := "(" (IDENT ":" type ("," IDENT ":" type)*)? ")"
//! stmt    := let | call | return | for | break | continue
//! return  := "return" expr? NEWLINE
//! let     := "let" IDENT ":" type "=" expr NEWLINE
//! call    := "call" IDENT "(" (expr ("," expr)*)? ")" NEWLINE
//! record  := "record" IDENT NEWLINE (IDENT ":" type NEWLINE)+ "end" NEWLINE
//! type    := ("int" | "int64" | "double" | "text" | "bool" | "bytes" | IDENT)
//!            ("[" "]" | "{" "}")?
//! expr    := term (("+" | "-") term)*
//! term    := factor (("*" | "/" | "%") factor)*
//! factor  := "-"? postfix
//! postfix := primary ("[" expr "]" | "." IDENT)*
//! primary := INT | FLOAT | STRING | list | dict | new | call | IDENT
//!          | "(" expr ")"
//! list    := "[" (expr ("," expr)*)? "]"
//! dict    := "{" (expr ":" expr ("," expr ":" expr)*)? "}"
//! new     := IDENT "(" IDENT ":" expr ("," IDENT ":" expr)* ")"
//! call    := IDENT "(" (expr ("," expr)*)? ")"
//! ```

use crate::lexer::{lex, Spanned, Tok};
use crate::{
    intern, BinOp, CmpOp, Component, Elem, Expr, Form, GlobalVar, Ident, Item, LogicalOp,
    Module, RecordDef, Span, Stmt, StmtKind, Sub, Target, Ty,
};

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub line: usize,
    pub msg: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "parse error (line {}): {}", self.line, self.msg)
    }
}
impl std::error::Error for ParseError {}

pub fn parse(src: &str) -> Result<Module, ParseError> {
    let toks = lex(src).map_err(|e| ParseError {
        line: e.line,
        msg: e.msg,
    })?;
    Parser { toks, pos: 0 }.module()
}

struct Parser {
    toks: Vec<Spanned>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> &Tok {
        &self.toks[self.pos].tok
    }
    fn line(&self) -> usize {
        self.toks[self.pos].line
    }
    /// The span of the token at `pos`.
    fn span_at(&self, pos: usize) -> Span {
        let t = &self.toks[pos.min(self.toks.len() - 1)];
        Span::new(t.line, t.col, t.end_col)
    }
    /// A statement whose header began at token `start`: its span runs to the
    /// end of that line, and its identifiers are the ones on it. The body of
    /// an `if` or a loop is not part of the header — each statement in it
    /// answers for itself.
    fn finish(&self, kind: StmtKind, start: usize) -> Stmt {
        let first = self.span_at(start);
        let mut span = first;
        let mut idents = Vec::new();
        for t in &self.toks[start..] {
            if matches!(t.tok, Tok::Newline | Tok::Eof) {
                break;
            }
            span.end_col = t.end_col;
            if let Tok::Ident(name) = &t.tok {
                idents.push(Ident {
                    name: name.clone(),
                    span: Span::new(t.line, t.col, t.end_col),
                });
            }
        }
        Stmt {
            kind,
            line: first.line,
            span,
            idents,
        }
    }
    /// The token `n` places past the cursor, saturating at the final `Eof`.
    fn peek_at(&self, n: usize) -> &Tok {
        &self.toks[(self.pos + n).min(self.toks.len() - 1)].tok
    }
    fn bump(&mut self) -> Tok {
        let t = self.toks[self.pos].tok.clone();
        if self.pos + 1 < self.toks.len() {
            self.pos += 1;
        }
        t
    }
    fn err<T>(&self, msg: impl Into<String>) -> Result<T, ParseError> {
        Err(ParseError {
            line: self.line(),
            msg: msg.into(),
        })
    }
    fn expect(&mut self, want: &Tok, what: &str) -> Result<(), ParseError> {
        if self.peek() == want {
            self.bump();
            Ok(())
        } else {
            self.err(format!("expected {what}, found {:?}", self.peek()))
        }
    }
    /// Consume zero or more newline separators.
    fn skip_newlines(&mut self) {
        while matches!(self.peek(), Tok::Newline) {
            self.bump();
        }
    }
    fn ident(&mut self, what: &str) -> Result<String, ParseError> {
        match self.bump() {
            Tok::Ident(s) => Ok(s),
            other => self.err(format!("expected {what}, found {other:?}")),
        }
    }

    fn module(&mut self) -> Result<Module, ParseError> {
        self.skip_newlines();
        self.expect(&Tok::Module, "`module`")?;
        let name = self.ident("module name")?;
        self.expect(&Tok::Newline, "newline after module name")?;

        // Optional `target <kind>` and `use <lib>` declarations precede the
        // items.
        //
        // `target` is a *soft* keyword — matched as an identifier in this one
        // position rather than reserved in the lexer. Reserving it would steal
        // `target` as a variable and property name everywhere, which is a poor
        // trade for one declaration.
        let mut target = None;
        let mut uses = Vec::new();
        loop {
            self.skip_newlines();
            if matches!(self.peek(), Tok::Ident(w) if w == "target") {
                let line = self.line();
                self.bump();
                let kind = self.ident("target kind")?;
                match Target::parse(&kind) {
                    Some(t) => target = Some(t),
                    None => {
                        return Err(ParseError {
                            line,
                            msg: format!(
                                "unknown target `{kind}` — expected console, gui, sharedlib \
                                 or staticlib"
                            ),
                        })
                    }
                }
                self.expect(&Tok::Newline, "newline after `target`")?;
                continue;
            }
            if matches!(self.peek(), Tok::Use) {
                self.bump();
                let lib = self.ident("library name")?;
                self.expect(&Tok::Newline, "newline after `use`")?;
                uses.push(lib);
            } else {
                break;
            }
        }

        let mut items = Vec::new();
        loop {
            self.skip_newlines();
            match self.peek() {
                Tok::Sub => items.push(Item::Sub(self.sub()?)),
                Tok::Form => items.push(Item::Form(self.form()?)),
                Tok::Var => items.push(Item::Var(self.global_var()?)),
                // `record` is a SOFT keyword, matched in this one position:
                // `record point` and `timer ticker` are the same two tokens, so
                // the word itself has to decide, and reserving it would steal an
                // ordinary noun from every variable in the language.
                Tok::Ident(w) if w == "record" => {
                    items.push(Item::UserType(self.record_def()?))
                }
                // `type id` at module level is a NON-VISUAL component — a timer,
                // a server. It reads exactly as it does inside a form, because
                // the only thing it lacks is a rectangle to be drawn in; the
                // validator is what rejects a button out here.
                Tok::Ident(type_name) => {
                    let type_name = type_name.clone();
                    self.bump();
                    items.push(Item::Component(self.component(type_name)?));
                }
                Tok::Eof => break,
                other => {
                    return self.err(format!(
                        "expected `sub`, `form`, `var`, a component, or end of file, \
                         found {other:?}"
                    ))
                }
            }
        }
        Ok(Module { name, target, uses, items })
    }

    /// ```text
    /// sub := "sub" IDENT params? (":" type)? NEWLINE stmt* "end" NEWLINE
    /// ```
    ///
    /// Both the parameter list and the return type are optional, so `sub main`
    /// — an entry point or an event handler — still means exactly what it did
    /// before parameters existed.
    fn sub(&mut self) -> Result<Sub, ParseError> {
        let sub_line = self.line();
        self.expect(&Tok::Sub, "`sub`")?;
        let name_span = self.span_at(self.pos);
        let name = self.ident("subroutine name")?;

        // `sub name(a: int, b: text)`; `sub name()` is the same as `sub name`.
        let mut params: Vec<(String, Ty)> = Vec::new();
        if matches!(self.peek(), Tok::LParen) {
            self.bump();
            if !matches!(self.peek(), Tok::RParen) {
                loop {
                    let pname = self.ident("parameter name")?;
                    self.expect(&Tok::Colon, "`:` after parameter name")?;
                    let pty = self.type_keyword()?;
                    if params.iter().any(|(n, _)| *n == pname) {
                        return self.err(format!(
                            "subroutine `{name}` declares parameter `{pname}` twice"
                        ));
                    }
                    params.push((pname, pty));
                    match self.peek() {
                        Tok::Comma => {
                            self.bump();
                        }
                        Tok::RParen => break,
                        other => {
                            return self
                                .err(format!("expected `,` or `)` in parameters, found {other:?}"))
                        }
                    }
                }
            }
            self.expect(&Tok::RParen, "`)` after parameters")?;
        }

        // `: type` — the same `name: type` shape a `let` uses, so a return type
        // reads like the declaration it is.
        let ret = if matches!(self.peek(), Tok::Colon) {
            self.bump();
            Some(self.type_keyword()?)
        } else {
            None
        };
        self.expect(&Tok::Newline, "newline after subroutine name")?;

        let mut body = Vec::new();
        loop {
            self.skip_newlines();
            match self.peek() {
                Tok::End => {
                    self.bump();
                    break;
                }
                Tok::Let => body.push(self.stmt_let(false)?),
                Tok::Var => body.push(self.stmt_let(true)?),
                Tok::Call => body.push(self.stmt_call()?),
                Tok::If => body.push(self.stmt_if()?),
                Tok::While => body.push(self.stmt_while()?),
                Tok::For => body.push(self.stmt_for()?),
                Tok::Break => body.push(self.stmt_jump(true)?),
                Tok::Continue => body.push(self.stmt_jump(false)?),
                Tok::Return => body.push(self.stmt_return()?),
                // Either `name = expr` or `component.property = expr`; one
                // token of lookahead past the identifier tells them apart.
                Tok::Ident(_) => body.push(self.stmt_ident()?),
                Tok::Eof => return self.err("unexpected end of file inside `sub` (missing `end`)"),
                other => return self.err(format!("expected statement or `end`, found {other:?}")),
            }
        }
        // Trailing newline after `end` is optional (EOF is fine).
        if matches!(self.peek(), Tok::Newline) {
            self.bump();
        }
        Ok(Sub {
            name,
            params,
            ret,
            line: sub_line,
            name_span,
            body,
        })
    }

    /// ```text
    /// form      := "form" IDENT NEWLINE member* "end" NEWLINE
    /// member    := property | binding | component
    /// property  := IDENT "=" expr NEWLINE
    /// binding   := "on" IDENT ":" IDENT NEWLINE
    /// component := IDENT IDENT NEWLINE (property | binding)* "end" NEWLINE
    /// ```
    fn form(&mut self) -> Result<Form, ParseError> {
        let first_line = self.line();
        self.expect(&Tok::Form, "`form`")?;
        let name = self.ident("form name")?;
        self.expect(&Tok::Newline, "newline after form name")?;

        let mut form = Form {
            name,
            line_span: (first_line, first_line),
            properties: Vec::new(),
            handlers: Vec::new(),
            property_spans: Vec::new(),
            handler_spans: Vec::new(),
            children: Vec::new(),
        };
        loop {
            self.skip_newlines();
            match self.peek().clone() {
                Tok::End => {
                    form.line_span.1 = self.line();
                    self.bump();
                    break;
                }
                Tok::On => {
                    let (event, handler, span) = self.binding()?;
                    form.handlers.push((event, handler));
                    form.handler_spans.push(span);
                }
                Tok::Ident(first) => {
                    let span = self.span_at(self.pos);
                    self.bump();
                    if matches!(self.peek(), Tok::Eq) {
                        self.bump();
                        let value = self.expr()?;
                        self.expect(&Tok::Newline, "newline after property")?;
                        form.properties.push((first, value));
                        form.property_spans.push(span);
                    } else {
                        form.children.push(self.component(first)?);
                    }
                }
                Tok::Eof => {
                    return self.err("unexpected end of file inside `form` (missing `end`)")
                }
                other => {
                    return self.err(format!(
                        "expected a property, component, `on`, or `end`, found {other:?}"
                    ))
                }
            }
        }
        if matches!(self.peek(), Tok::Newline) {
            self.bump();
        }
        Ok(form)
    }

    /// ```text
    /// record := "record" IDENT NEWLINE (IDENT ":" type NEWLINE)+ "end" NEWLINE
    /// ```
    fn record_def(&mut self) -> Result<RecordDef, ParseError> {
        let line = self.line();
        self.bump(); // `record`
        let name = self.ident("record name")?;
        self.expect(&Tok::Newline, "newline after record name")?;
        let mut fields: Vec<(String, Ty)> = Vec::new();
        loop {
            self.skip_newlines();
            match self.peek().clone() {
                Tok::End => {
                    self.bump();
                    break;
                }
                Tok::Ident(field) => {
                    self.bump();
                    self.expect(&Tok::Colon, "`:` after a field name")?;
                    let ty = self.type_keyword()?;
                    self.expect(&Tok::Newline, "newline after a field")?;
                    if fields.iter().any(|(n, _)| *n == field) {
                        return self
                            .err(format!("record `{name}` declares field `{field}` twice"));
                    }
                    fields.push((field, ty));
                }
                Tok::Eof => {
                    return self.err("unexpected end of file inside `record` (missing `end`)")
                }
                other => {
                    return self.err(format!("expected a field or `end`, found {other:?}"))
                }
            }
        }
        if matches!(self.peek(), Tok::Newline) {
            self.bump();
        }
        // Every field is named at construction, so a record with none could
        // only be written `point()` — which is a call, and would need a rule of
        // its own for no gain.
        if fields.is_empty() {
            return Err(ParseError {
                line,
                msg: format!(
                    "record `{name}` has no fields — a record names a group of values"
                ),
            });
        }
        Ok(RecordDef { name, fields, line })
    }

    /// A component instance; `type_name` has already been consumed.
    fn component(&mut self, type_name: String) -> Result<Component, ParseError> {
        let id = self.ident("component id")?;
        self.expect(&Tok::Newline, "newline after component id")?;
        let mut c = Component {
            type_name,
            id,
            properties: Vec::new(),
            handlers: Vec::new(),
            property_spans: Vec::new(),
            handler_spans: Vec::new(),
        };
        loop {
            self.skip_newlines();
            match self.peek().clone() {
                Tok::End => {
                    self.bump();
                    break;
                }
                Tok::On => {
                    let (event, handler, span) = self.binding()?;
                    c.handlers.push((event, handler));
                    c.handler_spans.push(span);
                }
                Tok::Ident(name) => {
                    let span = self.span_at(self.pos);
                    self.bump();
                    self.expect(&Tok::Eq, "`=` after property name")?;
                    let value = self.expr()?;
                    self.expect(&Tok::Newline, "newline after property")?;
                    c.properties.push((name, value));
                    c.property_spans.push(span);
                }
                Tok::Eof => {
                    return self.err("unexpected end of file inside component (missing `end`)")
                }
                other => {
                    return self.err(format!(
                        "expected a property, `on`, or `end`, found {other:?}"
                    ))
                }
            }
        }
        if matches!(self.peek(), Tok::Newline) {
            self.bump();
        }
        Ok(c)
    }

    /// `on <event>: <subroutine>`, and the span of the event name — the part
    /// of the line a diagnostic about the binding points at.
    fn binding(&mut self) -> Result<(String, String, Span), ParseError> {
        self.expect(&Tok::On, "`on`")?;
        let span = self.span_at(self.pos);
        let event = self.ident("event name")?;
        self.expect(&Tok::Colon, "`:` after event name")?;
        let handler = self.ident("handler subroutine name")?;
        self.expect(&Tok::Newline, "newline after event binding")?;
        Ok((event, handler, span))
    }

    /// A module-level `var NAME: TY = EXPR`.
    fn global_var(&mut self) -> Result<GlobalVar, ParseError> {
        self.expect(&Tok::Var, "`var`")?;
        let name = self.ident("variable name")?;
        self.expect(&Tok::Colon, "`:` after variable name")?;
        let ty = self.type_keyword()?;
        self.expect(&Tok::Eq, "`=`")?;
        let value = self.expr()?;
        self.expect(&Tok::Newline, "newline after `var`")?;
        Ok(GlobalVar { name, ty, value })
    }

    /// ```text
    /// type := IDENT ("[" "]")?
    /// ```
    ///
    /// The `[]` suffix follows the element type rather than wrapping it, so
    /// `int[]` reads as "ints" in the same left-to-right order the rest of a
    /// declaration does.
    fn type_keyword(&mut self) -> Result<Ty, ParseError> {
        let base = match self.bump() {
            Tok::Ident(w) => match Ty::from_keyword(&w) {
                Some(t) => t,
                // Any other word is a record type, by name. Whether a record
                // with that name exists is the validator's question: a record
                // may be declared after the subroutine that uses it, and a
                // one-pass parser has not read it yet.
                None => Ty::Record(intern(&w)),
            },
            other => return self.err(format!("expected a type, found {other:?}")),
        };
        // `[]` for a list, `{}` for a dictionary keyed by text. The suffix
        // follows the element type in both, so `int[]` and `int{}` read as
        // "ints" and "ints by name" in the order the rest of a declaration does.
        let (close, closing, wrap, what): (Tok, &str, fn(Elem) -> Ty, &str) = match self.peek() {
            Tok::LBracket => (Tok::RBracket, "`]` after `[` in an array type", Ty::Array, "a list"),
            Tok::LBrace => (Tok::RBrace, "`}` after `{` in a dictionary type", Ty::Dict, "a dictionary"),
            _ => return Ok(base),
        };
        self.bump();
        self.expect(&close, closing)?;
        // A second suffix is caught here rather than left to the checker so the
        // message names the real limitation instead of the token that follows.
        if matches!(self.peek(), Tok::LBracket | Tok::LBrace) {
            return self.err(format!(
                "{what} cannot hold arrays or dictionaries — `int[][]` is not a type"
            ));
        }
        match Elem::from_ty(base) {
            Some(e) => Ok(wrap(e)),
            None => self.err(format!("{what} cannot hold {} values", base.as_str())),
        }
    }

    /// A statement starting with an identifier: assignment or property-set.
    fn stmt_ident(&mut self) -> Result<Stmt, ParseError> {
        let start = self.pos;
        let name = self.ident("variable or component name")?;
        match self.peek() {
            Tok::LBracket => {
                self.bump();
                let index = self.expr()?;
                self.expect(&Tok::RBracket, "`]` after the index")?;
                self.expect(&Tok::Eq, "`=` in an element assignment")?;
                let value = self.expr()?;
                self.expect(&Tok::Newline, "newline after assignment")?;
                Ok(self.finish(
                    StmtKind::SetIndex { name, index, value },
                    start,
                ))
            }
            Tok::Eq => {
                self.bump();
                let value = self.expr()?;
                self.expect(&Tok::Newline, "newline after assignment")?;
                Ok(self.finish(StmtKind::Assign { name, value }, start))
            }
            Tok::Dot => {
                self.bump();
                let property = self.ident("property name")?;
                self.expect(&Tok::Eq, "`=` in property assignment")?;
                let value = self.expr()?;
                self.expect(&Tok::Newline, "newline after property assignment")?;
                Ok(self.finish(StmtKind::SetProperty {
                    component: name,
                    property,
                    value,
                }, start))
            }
            other => self.err(format!(
                "expected `=` (assignment), `[` (element) or `.` (property) after `{name}`, \
                 found {other:?}"
            )),
        }
    }

    fn stmt_let(&mut self, mutable: bool) -> Result<Stmt, ParseError> {
        let start = self.pos;
        self.expect(
            if mutable { &Tok::Var } else { &Tok::Let },
            "`let` or `var`",
        )?;
        let name = self.ident("variable name")?;
        self.expect(&Tok::Colon, "`:` after variable name")?;
        let ty = self.type_keyword()?;
        self.expect(&Tok::Eq, "`=`")?;
        let value = self.expr()?;
        self.expect(&Tok::Newline, "newline after `let`")?;
        Ok(self.finish(StmtKind::Let {
            name,
            ty,
            value,
            mutable,
        }, start))
    }

    /// Statements until one of `terminators`, which is not consumed.
    fn block(&mut self, terminators: &[Tok]) -> Result<Vec<Stmt>, ParseError> {
        let mut body = Vec::new();
        loop {
            self.skip_newlines();
            if terminators.contains(self.peek()) {
                return Ok(body);
            }
            match self.peek().clone() {
                Tok::Let => body.push(self.stmt_let(false)?),
                Tok::Var => body.push(self.stmt_let(true)?),
                Tok::Call => body.push(self.stmt_call()?),
                Tok::If => body.push(self.stmt_if()?),
                Tok::While => body.push(self.stmt_while()?),
                Tok::For => body.push(self.stmt_for()?),
                Tok::Break => body.push(self.stmt_jump(true)?),
                Tok::Continue => body.push(self.stmt_jump(false)?),
                Tok::Return => body.push(self.stmt_return()?),
                Tok::Ident(_) => body.push(self.stmt_ident()?),
                Tok::Eof => return self.err("unexpected end of file (missing `end`)"),
                other => {
                    return self.err(format!("expected a statement or `end`, found {other:?}"))
                }
            }
        }
    }

    /// `if COND NEWLINE ... (else if COND NEWLINE ...)* (else NEWLINE ...)? end`
    fn stmt_if(&mut self) -> Result<Stmt, ParseError> {
        let start = self.pos;
        self.expect(&Tok::If, "`if`")?;
        let mut arms = Vec::new();
        let mut otherwise = None;
        loop {
            let cond = self.expr()?;
            self.expect(&Tok::Newline, "newline after the condition")?;
            let body = self.block(&[Tok::Else, Tok::End])?;
            arms.push((cond, body));

            match self.peek() {
                Tok::Else => {
                    self.bump();
                    if matches!(self.peek(), Tok::If) {
                        self.bump();
                        continue; // `else if`
                    }
                    self.expect(&Tok::Newline, "newline after `else`")?;
                    otherwise = Some(self.block(&[Tok::End])?);
                    self.expect(&Tok::End, "`end`")?;
                    break;
                }
                Tok::End => {
                    self.bump();
                    break;
                }
                other => return self.err(format!("expected `else` or `end`, found {other:?}")),
            }
        }
        if matches!(self.peek(), Tok::Newline) {
            self.bump();
        }
        Ok(self.finish(StmtKind::If { arms, otherwise }, start))
    }

    /// `while COND NEWLINE ... end`
    fn stmt_while(&mut self) -> Result<Stmt, ParseError> {
        let start = self.pos;
        self.expect(&Tok::While, "`while`")?;
        let cond = self.expr()?;
        self.expect(&Tok::Newline, "newline after the condition")?;
        let body = self.block(&[Tok::End])?;
        self.expect(&Tok::End, "`end`")?;
        if matches!(self.peek(), Tok::Newline) {
            self.bump();
        }
        Ok(self.finish(StmtKind::While { cond, body }, start))
    }

    /// ```text
    /// for := "for" IDENT "=" expr "to" expr ("step" INT)? NEWLINE stmt* "end"
    /// ```
    ///
    /// `to` and `step` are **soft** keywords, matched as identifiers in these
    /// two positions only — reserving them would steal two ordinary words from
    /// every variable and property name in the language.
    fn stmt_for(&mut self) -> Result<Stmt, ParseError> {
        let head = self.pos;
        self.expect(&Tok::For, "`for`")?;
        let var = self.ident("loop variable name")?;
        self.expect(&Tok::Eq, "`=` after the loop variable")?;
        let start = self.expr()?;
        if !matches!(self.peek(), Tok::Ident(w) if w == "to") {
            return self.err(format!(
                "expected `to` after the start value, found {:?}",
                self.peek()
            ));
        }
        self.bump();
        let limit = self.expr()?;

        // `step K`. The step is a literal so that the loop's direction — and
        // therefore whether it counts while `i <= limit` or `i >= limit` — is
        // known without a run-time test.
        let mut step = 1i64;
        if matches!(self.peek(), Tok::Ident(w) if w == "step") {
            self.bump();
            let line = self.line();
            match self.expr()? {
                Expr::IntLit(0) => {
                    return Err(ParseError {
                        line,
                        msg: "`step 0` never advances the loop variable".into(),
                    })
                }
                Expr::IntLit(v) => step = v,
                _ => {
                    return Err(ParseError {
                        line,
                        msg: "`step` needs a whole-number literal, such as `step 2` or `step -1`"
                            .into(),
                    })
                }
            }
        }
        self.expect(&Tok::Newline, "newline after the loop header")?;
        let body = self.block(&[Tok::End])?;
        self.expect(&Tok::End, "`end`")?;
        if matches!(self.peek(), Tok::Newline) {
            self.bump();
        }
        Ok(self.finish(
            StmtKind::For {
                var,
                start,
                limit,
                step,
                body,
            },
            head,
        ))
    }

    /// `break` / `continue`.
    fn stmt_jump(&mut self, is_break: bool) -> Result<Stmt, ParseError> {
        let start = self.pos;
        self.bump();
        if matches!(self.peek(), Tok::Newline) {
            self.bump();
        }
        Ok(self.finish(
            if is_break {
                StmtKind::Break
            } else {
                StmtKind::Continue
            },
            start,
        ))
    }

    /// `return` or `return EXPR`. A bare `return` is the one that leaves a sub
    /// with no return type early; anything else on the line is the value.
    fn stmt_return(&mut self) -> Result<Stmt, ParseError> {
        let start = self.pos;
        self.expect(&Tok::Return, "`return`")?;
        let value = if matches!(self.peek(), Tok::Newline | Tok::Eof) {
            None
        } else {
            Some(self.expr()?)
        };
        if matches!(self.peek(), Tok::Newline) {
            self.bump();
        }
        Ok(self.finish(StmtKind::Return { value }, start))
    }

    fn stmt_call(&mut self) -> Result<Stmt, ParseError> {
        let start = self.pos;
        self.expect(&Tok::Call, "`call`")?;
        let cmd = self.ident("command name")?;
        self.expect(&Tok::LParen, "`(`")?;
        let args = self.arg_list()?;
        self.expect(&Tok::Newline, "newline after call")?;
        Ok(self.finish(StmtKind::Call { cmd, args }, start))
    }

    /// Parse a comma-separated argument list, assuming the opening `(` has been
    /// consumed; consumes the closing `)`.
    fn arg_list(&mut self) -> Result<Vec<Expr>, ParseError> {
        let mut args = Vec::new();
        if !matches!(self.peek(), Tok::RParen) {
            loop {
                args.push(self.expr()?);
                match self.peek() {
                    Tok::Comma => {
                        self.bump();
                    }
                    Tok::RParen => break,
                    other => return self.err(format!("expected `,` or `)`, found {other:?}")),
                }
            }
        }
        self.expect(&Tok::RParen, "`)`")?;
        Ok(args)
    }

    fn expr(&mut self) -> Result<Expr, ParseError> {
        self.or_expr()
    }

    fn or_expr(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.and_expr()?;
        while matches!(self.peek(), Tok::Or) {
            self.bump();
            let rhs = self.and_expr()?;
            lhs = Expr::Logical(LogicalOp::Or, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn and_expr(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.not_expr()?;
        while matches!(self.peek(), Tok::And) {
            self.bump();
            let rhs = self.not_expr()?;
            lhs = Expr::Logical(LogicalOp::And, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn not_expr(&mut self) -> Result<Expr, ParseError> {
        if matches!(self.peek(), Tok::Not) {
            self.bump();
            return Ok(Expr::Not(Box::new(self.not_expr()?)));
        }
        self.cmp_expr()
    }

    /// Comparisons are **non-associative**: `a < b < c` is a compile error
    /// rather than `(a < b) < c`, which would silently compare a bool to a
    /// number and give a confidently wrong answer.
    fn cmp_expr(&mut self) -> Result<Expr, ParseError> {
        let lhs = self.sum()?;
        let op = match self.peek() {
            Tok::Eq => CmpOp::Eq,
            Tok::Ne => CmpOp::Ne,
            Tok::Lt => CmpOp::Lt,
            Tok::Le => CmpOp::Le,
            Tok::Gt => CmpOp::Gt,
            Tok::Ge => CmpOp::Ge,
            _ => return Ok(lhs),
        };
        self.bump();
        let rhs = self.sum()?;
        if matches!(
            self.peek(),
            Tok::Eq | Tok::Ne | Tok::Lt | Tok::Le | Tok::Gt | Tok::Ge
        ) {
            return self.err(
                "comparisons cannot be chained; write `a < b and b < c` instead of `a < b < c`",
            );
        }
        Ok(Expr::Cmp(op, Box::new(lhs), Box::new(rhs)))
    }

    fn sum(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.term()?;
        loop {
            let op = match self.peek() {
                Tok::Plus => BinOp::Add,
                Tok::Minus => BinOp::Sub,
                _ => break,
            };
            self.bump();
            let rhs = self.term()?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn term(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.factor()?;
        loop {
            let op = match self.peek() {
                Tok::Star => BinOp::Mul,
                Tok::Slash => BinOp::Div,
                Tok::Percent => BinOp::Rem,
                _ => break,
            };
            self.bump();
            let rhs = self.factor()?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    /// A factor, optionally negated. Unary `-` binds tighter than `*`, so
    /// `-a * b` is `(-a) * b`, and a negated literal is folded into the literal
    /// itself — see `Expr::Neg`.
    fn factor(&mut self) -> Result<Expr, ParseError> {
        if matches!(self.peek(), Tok::Minus) {
            self.bump();
            return Ok(match self.factor()? {
                Expr::IntLit(v) => match v.checked_neg() {
                    Some(n) => Expr::IntLit(n),
                    None => return self.err("integer literal out of range"),
                },
                Expr::DoubleLit(v) => Expr::DoubleLit(-v),
                other => Expr::Neg(Box::new(other)),
            });
        }
        let primary = self.primary()?;
        self.postfix(primary)
    }

    /// `[` … `]` and `.field` after a value, repeatedly — both bind tighter
    /// than any operator, so `xs[0] + 1` adds to the element and not to the
    /// array, and `people[1].age * 2` doubles the age.
    ///
    /// The first `.` of a chain never arrives here: `p.x` is read in `primary`,
    /// where it is indistinguishable from a component property read and is left
    /// as one for the checker to resolve.
    fn postfix(&mut self, mut e: Expr) -> Result<Expr, ParseError> {
        loop {
            match self.peek() {
                Tok::LBracket => {
                    self.bump();
                    let index = self.expr()?;
                    self.expect(&Tok::RBracket, "`]` after the index")?;
                    e = Expr::Index {
                        base: Box::new(e),
                        index: Box::new(index),
                    };
                }
                Tok::Dot => {
                    self.bump();
                    let name = self.ident("field name")?;
                    e = Expr::Field {
                        base: Box::new(e),
                        name,
                    };
                }
                _ => return Ok(e),
            }
        }
    }

    fn primary(&mut self) -> Result<Expr, ParseError> {
        // `[a, b, c]` — the elements are a plain argument list, so a literal
        // may hold any expression, not only constants.
        if matches!(self.peek(), Tok::LBracket) {
            self.bump();
            let mut items = Vec::new();
            if !matches!(self.peek(), Tok::RBracket) {
                loop {
                    items.push(self.expr()?);
                    match self.peek() {
                        Tok::Comma => {
                            self.bump();
                        }
                        Tok::RBracket => break,
                        other => {
                            return self
                                .err(format!("expected `,` or `]` in a list, found {other:?}"))
                        }
                    }
                }
            }
            self.expect(&Tok::RBracket, "`]` closing the list")?;
            return Ok(Expr::ArrayLit(items));
        }
        // `{"a": 1, "b": 2}` — a dictionary. The key is an expression, not a
        // bare word, because a key is data: the common case is one a program
        // computed, not one it typed out.
        if matches!(self.peek(), Tok::LBrace) {
            self.bump();
            let mut pairs = Vec::new();
            self.skip_newlines();
            if !matches!(self.peek(), Tok::RBrace) {
                loop {
                    self.skip_newlines();
                    let key = self.expr()?;
                    self.expect(&Tok::Colon, "`:` between a key and its value")?;
                    let value = self.expr()?;
                    pairs.push((key, value));
                    self.skip_newlines();
                    match self.peek() {
                        Tok::Comma => {
                            self.bump();
                        }
                        Tok::RBrace => break,
                        other => {
                            return self.err(format!(
                                "expected `,` or `}}` in a dictionary, found {other:?}"
                            ))
                        }
                    }
                }
            }
            self.expect(&Tok::RBrace, "`}` closing the dictionary")?;
            return Ok(Expr::DictLit(pairs));
        }
        match self.bump() {
            Tok::True => Ok(Expr::BoolLit(true)),
            Tok::False => Ok(Expr::BoolLit(false)),
            Tok::Int(v) => Ok(Expr::IntLit(v)),
            Tok::Float(v) => Ok(Expr::DoubleLit(v)),
            Tok::Str(s) => Ok(Expr::TextLit(s)),
            Tok::Ident(name) => {
                // `point(x: 1, y: 2)` — a record. A named first argument is
                // what separates it from a call, and two tokens of lookahead
                // are enough to see one.
                if matches!(self.peek(), Tok::LParen)
                    && matches!(self.peek_at(1), Tok::Ident(_))
                    && matches!(self.peek_at(2), Tok::Colon)
                {
                    self.bump();
                    let mut fields: Vec<(String, Expr)> = Vec::new();
                    loop {
                        let field = self.ident("field name")?;
                        self.expect(&Tok::Colon, "`:` after a field name")?;
                        let value = self.expr()?;
                        fields.push((field, value));
                        match self.peek() {
                            Tok::Comma => {
                                self.bump();
                            }
                            Tok::RParen => break,
                            other => {
                                return self.err(format!(
                                    "expected `,` or `)` in `{name}`, found {other:?}"
                                ))
                            }
                        }
                    }
                    self.expect(&Tok::RParen, "`)` after the fields")?;
                    return Ok(Expr::RecordLit { name, fields });
                }
                if matches!(self.peek(), Tok::LParen) {
                    // Call-expression: NAME(args...)
                    self.bump();
                    let args = self.arg_list()?;
                    Ok(Expr::Call { cmd: name, args })
                } else if matches!(self.peek(), Tok::Dot) {
                    // Property read: component.property
                    self.bump();
                    let property = self.ident("property name")?;
                    Ok(Expr::GetProperty {
                        component: name,
                        property,
                    })
                } else {
                    Ok(Expr::Var(name))
                }
            }
            Tok::LParen => {
                let e = self.expr()?;
                self.expect(&Tok::RParen, "`)`")?;
                Ok(e)
            }
            other => self.err(format!("expected expression, found {other:?}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hello() {
        let src = "module m\nsub main\n  let x: int = 6 * 7\n  call print_int(x)\nend\n";
        let m = parse(src).unwrap();
        assert_eq!(m.name, "m");
        assert_eq!(m.subs().count(), 1);
    }

    #[test]
    fn parses_form_with_component_and_binding() {
        let src = "module m\nuse ui\nform win\n  title = \"Hi\"\n  width = 320\n  button ok\n    text = \"Go\"\n    on click: handler\n  end\nend\nsub handler\n  call print_text(\"hi\")\nend\n";
        let m = parse(src).unwrap();
        assert_eq!(m.uses, vec!["ui"]);
        let f = m.forms().next().expect("a form");
        assert_eq!(f.name, "win");
        assert_eq!(f.properties.len(), 2);
        assert_eq!(f.children.len(), 1);
        let c = &f.children[0];
        assert_eq!((c.type_name.as_str(), c.id.as_str()), ("button", "ok"));
        assert_eq!(
            c.handlers,
            vec![("click".to_string(), "handler".to_string())]
        );
        assert!(m.is_gui());
    }

    #[test]
    fn parses_parameters_and_a_return_type() {
        let m = parse("module m\nsub add(a: int, b: text): double\n  return 1.0\nend\n").unwrap();
        let s = m.subs().next().unwrap();
        assert_eq!(
            s.params,
            vec![("a".into(), Ty::Int), ("b".into(), Ty::Text)]
        );
        assert_eq!(s.ret, Some(Ty::Double));
        assert_eq!(s.line, 2);
    }

    /// The old shape must keep parsing untouched: an entry point and an event
    /// handler are subs with no parameters and no return type.
    #[test]
    fn a_bare_sub_is_still_a_sub() {
        for src in [
            "module m\nsub main\n  call print_int(1)\nend\n",
            "module m\nsub main()\n  call print_int(1)\nend\n",
        ] {
            let s = parse(src).unwrap().subs().next().unwrap().clone();
            assert!(s.is_plain(), "{src}");
            assert_eq!(s.signature().params.len(), 0);
        }
    }

    #[test]
    fn parses_return_with_and_without_a_value() {
        let m = parse("module m\nsub main\n  if true\n    return\n  end\n  return\nend\n")
            .unwrap();
        let s = m.subs().next().unwrap();
        assert!(matches!(
            s.body.last().unwrap().kind,
            StmtKind::Return { value: None }
        ));
        // The `return` nested inside the `if` must be reached too — the body
        // loop and the block loop are separate code paths.
        let StmtKind::If { arms, .. } = &s.body[0].kind else {
            panic!("expected an if")
        };
        assert!(matches!(
            arms[0].1[0].kind,
            StmtKind::Return { value: None }
        ));
    }

    #[test]
    fn rejects_a_duplicate_parameter() {
        assert!(parse("module m\nsub f(a: int, a: int)\nend\n").is_err());
    }

    #[test]
    fn folds_a_negative_literal_into_the_literal() {
        // `-5` must BE the literal -5, not a negation of 5: only then does it
        // type `int` at the extremes and stay usable as a form property.
        let m = parse("module m\nsub main\n  let x: int = -2147483648\nend\n").unwrap();
        let s = m.subs().next().unwrap();
        assert!(matches!(
            s.body[0].kind,
            StmtKind::Let {
                value: Expr::IntLit(-2147483648),
                ..
            }
        ));
    }

    #[test]
    fn negates_a_non_literal() {
        let m = parse("module m\nsub main\n  let x: int = 1\n  let y: int = -x\nend\n").unwrap();
        let s = m.subs().next().unwrap();
        assert!(matches!(
            s.body[1].kind,
            StmtKind::Let {
                value: Expr::Neg(_),
                ..
            }
        ));
    }

    /// Unary minus binds tighter than `*`: `-2 * 3` is `(-2) * 3`, which is
    /// -6 either way, but `-a * b` and `-(a * b)` differ in the general case
    /// only by association — what must not happen is the `-` swallowing the
    /// multiplication's right operand.
    #[test]
    fn unary_minus_binds_tighter_than_multiplication() {
        let m = parse("module m\nsub main\n  let x: int = -2 * 3\nend\n").unwrap();
        let s = m.subs().next().unwrap();
        let StmtKind::Let { value, .. } = &s.body[0].kind else {
            panic!()
        };
        assert_eq!(
            *value,
            Expr::Bin(
                BinOp::Mul,
                Box::new(Expr::IntLit(-2)),
                Box::new(Expr::IntLit(3))
            )
        );
    }

    #[test]
    fn parses_remainder() {
        let m = parse("module m\nsub main\n  let x: int = 7 % 2\nend\n").unwrap();
        let s = m.subs().next().unwrap();
        let StmtKind::Let { value, .. } = &s.body[0].kind else {
            panic!()
        };
        assert!(matches!(value, Expr::Bin(BinOp::Rem, _, _)));
    }

    #[test]
    fn parses_for_with_and_without_a_step() {
        let m = parse("module m\nsub main\n  for i = 1 to 10\n  end\n  for j = 9 to 0 step -3\n  end\nend\n").unwrap();
        let s = m.subs().next().unwrap();
        let StmtKind::For { var, step, .. } = &s.body[0].kind else {
            panic!("expected a for")
        };
        assert_eq!((var.as_str(), *step), ("i", 1));
        let StmtKind::For { var, step, .. } = &s.body[1].kind else {
            panic!("expected a for")
        };
        assert_eq!((var.as_str(), *step), ("j", -3));
    }

    #[test]
    fn rejects_a_zero_step_and_a_non_literal_step() {
        assert!(parse("module m\nsub main\n  for i = 1 to 3 step 0\n  end\nend\n").is_err());
        assert!(parse("module m\nsub main\n  var k: int = 2\n  for i = 1 to 3 step k\n  end\nend\n").is_err());
        assert!(parse("module m\nsub main\n  for i = 1 10\n  end\nend\n").is_err());
    }

    /// `break` and `continue` must be reachable from BOTH statement loops —
    /// the one inside `sub` and the one inside `block` — which are separate
    /// code paths that each enumerate statement heads.
    #[test]
    fn parses_break_and_continue_in_both_statement_loops() {
        let m = parse(
            "module m\nsub main\n  break\n  while true\n    continue\n  end\nend\n",
        )
        .unwrap();
        let s = m.subs().next().unwrap();
        assert!(matches!(s.body[0].kind, StmtKind::Break));
        let StmtKind::While { body, .. } = &s.body[1].kind else {
            panic!()
        };
        assert!(matches!(body[0].kind, StmtKind::Continue));
    }

    /// `to` and `step` are soft keywords: they must stay usable as ordinary
    /// names everywhere else, or adding `for` would break existing files.
    #[test]
    fn to_and_step_are_still_ordinary_names() {
        let m = parse("module m\nsub main\n  var to: int = 1\n  var step: int = 2\n  to = step\nend\n").unwrap();
        assert_eq!(m.subs().next().unwrap().body.len(), 3);
    }

    #[test]
    fn parses_an_array_type_and_an_index() {
        let m = parse(
            "module m\nsub f(xs: text[]): int\n  let a: int = xs[0 + 1]\n  return a\nend\n",
        )
        .unwrap();
        let s = m.subs().next().unwrap();
        assert_eq!(s.params, vec![("xs".into(), Ty::Array(Elem::Text))]);
        let StmtKind::Let { value, .. } = &s.body[0].kind else {
            panic!("expected a let")
        };
        assert!(matches!(value, Expr::Index { .. }));
    }

    /// Indexing binds tighter than any operator: `xs[0] + 1` adds to the
    /// element, and folding it the other way would index the sum.
    #[test]
    fn indexing_binds_tighter_than_addition() {
        let m = parse("module m\nsub main\n  var xs: int[] = [1]\n  let a: int = xs[0] + 1\nend\n")
            .unwrap();
        let s = m.subs().next().unwrap();
        let StmtKind::Let { value, .. } = &s.body[1].kind else {
            panic!()
        };
        let Expr::Bin(BinOp::Add, l, _) = value else {
            panic!("expected an addition, got {value:?}")
        };
        assert!(matches!(**l, Expr::Index { .. }));
    }

    #[test]
    fn parses_an_element_assignment() {
        let m =
            parse("module m\nsub main\n  var xs: int[] = [1]\n  xs[0] = 2\nend\n").unwrap();
        let s = m.subs().next().unwrap();
        assert!(matches!(s.body[1].kind, StmtKind::SetIndex { .. }));
    }

    /// The limitation is named where it is hit, rather than surfacing as a
    /// complaint about the token that happens to follow.
    #[test]
    fn rejects_an_array_of_arrays() {
        let e = parse("module m\nsub main\n  let xs: int[][] = []\nend\n").unwrap_err();
        assert!(e.msg.contains("cannot hold arrays"), "{}", e.msg);
    }

    #[test]
    fn rejects_an_array_of_byte_sets() {
        assert!(parse("module m\nsub main\n  let b: bytes[] = []\nend\n").is_err());
    }

    #[test]
    fn parses_a_record_declaration_and_a_construction() {
        let m = parse(
            "module m\nrecord point\n  x: int\n  y: text\nend\n\
             sub main\n  let p: point = point(x: 1, y: \"a\")\nend\n",
        )
        .unwrap();
        let r = m.records().next().expect("a record");
        assert_eq!(r.name, "point");
        assert_eq!(r.fields, vec![("x".into(), Ty::Int), ("y".into(), Ty::Text)]);
        assert_eq!(r.field("y"), Some((2, Ty::Text)));
        let s = m.subs().next().unwrap();
        let StmtKind::Let { ty, value, .. } = &s.body[0].kind else {
            panic!("expected a let")
        };
        assert_eq!(*ty, Ty::Record("point"));
        assert!(matches!(value, Expr::RecordLit { .. }), "{value:?}");
    }

    /// `record` stays an ordinary word everywhere except that one position, or
    /// adding records would break files that already use it as a name.
    #[test]
    fn record_is_a_soft_keyword() {
        let m = parse("module m\nsub main\n  var record: int = 1\n  record = 2\nend\n").unwrap();
        assert_eq!(m.subs().next().unwrap().body.len(), 2);
    }

    /// `point(1, 2)` is a call and `point(x: 1)` is a record: the named first
    /// argument is the only difference, and two tokens of lookahead see it.
    #[test]
    fn a_named_first_argument_is_what_makes_it_a_record() {
        let m = parse("module m\nsub main\n  let a: int = f(1, 2)\n  let b: int = f(x: 1)\nend\n")
            .unwrap();
        let s = m.subs().next().unwrap();
        let StmtKind::Let { value, .. } = &s.body[0].kind else { panic!() };
        assert!(matches!(value, Expr::Call { .. }), "{value:?}");
        let StmtKind::Let { value, .. } = &s.body[1].kind else { panic!() };
        assert!(matches!(value, Expr::RecordLit { .. }), "{value:?}");
    }

    /// A `.` further along a chain is a field read; the first one is left as a
    /// property read for the checker to resolve.
    #[test]
    fn a_dot_after_an_index_is_a_field() {
        let m = parse("module m\nsub main\n  let n: int = ps[1].x\nend\n").unwrap();
        let s = m.subs().next().unwrap();
        let StmtKind::Let { value, .. } = &s.body[0].kind else { panic!() };
        let Expr::Field { base, name } = value else {
            panic!("expected a field read, got {value:?}")
        };
        assert_eq!(name, "x");
        assert!(matches!(**base, Expr::Index { .. }));
    }

    #[test]
    fn parses_a_dictionary_type_and_literal() {
        let m = parse(
            "module m\nsub main\n  var ages: int{} = {\"Ada\": 36}\n  ages[\"Alan\"] = 41\nend\n",
        )
        .unwrap();
        let s = m.subs().next().unwrap();
        let StmtKind::Let { ty, value, .. } = &s.body[0].kind else { panic!() };
        assert_eq!(*ty, Ty::Dict(Elem::Int));
        assert!(matches!(value, Expr::DictLit(p) if p.len() == 1), "{value:?}");
        assert!(matches!(s.body[1].kind, StmtKind::SetIndex { .. }));
    }

    #[test]
    fn rejects_a_record_with_no_fields() {
        let e = parse("module m\nrecord empty\nend\nsub main\nend\n").unwrap_err();
        assert!(e.msg.contains("has no fields"), "{}", e.msg);
    }

    #[test]
    fn precedence() {
        // 2 + 3 * 4 == 2 + (3*4)
        let m = parse("module m\nsub main\n  let x: int = 2 + 3 * 4\nend\n").unwrap();
        let Item::Sub(s) = &m.items[0] else {
            panic!("expected a subroutine")
        };
        match &s.body[0].kind {
            StmtKind::Let {
                value: Expr::Bin(BinOp::Add, _, r),
                ..
            } => {
                assert!(matches!(**r, Expr::Bin(BinOp::Mul, _, _)));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }
}
