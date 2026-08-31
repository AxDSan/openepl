//! OpenEPL backend (Phase 2): typed IR -> textual LLVM IR (`.ll`), emitting the
//! real **slot ABI** calling convention (abi/openepl_abi.h).
//!
//! Every command is invoked as `void cmd(Slot* ret, i32 argc, Slot* argv)`
//! (PRD §11).  For each call the backend allocates an argv array of `%Slot`s and
//! a return slot, stores each argument's tag + reinterpreted 64-bit value, calls
//! the command by its runtime symbol (no dispatch table, no ordinal indirection
//! — G8), then reads the return slot back.  `clang` assembles + links this
//! against the static-linked command implementations (BlackMoon model, D1).
//!
//! `%Slot = { i32 tag, i32 pad, i64 value }` mirrors `OpenEPL_Slot` (16 bytes,
//! value at offset 8), enforced by `_Static_assert` on the C side.
//!
//! Assumes the module passed `openepl_ir::validate`.  Entry is `ECodeStart`.

use std::collections::{BTreeSet, HashMap};
use std::fmt::Write as _;

use openepl_ir::{BinOp, CmpOp, Expr, LogicalOp, Module, Registry, Ty};

/// Accessibility role for a form root (`OE_ROLE_WINDOW`, abi/openepl_abi.h).
fn form_role() -> i32 {
    1
}

#[derive(Debug, Clone, PartialEq)]
pub struct LowerError {
    pub msg: String,
}
impl std::fmt::Display for LowerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "lowering error: {}", self.msg)
    }
}
impl std::error::Error for LowerError {}

fn err<T>(msg: impl Into<String>) -> Result<T, LowerError> {
    Err(LowerError { msg: msg.into() })
}

fn llvm_ty(t: Ty) -> &'static str {
    match t {
        Ty::Int => "i32",
        Ty::Int64 => "i64",
        Ty::Double => "double",
        Ty::Text => "ptr",
        // Bool is int-sized, matching the ABI's BOOL: `icmp` yields i1, which we
        // widen immediately so slot marshaling has one less width to handle.
        Ty::Bool => "i32",
    }
}

/// Lower a whole module to a `.ll` string using the given command registry.
///
/// Entry shapes depend on the module's target (PRD G12):
///  * **console** — `main` is lowered into `ECodeStart` as before.
///  * **GUI** — the module declares a form; `ECodeStart` becomes the generated
///    form constructor: init the UI, create components, set properties, bind
///    handlers by function pointer, run the loop. `main`, if present, runs
///    first as start-up code.
///  * **library** (shared or static) — no entry at all. Each subroutine gets an
///    exported wrapper under its plain name so a host can call it through the
///    C ABI. Internal calls keep the mangled name, so the two never collide.
///
/// User subroutines each lower to their own `@oe_user_<name>` function so an
/// event handler can be bound by pointer. Handler names never appear as data —
/// there is no name-based dispatch at runtime (G8).
pub fn lower_module(m: &Module, reg: &Registry) -> Result<String, LowerError> {
    let target = m.target();
    let subs: Vec<_> = m.subs().collect();
    let forms: Vec<_> = m.forms().collect();
    if target.is_executable() && forms.is_empty() && !subs.iter().any(|s| s.name == "main") {
        return err("module has no `main` subroutine and no form");
    }
    if !target.is_executable() && subs.is_empty() {
        return err("a library target must define at least one subroutine to export");
    }

    let mut lo = Lowerer {
        reg,
        strings: Vec::new(),
        body: String::new(),
        vars: HashMap::new(),
        used: BTreeSet::new(),
        ui_used: BTreeSet::new(),
        globals: HashMap::new(),
        allocas: Vec::new(),
        handles: HashMap::new(),
        component_types: HashMap::new(),
        tmp: 0,
        label: 0,
    };
    for g in m.globals() {
        lo.globals.insert(g.name.clone(), g.ty);
    }

    // Assign component handles BEFORE lowering subroutines: a handler may
    // address a component, and handles are compile-time constants derived from
    // creation order (ADR 0008), so they can be known up front.
    if let Some(form) = forms.first() {
        lo.map_components(form);
    }

    // Each subroutine becomes its own function.
    let mut functions = String::new();
    for sub in &subs {
        lo.body.clear();
        lo.vars.clear();
        lo.allocas.clear();
        lo.label = 0;
        for stmt in &sub.body {
            lo.stmt(stmt)?;
        }
        functions.push_str(&format!(
            "define void @{}() {{\nentry:\n{}{}  ret void\n}}\n\n",
            user_symbol(&sub.name),
            lo.allocas.join(""),
            lo.body
        ));
    }

    // A library has no entry point: it exports its subroutines and stops there.
    // The wrapper carries the plain name while the body keeps the mangled one,
    // so a host links against `greet` and internal calls still resolve.
    if !target.is_executable() {
        for sub in &subs {
            functions.push_str(&format!(
                "define void @{}() {{\nentry:\n  call void @{}()\n  ret void\n}}\n\n",
                sub.name,
                user_symbol(&sub.name)
            ));
        }
        // Module variables still need initialising, but a library has no moment
        // that is obviously "start-up". Exported explicitly so the host can say
        // when — an implicit constructor would run before the host is ready.
        lo.body.clear();
        lo.vars.clear();
        lo.allocas.clear();
        for g in m.globals() {
            let v = lo.eval(&g.value)?;
            lo.store_global(&g.name, &v);
        }
        let init = format!(
            "define void @{}_init() {{\nentry:\n{}{}  ret void\n}}\n\n",
            m.name,
            lo.allocas.join(""),
            lo.body
        );
        functions.push_str(&init);
        return Ok(lo.finish_library(&m.name, &functions));
    }

    // The entry function. Order matters: the form must be BUILT before any user
    // code runs, or `main` could address a component that does not exist yet
    // (a segfault that would only appear for modules having both). The event
    // loop starts last, after start-up code has had its say.
    lo.body.clear();
    lo.vars.clear();
    lo.allocas.clear();
    // Module variables are initialised before anything else can observe them.
    for g in m.globals() {
        let v = lo.eval(&g.value)?;
        lo.store_global(&g.name, &v);
    }
    if let Some(form) = forms.first() {
        lo.form_build(form)?;
    }
    if subs.iter().any(|s| s.name == "main") {
        writeln!(lo.body, "  call void @{}()", user_symbol("main")).unwrap();
    }
    if !forms.is_empty() {
        lo.form_run();
    }

    Ok(lo.finish(&m.name, &functions))
}

