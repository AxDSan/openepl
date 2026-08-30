//! Recursive-descent parser: `.oir` text -> `Module`.
//!
//! Grammar (v0):
//! ```text
//! module  := "module" IDENT NEWLINE item*
//! item    := sub
//! sub     := "sub" IDENT NEWLINE stmt* "end" NEWLINE
//! stmt    := let | call
//! let     := "let" IDENT ":" type "=" expr NEWLINE
//! call    := "call" IDENT "(" (expr ("," expr)*)? ")" NEWLINE
//! type    := "int" | "text"
//! expr    := term (("+" | "-") term)*
//! term    := factor (("*" | "/") factor)*
//! factor  := INT | FLOAT | STRING | call | IDENT | "(" expr ")"
//! call    := IDENT "(" (expr ("," expr)*)? ")"
//! ```

use crate::lexer::{lex, Spanned, Tok};
use crate::{BinOp, Component, Expr, Form, Item, Module, Stmt, Sub, Ty};

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

        // Optional `use <lib>` declarations precede the items.
        let mut uses = Vec::new();
        loop {
            self.skip_newlines();
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
                Tok::Eof => break,
                other => {
                    return self.err(format!("expected `sub` or end of file, found {other:?}"))
                }
            }
        }
        Ok(Module { name, uses, items })
    }

    fn sub(&mut self) -> Result<Sub, ParseError> {
        self.expect(&Tok::Sub, "`sub`")?;
        let name = self.ident("subroutine name")?;
        self.expect(&Tok::Newline, "newline after subroutine name")?;

        let mut body = Vec::new();
        loop {
            self.skip_newlines();
            match self.peek() {
                Tok::End => {
                    self.bump();
                    break;
                }
                Tok::Let => body.push(self.stmt_let()?),
                Tok::Call => body.push(self.stmt_call()?),
                // `component.property = expr`
                Tok::Ident(_) => body.push(self.stmt_set_property()?),
                Tok::Eof => return self.err("unexpected end of file inside `sub` (missing `end`)"),
                other => return self.err(format!("expected statement or `end`, found {other:?}")),
            }
        }
        // Trailing newline after `end` is optional (EOF is fine).
        if matches!(self.peek(), Tok::Newline) {
            self.bump();
        }
        Ok(Sub { name, body })
    }

    /// ```text
    /// form      := "form" IDENT NEWLINE member* "end" NEWLINE
    /// member    := property | binding | component
    /// property  := IDENT "=" expr NEWLINE
    /// binding   := "on" IDENT ":" IDENT NEWLINE
    /// component := IDENT IDENT NEWLINE (property | binding)* "end" NEWLINE
    /// ```
    fn form(&mut self) -> Result<Form, ParseError> {
        self.expect(&Tok::Form, "`form`")?;
        let name = self.ident("form name")?;
        self.expect(&Tok::Newline, "newline after form name")?;

        let mut form = Form {
            name,
            properties: Vec::new(),
            handlers: Vec::new(),
            children: Vec::new(),
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

    /// `component.property = expr`
    fn stmt_set_property(&mut self) -> Result<Stmt, ParseError> {
        let component = self.ident("component name")?;
        self.expect(&Tok::Dot, "`.` after component name")?;
        let property = self.ident("property name")?;
        self.expect(&Tok::Eq, "`=` in property assignment")?;
        let value = self.expr()?;
        self.expect(&Tok::Newline, "newline after property assignment")?;
        Ok(Stmt::SetProperty {
            component,
            property,
            value,
        })
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

    fn stmt_let(&mut self) -> Result<Stmt, ParseError> {
        self.expect(&Tok::Let, "`let`")?;
        let name = self.ident("variable name")?;
        self.expect(&Tok::Colon, "`:` after variable name")?;
        let ty = match self.bump() {
            Tok::Ident(w) => match Ty::from_keyword(&w) {
                Some(t) => t,
                None => {
                    return self.err(format!(
                        "expected a type (int/int64/double/text), found `{w}`"
                    ))
                }
            },
            other => return self.err(format!("expected a type, found {other:?}")),
        };
        self.expect(&Tok::Eq, "`=`")?;
        let value = self.expr()?;
        self.expect(&Tok::Newline, "newline after `let`")?;
        Ok(Stmt::Let { name, ty, value })
    }

    fn stmt_call(&mut self) -> Result<Stmt, ParseError> {
        self.expect(&Tok::Call, "`call`")?;
        let cmd = self.ident("command name")?;
        self.expect(&Tok::LParen, "`(`")?;
        let args = self.arg_list()?;
        self.expect(&Tok::Newline, "newline after call")?;
        Ok(Stmt::Call { cmd, args })
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
                _ => break,
            };
            self.bump();
            let rhs = self.factor()?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn factor(&mut self) -> Result<Expr, ParseError> {
        match self.bump() {
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
    fn precedence() {
        // 2 + 3 * 4 == 2 + (3*4)
        let m = parse("module m\nsub main\n  let x: int = 2 + 3 * 4\nend\n").unwrap();
        let Item::Sub(s) = &m.items[0] else {
            panic!("expected a subroutine")
        };
        match &s.body[0] {
            Stmt::Let {
                value: Expr::Bin(BinOp::Add, _, r),
                ..
            } => {
                assert!(matches!(**r, Expr::Bin(BinOp::Mul, _, _)));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }
}
