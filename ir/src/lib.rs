//! OpenEPL e-code IR — v0.1 (Phase 1).
//!
//! The keystone artifact: a typed, tree-structured IR.  Phase 1 grows
//! the v0 slice with `int64`/`double` slot types, **command return types** and
//! **call-expressions** (so core commands are usable inside expressions), a
//! shared **command registry** (`registry`), a reusable **type checker**
//! (`sema`), and an **IR validator** (`validate`.1).
//!
//! The schema still reserves room for the form / component / property / event
//! nodes that later phases add (see `Item`).  Byte-set (`SDT_BIN`) and the
//! aggregate storage ABI are specified but deferred to Phase 2 (they ride on the
//! runtime↔library notification channel / memory-ownership model.2/D4).
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

/// What an array holds.  A separate, deliberately small enum rather than a
/// `Box<Ty>`: arrays of arrays are out of scope, and spelling that in the type
/// itself means the checker never has to discover it — `Ty` also stays `Copy`,
/// which is what every `vars.get(..).copied()` in the checker relies on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Elem {
    Int,
    Int64,
    Double,
    Text,
    Bool,
}

impl Elem {
    /// The element type as an ordinary type.
    pub fn ty(self) -> Ty {
        match self {
            Elem::Int => Ty::Int,
            Elem::Int64 => Ty::Int64,
            Elem::Double => Ty::Double,
            Elem::Text => Ty::Text,
            Elem::Bool => Ty::Bool,
        }
    }