/// Symbol for a user subroutine. Prefixed so user names can never collide with
/// runtime symbols.
fn user_symbol(name: &str) -> String {
    format!("oe_user_{name}")
}

/// Symbol for a module variable. `internal` linkage, so it is not exported and
/// the name is dropped by `strip` in release builds (G8).
fn global_symbol(name: &str) -> String {
    format!("oe_g_{name}")
}

/// A lowered value: its slot type plus its LLVM operand (a literal or `%tN`).
#[derive(Clone)]
struct Val {
    ty: Ty,
    operand: String,
}

struct Lowerer<'a> {
    reg: &'a Registry,
    strings: Vec<String>,
    body: String,
    /// Local variables: name -> (alloca pointer, type). Every local is
    /// alloca-backed, `let` and `var` alike — one lowering path, and `opt`'s
    /// mem2reg reconstructs SSA for free when optimisation is enabled.
    vars: HashMap<String, (String, Ty)>,
    /// Module-level variables: name -> type. Storage is an LLVM global.
    globals: HashMap<String, Ty>,
    /// Allocas to emit at the top of the current function.
    allocas: Vec<String>,
    /// Runtime command symbols actually referenced (drives declarations).
    used: BTreeSet<String>,
    /// UI-interface symbols referenced (declared separately; see finish()).
    ui_used: BTreeSet<&'static str>,
    /// Component id -> its runtime widget handle.
    ///
    /// Handles are assigned by creation order, and creation order is fully
    /// static, so every id resolves to a compile-time integer constant. This is
    /// why component ids need no interning table and never reach the binary:
    /// `ok_button` simply compiles to `3` (ADR 0008).
    handles: HashMap<String, u64>,
    /// Component id -> component type name, for resolving property types.
    component_types: HashMap<String, String>,
    tmp: usize,
    /// Basic-block label counter. Labels must be unique within a function.
    label: usize,
}

