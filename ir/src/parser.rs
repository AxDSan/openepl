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
//! type    := "int" | "text"
//! expr    := term (("+" | "-") term)*
//! term    := factor (("*" | "/" | "%") factor)*
//! factor  := "-"? (INT | FLOAT | STRING | call | IDENT | "(" expr ")")
//! call    := IDENT "(" (expr ("," expr)*)? ")"
//! ```

use crate::lexer::{lex, Spanned, Tok};
use crate::{
    BinOp, CmpOp, Component, Expr, Form, GlobalVar, Item, LogicalOp, Module, Stmt, StmtKind, Sub,
    Target, Ty,
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
                Tok::Eof => break,
                other => {
                    return self.err(format!("expected `sub` or end of file, found {other:?}"))
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
                    let (event, handler) = self.binding()?;
                    form.handlers.push((event, handler));
                }
                Tok::Ident(first) => {
                    self.bump();
                    if matches!(self.peek(), Tok::Eq) {
                        self.bump();
                        let value = self.expr()?;
                        self.expect(&Tok::Newline, "newline after property")?;
                        form.properties.push((first, value));
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

    /// A component instance; `type_name` has already been consumed.
    fn component(&mut self, type_name: String) -> Result<Component, ParseError> {
        let id = self.ident("component id")?;
        self.expect(&Tok::Newline, "newline after component id")?;
        let mut c = Component {
            type_name,
            id,
            properties: Vec::new(),
            handlers: Vec::new(),
        };
        loop {
            self.skip_newlines();
            match self.peek().clone() {
                Tok::End => {
                    self.bump();
                    break;
                }
                Tok::On => {
                    let (event, handler) = self.binding()?;
                    c.handlers.push((event, handler));
                }
                Tok::Ident(name) => {
                    self.bump();
                    self.expect(&Tok::Eq, "`=` after property name")?;
                    let value = self.expr()?;
                    self.expect(&Tok::Newline, "newline after property")?;
                    c.properties.push((name, value));
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

    /// `on <event>: <subroutine>`
    fn binding(&mut self) -> Result<(String, String), ParseError> {
        self.expect(&Tok::On, "`on`")?;
        let event = self.ident("event name")?;
        self.expect(&Tok::Colon, "`:` after event name")?;
        let handler = self.ident("handler subroutine name")?;
        self.expect(&Tok::Newline, "newline after event binding")?;
        Ok((event, handler))
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

    fn type_keyword(&mut self) -> Result<Ty, ParseError> {
        match self.bump() {
            Tok::Ident(w) => match Ty::from_keyword(&w) {
                Some(t) => Ok(t),
                None => self.err(format!(
                    "expected a type (int/int64/double/text), found `{w}`"
                )),
            },
            other => self.err(format!("expected a type, found {other:?}")),
        }
    }

    /// A statement starting with an identifier: assignment or property-set.
    fn stmt_ident(&mut self) -> Result<Stmt, ParseError> {
        let stmt_line = self.line();
        let name = self.ident("variable or component name")?;
        match self.peek() {
            Tok::Eq => {
                self.bump();
                let value = self.expr()?;
                self.expect(&Tok::Newline, "newline after assignment")?;
                Ok(Stmt::new(StmtKind::Assign { name, value }, stmt_line))
            }
            Tok::Dot => {
                self.bump();
                let property = self.ident("property name")?;
                self.expect(&Tok::Eq, "`=` in property assignment")?;
                let value = self.expr()?;
                self.expect(&Tok::Newline, "newline after property assignment")?;
                Ok(Stmt::new(StmtKind::SetProperty {
                    component: name,
                    property,
                    value,
                }, stmt_line))
            }
            other => self.err(format!(
                "expected `=` (assignment) or `.` (property) after `{name}`, found {other:?}"
            )),
        }
    }

    fn stmt_let(&mut self, mutable: bool) -> Result<Stmt, ParseError> {
        let stmt_line = self.line();
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
        Ok(Stmt::new(StmtKind::Let {
            name,
            ty,
            value,
            mutable,
        }, stmt_line))
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
        let stmt_line = self.line();
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
        Ok(Stmt::new(StmtKind::If { arms, otherwise }, stmt_line))
    }

    /// `while COND NEWLINE ... end`
    fn stmt_while(&mut self) -> Result<Stmt, ParseError> {
        let stmt_line = self.line();
        self.expect(&Tok::While, "`while`")?;
        let cond = self.expr()?;
        self.expect(&Tok::Newline, "newline after the condition")?;
        let body = self.block(&[Tok::End])?;
        self.expect(&Tok::End, "`end`")?;
        if matches!(self.peek(), Tok::Newline) {
            self.bump();
        }
        Ok(Stmt::new(StmtKind::While { cond, body }, stmt_line))
    }

    /// ```text
    /// for := "for" IDENT "=" expr "to" expr ("step" INT)? NEWLINE stmt* "end"
    /// ```
    ///
    /// `to` and `step` are **soft** keywords, matched as identifiers in these
    /// two positions only — reserving them would steal two ordinary words from
    /// every variable and property name in the language.
    fn stmt_for(&mut self) -> Result<Stmt, ParseError> {
        let stmt_line = self.line();
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
        Ok(Stmt::new(
            StmtKind::For {
                var,
                start,
                limit,
                step,
                body,
            },
            stmt_line,
        ))
    }

    /// `break` / `continue`.
    fn stmt_jump(&mut self, is_break: bool) -> Result<Stmt, ParseError> {
        let stmt_line = self.line();
        self.bump();
        if matches!(self.peek(), Tok::Newline) {
            self.bump();
        }
        Ok(Stmt::new(
            if is_break {
                StmtKind::Break
            } else {
                StmtKind::Continue
            },
            stmt_line,
        ))
    }

    /// `return` or `return EXPR`. A bare `return` is the one that leaves a sub
    /// with no return type early; anything else on the line is the value.
    fn stmt_return(&mut self) -> Result<Stmt, ParseError> {
        let stmt_line = self.line();
        self.expect(&Tok::Return, "`return`")?;
        let value = if matches!(self.peek(), Tok::Newline | Tok::Eof) {
            None
        } else {
            Some(self.expr()?)
        };
        if matches!(self.peek(), Tok::Newline) {
            self.bump();
        }
        Ok(Stmt::new(StmtKind::Return { value }, stmt_line))
    }

    fn stmt_call(&mut self) -> Result<Stmt, ParseError> {
        let stmt_line = self.line();
        self.expect(&Tok::Call, "`call`")?;
        let cmd = self.ident("command name")?;
        self.expect(&Tok::LParen, "`(`")?;
        let args = self.arg_list()?;
        self.expect(&Tok::Newline, "newline after call")?;
        Ok(Stmt::new(StmtKind::Call { cmd, args }, stmt_line))
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
        match self.bump() {
            Tok::True => Ok(Expr::BoolLit(true)),
            Tok::False => Ok(Expr::BoolLit(false)),
            Tok::Int(v) => Ok(Expr::IntLit(v)),
            Tok::Float(v) => Ok(Expr::DoubleLit(v)),
            Tok::Str(s) => Ok(Expr::TextLit(s)),
            Tok::Ident(name) => {
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