    /// The element types a scalar can be stored as; `None` for anything an
    /// array cannot hold (an array, a byte-set).
    pub fn from_ty(t: Ty) -> Option<Elem> {
        Some(match t {
            Ty::Int => Elem::Int,
            Ty::Int64 => Elem::Int64,
            Ty::Double => Elem::Double,
            Ty::Text => Elem::Text,
            Ty::Bool => Elem::Bool,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        self.ty().as_str()
    }
}

/// Slot data-type tags — the ABI type system.  Phase 1
/// exposes the numeric + text core; Phase 3 adds the aggregates.
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
    /// `SDT_BIN` — a byte-set: raw bytes, the type a PNG lives in.
    Bytes,
    /// An array of `Elem`. The value in the slot is a pointer to a
    /// runtime-owned array object, exactly as text is a pointer.
    Array(Elem),
    /// **Signature-only**: "an array, whatever it holds".
    ///
    /// `count`, `sort` and `join` do not care what is in the array — the array
    /// carries its element tag at run time. Without this they would need one
    /// command per element type, which is five spellings of one idea. It never
    /// names a variable: there is no surface syntax for it.
    AnyArray,
    /// **Signature-only**: "whatever THIS call's array argument holds".
    ///
    /// This is what makes `append(xs, 5)` on a `text[]` an error with a line
    /// number instead of a silently mistyped element.
    AnyElem,
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
            Ty::Bytes => "bytes",
            Ty::Array(Elem::Int) => "int[]",
            Ty::Array(Elem::Int64) => "int64[]",
            Ty::Array(Elem::Double) => "double[]",
            Ty::Array(Elem::Text) => "text[]",
            Ty::Array(Elem::Bool) => "bool[]",
            // Read in a signature, not in a sentence: `openepl commands` shows
            // `append(array, element) -> array`. The diagnostics that need a
            // phrase spell it out themselves.
            Ty::AnyArray => "array",
            Ty::AnyElem => "element",
        }
    }

    /// Parse a type keyword; `None` if unknown.  The `[]` suffix is the
    /// parser's business — it is syntax, not a keyword.
    pub fn from_keyword(s: &str) -> Option<Ty> {
        Some(match s {
            "int" => Ty::Int,
            "int64" => Ty::Int64,
            "double" => Ty::Double,
            "text" => Ty::Text,
            "bool" => Ty::Bool,
            "bytes" => Ty::Bytes,
            _ => return None,
        })
    }

    /// Whether arithmetic operators (`+ - * /`) apply.
    pub fn is_numeric(self) -> bool {
        matches!(self, Ty::Int | Ty::Int64 | Ty::Double)
    }

    /// What this array holds, or `None` if it is not one array in particular.
    pub fn elem(self) -> Option<Elem> {
        match self {
            Ty::Array(e) => Some(e),
            _ => None,
        }
    }

    /// Whether a value of this type is a pointer in the slot's value union —
    /// text and both aggregates. The backend marshals all of them identically.
    pub fn is_pointer(self) -> bool {
        matches!(
            self,
            Ty::Text | Ty::Bytes | Ty::Array(_) | Ty::AnyArray | Ty::AnyElem
        )
    }

    /// The ABI `SDT_*` numeric tag (must match `abi/openepl_abi.h`).
    ///
    /// Array-ness is a flag bit above the element tag rather than a block of
    /// new numbers, because every `SDT_*` value is frozen: `int[]` has to be
    /// expressible without moving `int`.
    pub fn sdt_tag(self) -> i32 {
        const ARRAY: i32 = 0x100; // OE_SDT_ARRAY_FLAG
        const ALL: i32 = 255; // OE_SDT_ALL
        match self {
            Ty::Int => 3,    // OE_SDT_INT
            Ty::Int64 => 4,  // OE_SDT_INT64
            Ty::Double => 6, // OE_SDT_DOUBLE
            Ty::Text => 9,   // OE_SDT_TEXT
            Ty::Bool => 8,   // OE_SDT_BOOL
            Ty::Bytes => 10, // OE_SDT_BIN
            Ty::Array(e) => ARRAY | e.ty().sdt_tag(),
            Ty::AnyArray => ARRAY | ALL,
            Ty::AnyElem => ALL,
        }
    }

    /// Map an ABI `SDT_*` tag back to an IR type; `None` for tags not modeled in
    /// this phase (or `OE_SDT_NULL`, used for void returns).
    pub fn from_sdt_tag(tag: i32) -> Option<Ty> {
        const ARRAY: i32 = 0x100;
        const ALL: i32 = 255;
        if tag == ARRAY | ALL {
            return Some(Ty::AnyArray);
        }
        if tag & ARRAY != 0 {
            return Elem::from_ty(Ty::from_sdt_tag(tag & !ARRAY)?).map(Ty::Array);
        }
        Some(match tag {
            3 => Ty::Int,
            4 => Ty::Int64,
            6 => Ty::Double,
            9 => Ty::Text,
            8 => Ty::Bool,
            10 => Ty::Bytes,
            ALL => Ty::AnyElem,
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
    /// `%` — remainder. On integers this is `mod_int`'s operator spelling; on
    /// doubles it is the IEEE remainder.
    Rem,
}

impl BinOp {
    pub fn symbol(self) -> char {
        match self {
            BinOp::Add => '+',
            BinOp::Sub => '-',
            BinOp::Mul => '*',
            BinOp::Div => '/',
            BinOp::Rem => '%',
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
    /// Call to a non-void command, used as a value.
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
    /// `xs[i]` — one element of an array, or one byte of a byte-set (which
    /// reads as an `int` 0..255). Bounds are checked at run time; a constant
    /// index the checker can see is checked before the program is built.
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    /// `[a, b, c]` — a new array. Every element must share one type; an empty
    /// `[]` takes its type from where it is going, which is why the checker
    /// carries an expected type rather than inferring bottom-up alone.
    ArrayLit(Vec<Expr>),
    /// `-EXPR` — arithmetic negation of a numeric value.
    ///
    /// Negated *literals* never reach here: the parser folds `-5` into
    /// `IntLit(-5)` so that it types `int` (an unfolded `2147483648` would
    /// type `int64` and make `let x: int = -2147483648` fail), and so that a
    /// form property, which must be a literal, can be negative.
    Neg(Box<Expr>),
}

/// A statement plus where it came from.
///
/// The line lives on the wrapper rather than inside each variant so that
/// existing pattern matches keep working, and so that a diagnostic can always
/// say *where* — an error without a position is nearly useless in an editor.
#[derive(Debug, Clone, PartialEq)]
pub struct Stmt {
    pub kind: StmtKind,
    /// 1-based source line; 0 when unknown.
    pub line: usize,
}

impl Stmt {
    pub fn new(kind: StmtKind, line: usize) -> Stmt {
        Stmt { kind, line }
    }
}

/// A single statement inside a subroutine body.
#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
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
    /// `xs[i] = EXPR` — replace one element in place.
    ///
    /// This changes the array, not the name, so it is allowed on a `let`: the
    /// binding still refers to the same array. `let` promises the name will
    /// not be re-pointed, exactly as it does for a component id.
    SetIndex {
        name: String,
        index: Expr,
        value: Expr,
    },
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
    /// `for NAME = START to LIMIT [step K] ... end`.
    ///
    /// `NAME` is an `int` that exists for the loop and is immutable inside it.
    /// `start` and `limit` are evaluated **once**, before the first iteration,
    /// so a loop cannot be lengthened by its own body. `step` is a non-zero
    /// integer literal: knowing its sign at compile time is what lets the
    /// comparison be `<=` for a counting-up loop and `>=` for a counting-down
    /// one, with no run-time test.
    For {
        var: String,
        start: Expr,
        limit: Expr,
        step: i64,
        body: Vec<Stmt>,
    },
    /// `break` — leave the innermost loop.
    Break,
    /// `continue` — skip to the innermost loop's next iteration.
    Continue,
    /// `return` (from a sub with no return type) or `return EXPR`.
    Return { value: Option<Expr> },
}

/// A subroutine (EPL 子程序).
///
/// `main` and event handlers are subs with an empty parameter list and no
/// return type — the shape every sub had before parameters existed, which is
/// why `sub main` keeps parsing and lowering exactly as it did.
#[derive(Debug, Clone, PartialEq)]
pub struct Sub {
    pub name: String,
    /// Declared parameters, in order: `(name, type)`. Parameters are immutable
    /// inside the body, like a `let`.
    pub params: Vec<(String, Ty)>,
    /// Declared return type; `None` is a sub that returns nothing and may only
    /// be invoked with `call`.
    pub ret: Option<Ty>,
    /// 1-based source line of the `sub` keyword; 0 when unknown.
    pub line: usize,
    pub body: Vec<Stmt>,
}

impl Sub {
    /// The sub's signature, in the same shape a library command has — so one
    /// argument checker serves both.
    pub fn signature(&self) -> Signature {
        Signature {
            params: self.params.iter().map(|(_, t)| *t).collect(),
            ret: self.ret,
        }
    }

    /// Whether this sub has the shape an entry point or an event handler needs:
    /// nothing in, nothing out.
    pub fn is_plain(&self) -> bool {
        self.params.is_empty() && self.ret.is_none()
    }
}

/// One component instance inside a form: a type, an id, literal property
/// values, and event bindings to subroutines.  This is the IR half of the
/// component model — the designer will emit exactly this.
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

/// A form: the root component plus its children.
#[derive(Debug, Clone, PartialEq)]
pub struct Form {
    pub name: String,
    /// 1-based `(first, last)` source lines of the whole `form … end` block.
    ///
    /// The designer rewrites **only** these lines when saving, splicing them
    /// into the original file. Re-emitting the whole module would destroy every
    /// hand-written subroutine body.
    pub line_span: (usize, usize),
    /// Properties of the form itself (title, size, …).
    pub properties: Vec<(String, Expr)>,
    pub handlers: Vec<(String, String)>,
    pub children: Vec<Component>,
}

/// A top-level module item.  Remaining seam: `UserType`, `Const`, `Enum`
///.
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

/// What a module is compiled into.
///
/// The target changes the *entry contract*, not the language: an executable
/// gets `ECodeStart`, a library gets no entry and exports its subroutines
/// instead. Everything else about lowering is identical, which is the point —
/// shipping a `.so` is a build-target choice, not a rewrite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// Terminal program; `main` is the entry.
    Console,
    /// Windowed program; the form drives the entry, `main` runs first if present.
    Gui,
    /// `.so` / `.dll` / `.dylib` — no entry, subroutines exported.
    SharedLib,
    /// `.a` / `.lib` — no entry, subroutines exported, linked into a host.
    StaticLib,
}

impl Target {
    pub fn as_str(self) -> &'static str {
        match self {
            Target::Console => "console",
            Target::Gui => "gui",
            Target::SharedLib => "sharedlib",
            Target::StaticLib => "staticlib",
        }
    }

    pub fn parse(s: &str) -> Option<Target> {
        match s {
            "console" => Some(Target::Console),
            "gui" => Some(Target::Gui),
            "sharedlib" | "shared" | "dll" | "so" => Some(Target::SharedLib),
            "staticlib" | "static" => Some(Target::StaticLib),
            _ => None,
        }
    }

    /// Does this target produce a program with an entry point?
    pub fn is_executable(self) -> bool {
        matches!(self, Target::Console | Target::Gui)
    }

    /// The conventional file extension on this platform, or `""` for an
    /// executable (which has none on Unix).
    pub fn extension(self) -> &'static str {
        match self {
            Target::Console | Target::Gui => "",
            // Linux naming for now; Windows/macOS land with Phase 4's
            // cross-platform work.
            Target::SharedLib => "so",
            Target::StaticLib => "a",
        }
    }
}

/// A whole compilation unit.
#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub name: String,
    /// What to build this module into. `None` means "infer": a module with a
    /// form is a GUI program, otherwise a console one — so every file written
    /// before targets existed still means what it meant.
    pub target: Option<Target>,
    /// Support libraries this module uses (`use <name>`), beyond the implicit
    /// `core`.  The compiler introspects each for command signatures and links
    /// its implementations.
    pub uses: Vec<String>,
    pub items: Vec<Item>,
}

impl Module {
    /// The target to build, declared or inferred.
    pub fn target(&self) -> Target {
        self.target.unwrap_or_else(|| {
            if self.forms().next().is_some() {
                Target::Gui
            } else {
                Target::Console
            }
        })
    }

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