impl Lowerer<'_> {
    fn fresh(&mut self) -> String {
        let t = format!("%t{}", self.tmp);
        self.tmp += 1;
        t
    }

    /// Emit a string constant and return an operand pointing at it.
    fn cstr(&mut self, text: &str) -> String {
        let id = self.strings.len();
        self.strings.push(text.to_string());
        let bytes = text.len() + 1;
        format!("getelementptr inbounds ([{bytes} x i8], ptr @.str{id}, i64 0, i64 0)")
    }

    /// Render a property value literal as the text the UI layer expects.
    /// Values are textual at the D10 boundary in v0 (see abi/openepl_ui.h).
    fn property_text(&self, e: &Expr) -> Result<String, LowerError> {
        Ok(match e {
            Expr::TextLit(s) => s.clone(),
            Expr::IntLit(v) => v.to_string(),
            Expr::DoubleLit(v) => format!("{v}"),
            Expr::BoolLit(b) => b.to_string(),
            _ => return err("component property values must be literals in v0.2"),
        })
    }

    /// Lower a form into the calls that build it at run time.
    /// Assign each component its compile-time handle constant. The root form is
    /// always 1 and children follow in declaration order, which is exactly the
    /// order `form_build` creates them in.
    fn map_components(&mut self, form: &openepl_ir::Form) {
        let mut next: u64 = 2;
        for child in &form.children {
            self.handles.insert(child.id.clone(), next);
            self.component_types
                .insert(child.id.clone(), child.type_name.clone());
            next += 1;
        }
    }

    fn form_build(&mut self, form: &openepl_ir::Form) -> Result<(), LowerError> {
        // Window geometry/title come from the form's own properties.
        let mut title = "OpenEPL Application".to_string();
        let (mut width, mut height) = (800i64, 600i64);
        for (name, value) in &form.properties {
            match name.as_str() {
                "title" => title = self.property_text(value)?,
                "width" => width = self.property_text(value)?.parse().unwrap_or(800),
                "height" => height = self.property_text(value)?.parse().unwrap_or(600),
                _ => {}
            }
        }

        let title_op = self.cstr(&title);
        self.ui_used.insert("oe_ui_init");
        writeln!(
            self.body,
            "  call i32 @oe_ui_init(ptr {title_op}, i32 {width}, i32 {height})"
        )
        .unwrap();

        // The root is always handle 1; children follow in creation order.
        self.ui_used.insert("oe_ui_root");
        let root_tmp = self.fresh();
        writeln!(self.body, "  {root_tmp} = call i64 @oe_ui_root()").unwrap();
        let root = "1".to_string();

        // Root properties (skip the window-level ones already consumed).
        for (name, value) in &form.properties {
            if matches!(name.as_str(), "title" | "width" | "height") {
                continue;
            }
            self.set_property(&root, name, &self.property_text(value)?);
        }
        // The accessible name is user-facing TEXT (the title), never the form's
        // identifier — identifiers must not reach the binary (G8).
        self.a11y(&root, form_role(), &title);
        self.bind_handlers(&root, &form.handlers);

        // Children.
        for child in &form.children {
            let desc = self
                .reg
                .component(&child.type_name)
                .ok_or_else(|| LowerError {
                    msg: format!("unknown component type `{}`", child.type_name),
                })?;
            let role = desc.a11y_role;
            let type_op = self.cstr(&child.type_name);
            self.ui_used.insert("oe_ui_create");
            let handle = self.fresh();
            writeln!(
                self.body,
                "  {handle} = call i64 @oe_ui_create(i64 {root}, ptr {type_op})"
            )
            .unwrap();

            for (name, value) in &child.properties {
                let text = self.property_text(value)?;
                self.set_property(&handle, name, &text);
            }
            // The accessible name comes from user-facing text. If a component
            // has none, we emit the role only rather than falling back to the
            // instance id: ids are compile-time and must not ship (G8).
            // A future designer should prompt for an explicit accessible name
            // when a component has no text (D16).
            match child.properties.iter().find(|(n, _)| n == "text") {
                Some((_, v)) => {
                    let name = self.property_text(v)?;
                    self.a11y(&handle, role, &name);
                }
                None => self.a11y_role_only(&handle, role),
            }
            self.bind_handlers(&handle, &child.handlers);
        }

        Ok(())
    }

    /// Start the event loop and tear down. Emitted after start-up code.
    fn form_run(&mut self) {
        self.ui_used.insert("oe_ui_run");
        let rc = self.fresh();
        writeln!(self.body, "  {rc} = call i32 @oe_ui_run()").unwrap();
        self.ui_used.insert("oe_ui_shutdown");
        writeln!(self.body, "  call void @oe_ui_shutdown()").unwrap();
    }

    fn set_property(&mut self, handle: &str, name: &str, value: &str) {
        let n = self.cstr(name);
        let v = self.cstr(value);
        self.ui_used.insert("oe_ui_set");
        writeln!(
            self.body,
            "  call i32 @oe_ui_set(i64 {handle}, ptr {n}, ptr {v})"
        )
        .unwrap();
    }

    /// Record the a11y role with no accessible name (see the G8 note above).
    fn a11y_role_only(&mut self, handle: &str, role: i32) {
        self.ui_used.insert("oe_ui_set_a11y");
        writeln!(
            self.body,
            "  call i32 @oe_ui_set_a11y(i64 {handle}, i32 {role}, ptr null)"
        )
        .unwrap();
    }

    fn a11y(&mut self, handle: &str, role: i32, name: &str) {
        let n = self.cstr(name);
        self.ui_used.insert("oe_ui_set_a11y");
        writeln!(
            self.body,
            "  call i32 @oe_ui_set_a11y(i64 {handle}, i32 {role}, ptr {n})"
        )
        .unwrap();
    }

    /// Bind events to handler FUNCTION POINTERS (never names — G8).
    fn bind_handlers(&mut self, handle: &str, handlers: &[(String, String)]) {
        for (event, sub) in handlers {
            let ev = self.cstr(event);
            self.ui_used.insert("oe_ui_on");
            writeln!(
                self.body,
                "  call i32 @oe_ui_on(i64 {handle}, ptr {ev}, ptr @{})",
                user_symbol(sub)
            )
            .unwrap();
        }
    }

    fn fresh_label(&mut self, kind: &str) -> String {
        let l = format!("bb_{kind}_{}", self.label);
        self.label += 1;
        l
    }

    /// Evaluate a condition and branch. LLVM needs an `i1`, and bools are held
    /// as `i32`, so compare against zero at the branch.
    fn branch_on(&mut self, cond: &Expr, yes: &str, no: &str) -> Result<(), LowerError> {
        let v = self.eval(cond)?;
        if v.ty != Ty::Bool {
            return err(format!(
                "condition must be a truth value, got {}",
                v.ty.as_str()
            ));
        }
        let t = self.fresh();
        writeln!(self.body, "  {t} = icmp ne i32 {}, 0", v.operand).unwrap();
        writeln!(self.body, "  br i1 {t}, label %{yes}, label %{no}").unwrap();
        Ok(())
    }

    fn block(&mut self, stmts: &[openepl_ir::Stmt]) -> Result<(), LowerError> {
        for s in stmts {
            self.stmt(s)?;
        }
        Ok(())
    }

    /// Reserve a stack slot, emitted at the top of the function.
    fn alloca(&mut self, ty: Ty) -> String {
        let slot = format!("%v{}", self.allocas.len());
        self.allocas
            .push(format!("  {slot} = alloca {}\n", llvm_ty(ty)));
        slot
    }

    fn store_global(&mut self, name: &str, v: &Val) {
        writeln!(
            self.body,
            "  store {} {}, ptr @{}",
            llvm_ty(v.ty),
            v.operand,
            global_symbol(name)
        )
        .unwrap();
    }

    fn stmt(&mut self, s: &openepl_ir::Stmt) -> Result<(), LowerError> {
        use openepl_ir::StmtKind;
        match &s.kind {
            StmtKind::Let {
                name,
                ty,
                value,
                mutable: _,
            } => {
                let v = self.eval(value)?;
                if v.ty != *ty {
                    return err(format!(
                        "type mismatch in `let {name}`: declared {}, expression is {}",
                        ty.as_str(),
                        v.ty.as_str()
                    ));
                }
                let slot = self.alloca(*ty);
                writeln!(
                    self.body,
                    "  store {} {}, ptr {slot}",
                    llvm_ty(*ty),
                    v.operand
                )
                .unwrap();
                self.vars.insert(name.clone(), (slot, *ty));
                Ok(())
            }
            StmtKind::Assign { name, value } => {
                let v = self.eval(value)?;
                if let Some((slot, ty)) = self.vars.get(name).cloned() {
                    if v.ty != ty {
                        return err(format!("cannot assign {} to `{name}`", v.ty.as_str()));
                    }
                    writeln!(
                        self.body,
                        "  store {} {}, ptr {slot}",
                        llvm_ty(ty),
                        v.operand
                    )
                    .unwrap();
                    Ok(())
                } else if self.globals.contains_key(name) {
                    self.store_global(name, &v);
                    Ok(())
                } else {
                    err(format!("assignment to undefined variable `{name}`"))
                }
            }
            StmtKind::Call { cmd, args } => {
                self.eval_call(cmd, args)?; // any return value discarded
                Ok(())
            }
            StmtKind::If { arms, otherwise } => {
                let done = self.fresh_label("endif");
                for (cond, body) in arms {
                    let then = self.fresh_label("then");
                    let next = self.fresh_label("elif");
                    self.branch_on(cond, &then, &next)?;
                    writeln!(self.body, "{then}:").unwrap();
                    self.block(body)?;
                    writeln!(self.body, "  br label %{done}").unwrap();
                    writeln!(self.body, "{next}:").unwrap();
                }
                if let Some(body) = otherwise {
                    self.block(body)?;
                }
                writeln!(self.body, "  br label %{done}").unwrap();
                writeln!(self.body, "{done}:").unwrap();
                Ok(())
            }
            StmtKind::While { cond, body } => {
                let head = self.fresh_label("while");
                let inner = self.fresh_label("do");
                let done = self.fresh_label("done");
                writeln!(self.body, "  br label %{head}").unwrap();
                writeln!(self.body, "{head}:").unwrap();
                self.branch_on(cond, &inner, &done)?;
                writeln!(self.body, "{inner}:").unwrap();
                self.block(body)?;
                writeln!(self.body, "  br label %{head}").unwrap();
                writeln!(self.body, "{done}:").unwrap();
                Ok(())
            }
            StmtKind::SetProperty {
                component,
                property,
                value,
            } => {
                let handle = self.handle_of(component)?;
                let v = self.eval(value)?;
                // The D10 boundary takes textual values, so convert first.
                let text = self.value_as_text(&v)?;
                let n = self.cstr(property);
                self.ui_used.insert("oe_ui_set");
                writeln!(
                    self.body,
                    "  call i32 @oe_ui_set(i64 {handle}, ptr {n}, ptr {text})"
                )
                .unwrap();
                Ok(())
            }
        }
    }

    /// The compile-time handle constant for a component id (ADR 0008).
    fn handle_of(&self, id: &str) -> Result<u64, LowerError> {
        self.handles.get(id).copied().ok_or_else(|| LowerError {
            msg: format!("unknown component `{id}`"),
        })
    }

    /// Render a value as a `ptr` to text, converting numbers via the runtime.
    fn value_as_text(&mut self, v: &Val) -> Result<String, LowerError> {
        match v.ty {
            Ty::Text => Ok(v.operand.clone()),
            Ty::Bool => err("cannot use a truth value where text is expected"),
            Ty::Int | Ty::Int64 | Ty::Double => {
                let sym = match v.ty {
                    Ty::Int => "oe_int_to_text",
                    Ty::Int64 => "oe_int64_to_text",
                    _ => "oe_double_to_text",
                };
                let converted = self.call_symbol_1(sym, v)?;
                Ok(converted)
            }
        }
    }

    /// Compare two text values by content via the runtime, yielding i32 0/1.
    fn call_text_eq(&mut self, a: &Val, b: &Val) -> Result<String, LowerError> {
        let argv = self.fresh();
        writeln!(self.body, "  {argv} = alloca [2 x %Slot]").unwrap();
        for (i, v) in [a, b].iter().enumerate() {
            let raw = self.emit_arg_i64(v);
            let slot = self.fresh();
            writeln!(
                self.body,
                "  {slot} = getelementptr [2 x %Slot], ptr {argv}, i64 0, i64 {i}"
            )
            .unwrap();
            let tagp = self.fresh();
            writeln!(
                self.body,
                "  {tagp} = getelementptr %Slot, ptr {slot}, i32 0, i32 0"
            )
            .unwrap();
            writeln!(self.body, "  store i32 {}, ptr {tagp}", Ty::Text.sdt_tag()).unwrap();
            let valp = self.fresh();
            writeln!(
                self.body,
                "  {valp} = getelementptr %Slot, ptr {slot}, i32 0, i32 2"
            )
            .unwrap();
            writeln!(self.body, "  store i64 {raw}, ptr {valp}").unwrap();
        }
        let base = self.fresh();
        writeln!(
            self.body,
            "  {base} = getelementptr [2 x %Slot], ptr {argv}, i64 0, i64 0"
        )
        .unwrap();
        let ret = self.fresh();
        writeln!(self.body, "  {ret} = alloca %Slot").unwrap();
        self.used.insert("oe_text_eq".to_string());
        writeln!(
            self.body,
            "  call void @oe_text_eq(ptr {ret}, i32 2, ptr {base})"
        )
        .unwrap();
        let valp = self.fresh();
        writeln!(
            self.body,
            "  {valp} = getelementptr %Slot, ptr {ret}, i32 0, i32 2"
        )
        .unwrap();
        let raw = self.fresh();
        writeln!(self.body, "  {raw} = load i64, ptr {valp}").unwrap();
        let t = self.fresh();
        writeln!(self.body, "  {t} = trunc i64 {raw} to i32").unwrap();
        Ok(t)
    }

    /// Call a one-argument slot-ABI runtime command and return its text result.
    fn call_symbol_1(&mut self, symbol: &str, arg: &Val) -> Result<String, LowerError> {
        let raw = self.emit_arg_i64(arg);
        let argv = self.fresh();
        writeln!(self.body, "  {argv} = alloca [1 x %Slot]").unwrap();
        let slot = self.fresh();
        writeln!(
            self.body,
            "  {slot} = getelementptr [1 x %Slot], ptr {argv}, i64 0, i64 0"
        )
        .unwrap();
        let tagp = self.fresh();
        writeln!(
            self.body,
            "  {tagp} = getelementptr %Slot, ptr {slot}, i32 0, i32 0"
        )
        .unwrap();
        writeln!(self.body, "  store i32 {}, ptr {tagp}", arg.ty.sdt_tag()).unwrap();
        let valp = self.fresh();
        writeln!(
            self.body,
            "  {valp} = getelementptr %Slot, ptr {slot}, i32 0, i32 2"
        )
        .unwrap();
        writeln!(self.body, "  store i64 {raw}, ptr {valp}").unwrap();
        let ret = self.fresh();
        writeln!(self.body, "  {ret} = alloca %Slot").unwrap();
        self.used.insert(symbol.to_string());
        writeln!(
            self.body,
            "  call void @{symbol}(ptr {ret}, i32 1, ptr {slot})"
        )
        .unwrap();
        let rvalp = self.fresh();
        writeln!(
            self.body,
            "  {rvalp} = getelementptr %Slot, ptr {ret}, i32 0, i32 2"
        )
        .unwrap();
        let rraw = self.fresh();
        writeln!(self.body, "  {rraw} = load i64, ptr {rvalp}").unwrap();
        Ok(self.emit_ret_from_i64(Ty::Text, &rraw))
    }

    fn eval(&mut self, e: &Expr) -> Result<Val, LowerError> {
        match e {
            Expr::IntLit(v) => {
                if let Ok(v32) = i32::try_from(*v) {
                    Ok(Val {
                        ty: Ty::Int,
                        operand: v32.to_string(),
                    })
                } else {
                    Ok(Val {
                        ty: Ty::Int64,
                        operand: v.to_string(),
                    })
                }
            }
            Expr::DoubleLit(v) => Ok(Val {
                ty: Ty::Double,
                operand: format!("0x{:016X}", v.to_bits()),
            }),
            Expr::TextLit(s) => {
                let id = self.strings.len();
                self.strings.push(s.clone());
                let bytes = s.len() + 1;
                Ok(Val {
                    ty: Ty::Text,
                    operand: format!(
                        "getelementptr inbounds ([{bytes} x i8], ptr @.str{id}, i64 0, i64 0)"
                    ),
                })
            }
            Expr::Var(name) => {
                if let Some((slot, ty)) = self.vars.get(name).cloned() {
                    let t = self.fresh();
                    writeln!(self.body, "  {t} = load {}, ptr {slot}", llvm_ty(ty)).unwrap();
                    Ok(Val { ty, operand: t })
                } else if let Some(ty) = self.globals.get(name).copied() {
                    let t = self.fresh();
                    writeln!(
                        self.body,
                        "  {t} = load {}, ptr @{}",
                        llvm_ty(ty),
                        global_symbol(name)
                    )
                    .unwrap();
                    Ok(Val { ty, operand: t })
                } else {
                    Err(LowerError {
                        msg: format!("use of undefined variable `{name}`"),
                    })
                }
            }
            Expr::Bin(op, l, r) => {
                let lv = self.eval(l)?;
                let rv = self.eval(r)?;
                if lv.ty != rv.ty || !lv.ty.is_numeric() {
                    return err("arithmetic requires matching numeric operands");
                }
                let opcode = match (op, lv.ty) {
                    (BinOp::Add, Ty::Double) => "fadd",
                    (BinOp::Sub, Ty::Double) => "fsub",
                    (BinOp::Mul, Ty::Double) => "fmul",
                    (BinOp::Div, Ty::Double) => "fdiv",
                    (BinOp::Add, _) => "add",
                    (BinOp::Sub, _) => "sub",
                    (BinOp::Mul, _) => "mul",
                    (BinOp::Div, _) => "sdiv",
                };
                let t = self.fresh();
                writeln!(
                    self.body,
                    "  {t} = {opcode} {} {}, {}",
                    llvm_ty(lv.ty),
                    lv.operand,
                    rv.operand
                )
                .unwrap();
                Ok(Val {
                    ty: lv.ty,
                    operand: t,
                })
            }
            Expr::Call { cmd, args } => {
                let v = self.eval_call(cmd, args)?;
                v.ok_or_else(|| LowerError {
                    msg: format!("command `{cmd}` returns nothing and cannot be used as a value"),
                })
            }
            Expr::BoolLit(b) => Ok(Val {
                ty: Ty::Bool,
                operand: (*b as i32).to_string(),
            }),
            Expr::Not(e) => {
                let v = self.eval(e)?;
                let t = self.fresh();
                writeln!(self.body, "  {t} = xor i32 {}, 1", v.operand).unwrap();
                Ok(Val {
                    ty: Ty::Bool,
                    operand: t,
                })
            }
            Expr::Cmp(op, l, r) => {
                let lv = self.eval(l)?;
                let rv = self.eval(r)?;
                if lv.ty == Ty::Text {
                    // Text comparison must compare CONTENT, not pointers.
                    let eq = self.call_text_eq(&lv, &rv)?;
                    return match op {
                        CmpOp::Eq => Ok(Val {
                            ty: Ty::Bool,
                            operand: eq,
                        }),
                        CmpOp::Ne => {
                            let t = self.fresh();
                            writeln!(self.body, "  {t} = xor i32 {eq}, 1").unwrap();
                            Ok(Val {
                                ty: Ty::Bool,
                                operand: t,
                            })
                        }
                        _ => err("text values support only `=` and `<>`"),
                    };
                }
                let pred = match (op, lv.ty) {
                    (CmpOp::Eq, Ty::Double) => "fcmp oeq",
                    (CmpOp::Ne, Ty::Double) => "fcmp one",
                    (CmpOp::Lt, Ty::Double) => "fcmp olt",
                    (CmpOp::Le, Ty::Double) => "fcmp ole",
                    (CmpOp::Gt, Ty::Double) => "fcmp ogt",
                    (CmpOp::Ge, Ty::Double) => "fcmp oge",
                    (CmpOp::Eq, _) => "icmp eq",
                    (CmpOp::Ne, _) => "icmp ne",
                    (CmpOp::Lt, _) => "icmp slt",
                    (CmpOp::Le, _) => "icmp sle",
                    (CmpOp::Gt, _) => "icmp sgt",
                    (CmpOp::Ge, _) => "icmp sge",
                };
                let bit = self.fresh();
                writeln!(
                    self.body,
                    "  {bit} = {pred} {} {}, {}",
                    llvm_ty(lv.ty),
                    lv.operand,
                    rv.operand
                )
                .unwrap();
                let t = self.fresh();
                writeln!(self.body, "  {t} = zext i1 {bit} to i32").unwrap();
                Ok(Val {
                    ty: Ty::Bool,
                    operand: t,
                })
            }
            Expr::Logical(op, l, r) => {
                // Short-circuit: the right side is evaluated only when needed,
                // so `x > 0 and 100 / x > 2` is safe.
                let slot = self.alloca(Ty::Bool);
                let rhs_label = self.fresh_label("rhs");
                let done = self.fresh_label("logic");
                let lv = self.eval(l)?;
                writeln!(self.body, "  store i32 {}, ptr {slot}", lv.operand).unwrap();
                let c = self.fresh();
                writeln!(self.body, "  {c} = icmp ne i32 {}, 0", lv.operand).unwrap();
                match op {
                    LogicalOp::And => {
                        writeln!(self.body, "  br i1 {c}, label %{rhs_label}, label %{done}")
                            .unwrap()
                    }
                    LogicalOp::Or => {
                        writeln!(self.body, "  br i1 {c}, label %{done}, label %{rhs_label}")
                            .unwrap()
                    }
                }
                writeln!(self.body, "{rhs_label}:").unwrap();
                let rv = self.eval(r)?;
                writeln!(self.body, "  store i32 {}, ptr {slot}", rv.operand).unwrap();
                writeln!(self.body, "  br label %{done}").unwrap();
                writeln!(self.body, "{done}:").unwrap();
                let t = self.fresh();
                writeln!(self.body, "  {t} = load i32, ptr {slot}").unwrap();
                Ok(Val {
                    ty: Ty::Bool,
                    operand: t,
                })
            }
            Expr::GetProperty {
                component,
                property,
            } => {
                let handle = self.handle_of(component)?;
                let n = self.cstr(property);
                let ty = self.property_ty(component, property)?;
                match ty {
                    Ty::Int => {
                        self.ui_used.insert("oe_ui_get_int");
                        let t = self.fresh();
                        writeln!(
                            self.body,
                            "  {t} = call i32 @oe_ui_get_int(i64 {handle}, ptr {n})"
                        )
                        .unwrap();
                        Ok(Val {
                            ty: Ty::Int,
                            operand: t,
                        })
                    }
                    _ => {
                        self.ui_used.insert("oe_ui_get");
                        let t = self.fresh();
                        writeln!(
                            self.body,
                            "  {t} = call ptr @oe_ui_get(i64 {handle}, ptr {n})"
                        )
                        .unwrap();
                        Ok(Val {
                            ty: Ty::Text,
                            operand: t,
                        })
                    }
                }
            }
        }
    }

    /// The declared type of a component property, from the introspected
    /// descriptor (the validator has already proven it exists).
    fn property_ty(&self, component: &str, property: &str) -> Result<Ty, LowerError> {
        let type_name = self
            .component_types
            .get(component)
            .ok_or_else(|| LowerError {
                msg: format!("unknown component `{component}`"),
            })?;
        self.reg
            .component(type_name)
            .and_then(|d| d.property(property))
            .map(|p| p.ty)
            .ok_or_else(|| LowerError {
                msg: format!("`{type_name}` has no property `{property}`"),
            })
    }

    /// Reinterpret a value's operand as the raw `i64` stored in a slot's value
    /// field; returns the operand holding the i64.
    fn emit_arg_i64(&mut self, v: &Val) -> String {
        match v.ty {
            Ty::Int64 => v.operand.clone(),
            Ty::Int | Ty::Bool => {
                let t = self.fresh();
                writeln!(self.body, "  {t} = sext i32 {} to i64", v.operand).unwrap();
                t
            }
            Ty::Double => {
                let t = self.fresh();
                writeln!(self.body, "  {t} = bitcast double {} to i64", v.operand).unwrap();
                t
            }
            Ty::Text => {
                let t = self.fresh();
                writeln!(self.body, "  {t} = ptrtoint ptr {} to i64", v.operand).unwrap();
                t
            }
        }
    }

    /// Reinterpret an `i64` loaded from a slot's value field back to `ty`.
    fn emit_ret_from_i64(&mut self, ty: Ty, raw: &str) -> String {
        match ty {
            Ty::Int64 => raw.to_string(),
            Ty::Int | Ty::Bool => {
                let t = self.fresh();
                writeln!(self.body, "  {t} = trunc i64 {raw} to i32").unwrap();
                t
            }
            Ty::Double => {
                let t = self.fresh();
                writeln!(self.body, "  {t} = bitcast i64 {raw} to double").unwrap();
                t
            }
            Ty::Text => {
                let t = self.fresh();
                writeln!(self.body, "  {t} = inttoptr i64 {raw} to ptr").unwrap();
                t
            }
        }
    }

    /// Lower a command call via the slot ABI; returns the result if non-void.
    fn eval_call(&mut self, cmd: &str, args: &[Expr]) -> Result<Option<Val>, LowerError> {
        let command = self.reg.get(cmd).ok_or_else(|| LowerError {
            msg: format!("unknown command `{cmd}`"),
        })?;
        let sig = command.sig.clone();
        let symbol = command.symbol.clone();

        if args.len() != sig.params.len() {
            return err(format!(
                "command `{cmd}` expects {} argument(s), got {}",
                sig.params.len(),
                args.len()
            ));
        }

        // Lower and type-check each argument first (may emit arithmetic).
        let mut arg_vals = Vec::new();
        for (i, a) in args.iter().enumerate() {
            let v = self.eval(a)?;
            if v.ty != sig.params[i] {
                return err(format!(
                    "command `{cmd}` argument {} expects {}, got {}",
                    i + 1,
                    sig.params[i].as_str(),
                    v.ty.as_str()
                ));
            }
            arg_vals.push(v);
        }

        let argc = arg_vals.len();
        // Return slot (always allocated; ignored for void commands).
        let ret_slot = self.fresh();
        writeln!(self.body, "  {ret_slot} = alloca %Slot").unwrap();

        // argv array + per-argument stores.
        let argv_base = if argc > 0 {
            let argv = self.fresh();
            writeln!(self.body, "  {argv} = alloca [{argc} x %Slot]").unwrap();
            for (i, v) in arg_vals.iter().enumerate() {
                let raw = self.emit_arg_i64(v);
                let slot = self.fresh();
                writeln!(
                    self.body,
                    "  {slot} = getelementptr [{argc} x %Slot], ptr {argv}, i64 0, i64 {i}"
                )
                .unwrap();
                let tagp = self.fresh();
                writeln!(
                    self.body,
                    "  {tagp} = getelementptr %Slot, ptr {slot}, i32 0, i32 0"
                )
                .unwrap();
                writeln!(self.body, "  store i32 {}, ptr {tagp}", v.ty.sdt_tag()).unwrap();
                let valp = self.fresh();
                writeln!(
                    self.body,
                    "  {valp} = getelementptr %Slot, ptr {slot}, i32 0, i32 2"
                )
                .unwrap();
                writeln!(self.body, "  store i64 {raw}, ptr {valp}").unwrap();
            }
            let base = self.fresh();
            writeln!(
                self.body,
                "  {base} = getelementptr [{argc} x %Slot], ptr {argv}, i64 0, i64 0"
            )
            .unwrap();
            base
        } else {
            "null".to_string()
        };

        self.used.insert(symbol.clone());
        writeln!(
            self.body,
            "  call void @{symbol}(ptr {ret_slot}, i32 {argc}, ptr {argv_base})"
        )
        .unwrap();

        match sig.ret {
            None => Ok(None),
            Some(rt) => {
                let valp = self.fresh();
                writeln!(
                    self.body,
                    "  {valp} = getelementptr %Slot, ptr {ret_slot}, i32 0, i32 2"
                )
                .unwrap();
                let raw = self.fresh();
                writeln!(self.body, "  {raw} = load i64, ptr {valp}").unwrap();
                let res = self.emit_ret_from_i64(rt, &raw);
                Ok(Some(Val {
                    ty: rt,
                    operand: res,
                }))
            }
        }
    }

    /// Everything except the entry point. A library stops here.
    fn finish_library(self, module_name: &str, functions: &str) -> String {
        self.finish_with(module_name, functions, false)
    }

    fn finish(self, module_name: &str, functions: &str) -> String {
        self.finish_with(module_name, functions, true)
    }

    fn finish_with(self, module_name: &str, functions: &str, entry: bool) -> String {
        let mut out = String::new();
        writeln!(
            out,
            "; OpenEPL-generated LLVM IR — module `{module_name}` (Phase 2, slot ABI)"
        )
        .unwrap();
        writeln!(out, "; Do not edit; regenerate from the .oir source.\n").unwrap();

        // The slot type mirrors OpenEPL_Slot (abi/openepl_abi.h): {tag, pad, value}.
        writeln!(out, "%Slot = type {{ i32, i32, i64 }}\n").unwrap();

        // Module variables. Zero-initialised here; their declared initializers
        // run at entry, so a `var` may call a command (`var t: int64 = now()`).
        let mut gnames: Vec<(&String, &Ty)> = self.globals.iter().collect();
        gnames.sort_by(|a, b| a.0.cmp(b.0));
        for (name, ty) in gnames {
            let zero = match ty {
                Ty::Double => "0.0",
                Ty::Text => "null",
                _ => "0",
            };
            writeln!(
                out,
                "@{} = internal global {} {zero}",
                global_symbol(name),
                llvm_ty(*ty)
            )
            .unwrap();
        }
        if !self.globals.is_empty() {
            out.push('\n');
        }

        for (id, s) in self.strings.iter().enumerate() {
            let encoded = encode_llvm_string(s);
            let bytes = s.len() + 1;
            writeln!(
                out,
                "@.str{id} = private unnamed_addr constant [{bytes} x i8] c\"{encoded}\\00\""
            )
            .unwrap();
        }
        if !self.strings.is_empty() {
            out.push('\n');
        }

        // Every command shares the one slot-ABI signature.
        for sym in &self.used {
            writeln!(out, "declare void @{sym}(ptr, i32, ptr)").unwrap();
        }
        if !self.used.is_empty() {
            out.push('\n');
        }

        // UI interface declarations (abi/openepl_ui.h), only when referenced.
        for sym in &self.ui_used {
            let decl = match *sym {
                "oe_ui_init" => "declare i32 @oe_ui_init(ptr, i32, i32)",
                "oe_ui_shutdown" => "declare void @oe_ui_shutdown()",
                "oe_ui_root" => "declare i64 @oe_ui_root()",
                "oe_ui_create" => "declare i64 @oe_ui_create(i64, ptr)",
                "oe_ui_set" => "declare i32 @oe_ui_set(i64, ptr, ptr)",
                "oe_ui_get" => "declare ptr @oe_ui_get(i64, ptr)",
                "oe_ui_get_int" => "declare i32 @oe_ui_get_int(i64, ptr)",
                "oe_ui_on" => "declare i32 @oe_ui_on(i64, ptr, ptr)",
                "oe_ui_set_a11y" => "declare i32 @oe_ui_set_a11y(i64, i32, ptr)",
                "oe_ui_run" => "declare i32 @oe_ui_run()",
                other => panic!("undeclared UI symbol {other}"),
            };
            writeln!(out, "{decl}").unwrap();
        }
        if !self.ui_used.is_empty() {
            out.push('\n');
        }

        out.push_str(functions);

        if entry {
            writeln!(out, "define i32 @ECodeStart() {{").unwrap();
            writeln!(out, "entry:").unwrap();
            out.push_str(&self.allocas.join(""));
            out.push_str(&self.body);
            writeln!(out, "  ret i32 0").unwrap();
            writeln!(out, "}}").unwrap();
        }
        out
    }
}

