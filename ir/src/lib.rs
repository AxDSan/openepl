//! OpenEPL e-code IR — v0.1 (Phase 1).
//!
//! The keystone artifact (PRD G1): a typed, tree-structured IR.  Phase 1 grows
//! the v0 slice with `int64`/`double` slot types, **command return types** and
//! **call-expressions** (so core commands are usable inside expressions), a
//! shared **command registry** (`registry`), a reusable **type checker**
//! (`sema`), and an **IR validator** (`validate`, PRD §5.1).
//!
//! The schema still reserves room for the form / component / property / event
//! nodes that later phases add (see `Item`).  Byte-set (`SDT_BIN`) and the
//! aggregate storage ABI are specified but deferred to Phase 2 (they ride on the
//! runtime↔library notification channel / memory-ownership model, PRD §1.2/D4).
//!
//! Only the *text* encoding is implemented; the binary encoding stays deferred.

pub mod lexer;
pub mod parser;
pub mod registry;
pub mod sema;
pub mod validate;

pub use parser::{parse, ParseError};
pub use registry::Registry;
pub use validate::{validate, ValidateError};

/// Slot data-type tags — the ABI type system (PRD §1.2, `SDT_*`).  Phase 1
/// exposes the numeric + text core; the full set is frozen in `docs/spec/abi.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ty {
    /// `SDT_INT` — 32-bit signed integer.
    Int,
    /// `SDT_INT64` — 64-bit signed integer.
    Int64,
    /// `SDT_DOUBLE` — 64-bit IEEE-754 float.
    Double,
    /// `SDT_TEXT` — pointer to a NUL-terminated string; NULL = empty.
    Text,
    /// `SDT_BOOL` — truth value. Carried as an int-sized value, matching the
    /// ABI's `BOOL`, so slot marshaling has one less width to juggle.
    Bool,
}

impl Ty {
    /// The surface-syntax / spec spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Ty::Int => "int",
            Ty::Int64 => "int64",
            Ty::Double => "double",
            Ty::Text => "text",
            Ty::Bool => "bool",
        }
    }

    /// Parse a type keyword; `None` if unknown.
    pub fn from_keyword(s: &str) -> Option<Ty> {
        Some(match s {
            "int" => Ty::Int,
            "int64" => Ty::Int64,
            "double" => Ty::Double,
            "text" => Ty::Text,
            "bool" => Ty::Bool,
            _ => return None,
        })
    }

    /// Whether arithmetic operators (`+ - * /`) apply.
    pub fn is_numeric(self) -> bool {
        matches!(self, Ty::Int | Ty::Int64 | Ty::Double)
    }

    /// The ABI `SDT_*` numeric tag (must match `abi/openepl_abi.h`).
    pub fn sdt_tag(self) -> i32 {
        match self {
            Ty::Int => 3,    // OE_SDT_INT
            Ty::Int64 => 4,  // OE_SDT_INT64
            Ty::Double => 6, // OE_SDT_DOUBLE
            Ty::Text => 9,   // OE_SDT_TEXT
            Ty::Bool => 8,   // OE_SDT_BOOL
        }
    }

    /// Map an ABI `SDT_*` tag back to an IR type; `None` for tags not modeled in
    /// this phase (or `OE_SDT_NULL`, used for void returns).
    pub fn from_sdt_tag(tag: i32) -> Option<Ty> {
        Some(match tag {
            3 => Ty::Int,
            4 => Ty::Int64,
            6 => Ty::Double,
            9 => Ty::Text,
            8 => Ty::Bool,
            _ => return None,
        })
    }
}

/// A command's signature: parameter slot types plus an optional return slot
/// (`None` = a void command, callable only as a statement).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    pub params: Vec<Ty>,
    pub ret: Option<Ty>,
}

/// Comparison operators. `=` is equality *in expression position*; assignment
/// exists only as a statement, so `if x = 5` cannot silently assign (the C
/// footgun is structurally impossible here).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl CmpOp {
    pub fn symbol(self) -> &'static str {
        match self {
            CmpOp::Eq => "=",
            CmpOp::Ne => "<>",
            CmpOp::Lt => "<",
            CmpOp::Le => "<=",
            CmpOp::Gt => ">",
            CmpOp::Ge => ">=",
        }
    }
    /// Ordering comparisons apply to numbers only; `=`/`<>` also apply to text.
    pub fn is_ordering(self) -> bool {
        !matches!(self, CmpOp::Eq | CmpOp::Ne)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalOp {
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
}

impl BinOp {
    pub fn symbol(self) -> char {
        match self {
            BinOp::Add => '+',
            BinOp::Sub => '-',
            BinOp::Mul => '*',
            BinOp::Div => '/',
        }
    }
}

/// An expression.  Every expression has a static `Ty` (see `sema`).
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Integer literal — typed `int` unless it overflows `i32` (then `int64`).
    IntLit(i64),
    /// Floating-point literal (`double`).
    DoubleLit(f64),
    /// Text literal (decoded, unescaped bytes).
    TextLit(String),
    /// Reference to a `let`-bound local.
    Var(String),
    /// Binary arithmetic; both operands must share one numeric type.
    Bin(BinOp, Box<Expr>, Box<Expr>),
    /// Call to a non-void command, used as a value (PRD §5.0 uniform call form).
    Call { cmd: String, args: Vec<Expr> },
    /// Read a component's property: `ok_button.text`.
    GetProperty { component: String, property: String },
    /// `true` / `false`.
    BoolLit(bool),
    /// A comparison; yields `bool`. Non-associative: `a < b < c` is rejected.
    Cmp(CmpOp, Box<Expr>, Box<Expr>),
    /// `and` / `or`, short-circuiting.
    Logical(LogicalOp, Box<Expr>, Box<Expr>),
    /// `not EXPR`.
    Not(Box<Expr>),
}

