//! OpenEPL e-code IR — v0 (Phase 0 subset).
//!
//! This is the *keystone* artifact (PRD G1): a typed, tree-structured IR.  v0
//! implements only what the "print + arithmetic" vertical slice needs — modules,
//! subroutines, `let` bindings, command calls, and arithmetic expressions over
//! `SDT_INT` / `SDT_TEXT` slots.  The schema deliberately leaves reserved room
//! for the form / component / property / event nodes that Phase 2+ will add
//! (see `Item`), but codegen for those is out of scope here.
//!
//! Only the *text* encoding is implemented in v0; the compact binary encoding
//! (the shipping form) is specified as deferred in `docs/spec/ir.md`.

pub mod lexer;
pub mod parser;

pub use parser::{parse, ParseError};

/// Slot data-type tags — the ABI type system (PRD §1.2, `SDT_*`).  v0 exposes
/// only the two the slice uses; the full set is frozen in `docs/spec/abi.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ty {
    /// `SDT_INT` — 32-bit signed integer.
    Int,
    /// `SDT_TEXT` — pointer to a NUL-terminated string; NULL = empty.
    Text,
}

impl Ty {
    pub fn as_str(self) -> &'static str {
        match self {
            Ty::Int => "int",
            Ty::Text => "text",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
}

/// An expression.  Every expression has a static `Ty` (see `crate`-level
/// checking in the backend); v0 keeps the tree small and total.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Integer literal (stored widened; lowered as 32-bit `SDT_INT`).
    IntLit(i64),
    /// Text literal (decoded, unescaped bytes).
    TextLit(String),
    /// Reference to a `let`-bound local.
    Var(String),
    /// Binary arithmetic (int only in v0).
    Bin(BinOp, Box<Expr>, Box<Expr>),
}

/// A single statement inside a subroutine body.
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// `let NAME: TY = EXPR` — bind a local (immutable in v0).
    Let { name: String, ty: Ty, value: Expr },
    /// `call CMD(args...)` — the one uniform call form (PRD §5.0).
    Call { cmd: String, args: Vec<Expr> },
}

/// A subroutine (EPL 子程序).  v0 subs take no params and return nothing;
/// `main` is the program entry, lowered to `ECodeStart` (PRD §1.4 entry model).
#[derive(Debug, Clone, PartialEq)]
pub struct Sub {
    pub name: String,
    pub body: Vec<Stmt>,
}

/// A top-level module item.  v0 only has subroutines, but the enum is the
/// reserved seam where `Form`, `Component`, `UserType`, `Const`, `Enum` land
/// (PRD §5.1 / §4.5) without reshaping callers.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Sub(Sub),
    // Reserved for later phases (not parsed/lowered in v0):
    //   Form(Form), Component(Component), UserType(..), Const(..), Enum(..)
}

/// A whole compilation unit.
#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub name: String,
    pub items: Vec<Item>,
}

impl Module {
    /// Convenience: iterate the subroutines.
    pub fn subs(&self) -> impl Iterator<Item = &Sub> {
        self.items.iter().map(|i| match i {
            Item::Sub(s) => s,
        })
    }
}