fn encode_llvm_string(s: &str) -> String {
    let mut out = String::new();
    for &b in s.as_bytes() {
        match b {
            b'"' | b'\\' => write!(out, "\\{:02X}", b).unwrap(),
            0x20..=0x7E => out.push(b as char),
            _ => write!(out, "\\{:02X}", b).unwrap(),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use openepl_ir::parse;

    fn lower(src: &str) -> Result<String, LowerError> {
        let m = parse(src).unwrap();
        lower_module(&m, &Registry::core())
    }

    #[test]
    fn emits_slot_type_and_abi_call() {
        let ll = lower("module m\nsub main\n  call print_int(42)\nend\n").unwrap();
        assert!(ll.contains("%Slot = type { i32, i32, i64 }"));
        assert!(ll.contains("declare void @oe_print_int(ptr, i32, ptr)"));
        assert!(ll.contains("call void @oe_print_int(ptr %"));
        assert!(ll.contains("store i32 3, ptr")); // SDT_INT tag
    }

    #[test]
    fn call_expr_reads_return_slot() {
        let ll =
            lower("module m\nsub main\n  let n: int = length(\"hi\")\n  call print_int(n)\nend\n")
                .unwrap();
        assert!(ll.contains("call void @oe_length(ptr"));
        assert!(ll.contains("load i64, ptr")); // reads the return slot
        assert!(ll.contains("trunc i64")); // int result reinterpret
        assert!(ll.contains("ptrtoint ptr")); // text arg reinterpret
    }

    #[test]
    fn double_roundtrips_through_slot() {
        let ll =
            lower("module m\nsub main\n  let r: double = sqrt(2.0)\n  call print_double(r)\nend\n")
                .unwrap();
        assert!(ll.contains("bitcast double 0x")); // arg store
        assert!(ll.contains("bitcast i64")); // return reinterpret
    }

    #[test]
    fn only_used_commands_declared() {
        let ll = lower("module m\nsub main\n  call print_int(1)\nend\n").unwrap();
        assert!(ll.contains("declare void @oe_print_int(ptr, i32, ptr)"));
        assert!(!ll.contains("oe_sqrt"));
    }
}