/// A single statement inside a subroutine body.
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// `let NAME: TY = EXPR` (immutable) or `var NAME: TY = EXPR` (mutable).
    Let {
        name: String,
        ty: Ty,
        value: Expr,
        mutable: bool,
    },
    /// `NAME = EXPR` — assign to a mutable variable (local or module-level).
    Assign { name: String, value: Expr },
    /// `call CMD(args...)` — a call in statement position; a non-void return is
    /// discarded.
    Call { cmd: String, args: Vec<Expr> },
    /// `ok_button.text = EXPR` — assign a component property at run time.
    SetProperty {
        component: String,
        property: String,
        value: Expr,
    },
    /// `if COND ... else if COND ... else ... end`.
    ///
    /// `arms` holds the `if`/`else if` conditions with their bodies, in order;
    /// `otherwise` is the optional final `else`.
    If {
        arms: Vec<(Expr, Vec<Stmt>)>,
        otherwise: Option<Vec<Stmt>>,
    },
    /// `while COND ... end`.
    While { cond: Expr, body: Vec<Stmt> },
}

/// A subroutine (EPL 子程序).  v0.1 subs take no params and return nothing;
/// `main` is the program entry, lowered to `ECodeStart` (PRD §1.4).
#[derive(Debug, Clone, PartialEq)]
pub struct Sub {
    pub name: String,
    pub body: Vec<Stmt>,
}

/// One component instance inside a form: a type, an id, literal property
/// values, and event bindings to subroutines.  This is the IR half of the
/// component model (PRD D9/D11) — the designer will emit exactly this.
#[derive(Debug, Clone, PartialEq)]
pub struct Component {
    /// Registered component type name, e.g. `button`.
    pub type_name: String,
    /// Author-chosen instance id, e.g. `ok_button`.  **Compile-time only** in
    /// v0: ids are not emitted into the binary (G8).
    pub id: String,
    /// Property assignments, in source order: `(name, literal)`.
    pub properties: Vec<(String, Expr)>,
    /// Event bindings: `(event name, handler subroutine name)`.
    pub handlers: Vec<(String, String)>,
}

/// A form: the root component plus its children (PRD §4.5).
#[derive(Debug, Clone, PartialEq)]
pub struct Form {
    pub name: String,
    /// 1-based `(first, last)` source lines of the whole `form … end` block.
    ///
    /// The designer rewrites **only** these lines when saving, splicing them
    /// into the original file. Re-emitting the whole module would destroy every
    /// hand-written subroutine body (ADR 0011).
    pub line_span: (usize, usize),
    /// Properties of the form itself (title, size, …).
    pub properties: Vec<(String, Expr)>,
    pub handlers: Vec<(String, String)>,
    pub children: Vec<Component>,
}

/// A top-level module item.  Remaining seam: `UserType`, `Const`, `Enum`
/// (PRD §5.1 / §4.5).
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Sub(Sub),
    Form(Form),
    /// A module-level mutable variable: `var count: int = 0`.
    ///
    /// This is where state that must outlive a single event handler lives —
    /// without it, an app has nowhere to keep a counter but the UI itself.
    Var(GlobalVar),
}

/// A module-level variable.
#[derive(Debug, Clone, PartialEq)]
pub struct GlobalVar {
    pub name: String,
    pub ty: Ty,
    /// Initializer. A literal becomes a static initializer; anything else is
    /// evaluated at program entry, in declaration order.
    pub value: Expr,
}

/// A whole compilation unit.
#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub name: String,
    /// Support libraries this module uses (`use <name>`), beyond the implicit
    /// `core`.  The compiler introspects each for command signatures and links
    /// its implementations (PRD §5.4).
    pub uses: Vec<String>,
    pub items: Vec<Item>,
}

impl Module {
    /// Iterate the subroutines.
    pub fn subs(&self) -> impl Iterator<Item = &Sub> {
        self.items.iter().filter_map(|i| match i {
            Item::Sub(s) => Some(s),
            _ => None,
        })
    }

    /// Iterate the module-level variables, in declaration order.
    pub fn globals(&self) -> impl Iterator<Item = &GlobalVar> {
        self.items.iter().filter_map(|i| match i {
            Item::Var(v) => Some(v),
            _ => None,
        })
    }

    /// Iterate the forms.
    pub fn forms(&self) -> impl Iterator<Item = &Form> {
        self.items.iter().filter_map(|i| match i {
            Item::Form(f) => Some(f),
            _ => None,
        })
    }

    /// A GUI module (one that declares a form) links the UI stack and uses the
    /// form entry; a console module keeps the original `main` path.
    pub fn is_gui(&self) -> bool {
        self.forms().next().is_some()
    }
}
