//! Command registry — the Phase 1 stand-in for the support-library ABI.
//!
//! Maps a surface command name to its `Signature` and the runtime symbol the
//! backend emits a call to.  In Phase 2 this table is replaced by signatures
//! loaded from `openepl_get_lib_info`; keeping it in one place now
//! means the validator and the backend agree on exactly one source of truth.

use std::collections::HashMap;

use crate::{Module, Signature, Ty};

/// A component property, as declared in a library's `ComponentDesc`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PropertyDesc {
    pub name: String,
    pub ty: Ty,
}

/// A visual component type contributed by a support library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentDesc {
    pub name: String,
    /// Accessibility role (`OE_ROLE_*`) — carried from the descriptor so a11y
    /// data exists from the start.
    pub a11y_role: i32,
    pub properties: Vec<PropertyDesc>,
    pub events: Vec<String>,
}

impl ComponentDesc {
    pub fn property(&self, name: &str) -> Option<&PropertyDesc> {
        self.properties.iter().find(|p| p.name == name)
    }
    pub fn has_event(&self, name: &str) -> bool {
        self.events.iter().any(|e| e == name)
    }
}

/// One registered command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub sig: Signature,
    /// The runtime function symbol the backend emits a call to (see `runtime/`).
    pub symbol: String,
}

/// The set of commands the compiler knows how to lower.  Built either from the
/// hard-coded `core()` set (used by unit tests) or from `LibInfo` metadata read
/// out of a support library at build time (the CLI's authoritative path).
#[derive(Debug, Clone, Default)]
pub struct Registry {
    map: HashMap<String, Command>,
    components: HashMap<String, ComponentDesc>,
    /// Subroutines defined in the module being compiled, by name.
    ///
    /// They live beside the commands rather than among them so a user sub can
    /// never quietly take a library command's name: `register_subs` reports the
    /// collision instead of overwriting. Lookup order is commands first, then
    /// these, and `get` still means "library command" for every existing caller.
    subs: HashMap<String, Signature>,
}

impl Registry {
    pub fn new() -> Registry {
        Registry {
            map: HashMap::new(),
            components: HashMap::new(),
            subs: HashMap::new(),
        }
    }

    /// Record every subroutine in `m` as callable, returning the names that
    /// collide with a library command (the caller reports them — the validator
    /// has the line numbers).
    pub fn register_subs(&mut self, m: &Module) -> Vec<String> {
        let mut collisions = Vec::new();
        for sub in m.subs() {
            if self.map.contains_key(&sub.name) {
                collisions.push(sub.name.clone());
                continue;
            }
            self.subs.insert(sub.name.clone(), sub.signature());
        }
        collisions
    }

    /// The signature of a user subroutine, if `name` is one.
    pub fn sub(&self, name: &str) -> Option<&Signature> {
        self.subs.get(name)
    }

    /// Whether `name` names a user subroutine.
    pub fn is_sub(&self, name: &str) -> bool {
        self.subs.contains_key(name)
    }

    /// User subroutine names, for diagnostics.
    pub fn sub_names(&self) -> impl Iterator<Item = &str> {
        self.subs.keys().map(|s| s.as_str())
    }

    /// Look up a component type by its surface name.
    pub fn component(&self, name: &str) -> Option<&ComponentDesc> {
        self.components.get(name)
    }

    /// Register a component type; `false` if the name was already taken.
    pub fn insert_component(&mut self, desc: ComponentDesc) -> bool {
        if self.components.contains_key(&desc.name) {
            return false;
        }
        self.components.insert(desc.name.clone(), desc);
        true
    }

    /// Known component type names, for diagnostics.
    pub fn component_names(&self) -> impl Iterator<Item = &str> {
        self.components.keys().map(|s| s.as_str())
    }

    pub fn get(&self, name: &str) -> Option<&Command> {
        self.map.get(name)
    }

    /// Number of registered commands.
    pub fn len(&self) -> usize {
        self.map.len()
    }
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Command names, for diagnostics / drift checks.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.map.keys().map(|s| s.as_str())
    }

    /// Iterate `(name, command)` pairs.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Command)> {
        self.map.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Register a command; returns `false` if the name was already taken (a
    /// duplicate across libraries — the caller reports it).
    pub fn insert(
        &mut self,
        name: impl Into<String>,
        sig: Signature,
        symbol: impl Into<String>,
    ) -> bool {
        let name = name.into();
        if self.map.contains_key(&name) {
            return false;
        }
        self.map.insert(
            name,
            Command {
                sig,
                symbol: symbol.into(),
            },
        );
        true
    }

    /// The built-in core command set (math / conversions / text / datetime / io).
    pub fn core() -> Registry {
        use Ty::*;
        let mut r = Registry::new();
        {
            let mut cmd =
                |name: &'static str, symbol: &'static str, params: &[Ty], ret: Option<Ty>| {
                    r.insert(
                        name,
                        Signature {
                            params: params.to_vec(),
                            ret,
                        },
                        symbol,
                    );
                };

            // --- I/O (void) --------------------------------------------------
            cmd("print_int", "oe_print_int", &[Int], None);
            cmd("print_int64", "oe_print_int64", &[Int64], None);
            cmd("print_double", "oe_print_double", &[Double], None);
            cmd("print_text", "oe_print_text", &[Text], None);
            cmd("read_line", "oe_read_line", &[], Some(Text));
            cmd("input_ended", "oe_input_ended", &[], Some(Bool));
            cmd("ask", "oe_ask", &[Text], Some(Text));

            // --- Errors ------------------------------------------------------
            cmd("last_error_code", "oe_last_error_code", &[], Some(Int));
            cmd("last_error_text", "oe_last_error_text", &[], Some(Text));

            // --- Integer math ------------------------------------------------
            cmd("abs_int", "oe_abs_int", &[Int], Some(Int));
            cmd("min_int", "oe_min_int", &[Int, Int], Some(Int));
            cmd("max_int", "oe_max_int", &[Int, Int], Some(Int));
            cmd("mod_int", "oe_mod_int", &[Int, Int], Some(Int));
            cmd("pow_int", "oe_pow_int", &[Int, Int], Some(Int));

            // --- Floating-point math ----------------------------------------
            cmd("sqrt", "oe_sqrt", &[Double], Some(Double));
            cmd("sin", "oe_sin", &[Double], Some(Double));
            cmd("cos", "oe_cos", &[Double], Some(Double));
            cmd("tan", "oe_tan", &[Double], Some(Double));
            cmd("pow", "oe_pow", &[Double, Double], Some(Double));
            cmd("exp", "oe_exp", &[Double], Some(Double));
            cmd("ln", "oe_ln", &[Double], Some(Double));
            cmd("log10", "oe_log10", &[Double], Some(Double));
            cmd("floor", "oe_floor", &[Double], Some(Double));
            cmd("ceil", "oe_ceil", &[Double], Some(Double));
            cmd("round", "oe_round", &[Double], Some(Double));
            cmd("abs_double", "oe_abs_double", &[Double], Some(Double));
            cmd(
                "min_double",
                "oe_min_double",
                &[Double, Double],
                Some(Double),
            );
            cmd(
                "max_double",
                "oe_max_double",
                &[Double, Double],
                Some(Double),
            );

            // --- Conversions -------------------------------------------------
            cmd("int_to_double", "oe_int_to_double", &[Int], Some(Double));
            cmd("double_to_int", "oe_double_to_int", &[Double], Some(Int));
            cmd("int_to_int64", "oe_int_to_int64", &[Int], Some(Int64));
            cmd("int64_to_int", "oe_int64_to_int", &[Int64], Some(Int));
            cmd("int_to_text", "oe_int_to_text", &[Int], Some(Text));
            cmd("int64_to_text", "oe_int64_to_text", &[Int64], Some(Text));
            cmd("double_to_text", "oe_double_to_text", &[Double], Some(Text));
            cmd("text_to_int", "oe_text_to_int", &[Text], Some(Int));
            cmd("text_to_double", "oe_text_to_double", &[Text], Some(Double));

            // --- Text --------------------------------------------------------
            cmd("text_eq", "oe_text_eq", &[Text, Text], Some(Bool));
            cmd("length", "oe_length", &[Text], Some(Int));
            cmd("uppercase", "oe_uppercase", &[Text], Some(Text));
            cmd("lowercase", "oe_lowercase", &[Text], Some(Text));
            cmd("trim", "oe_trim", &[Text], Some(Text));
            cmd("substr", "oe_substr", &[Text, Int, Int], Some(Text));
            cmd("find", "oe_find", &[Text, Text], Some(Int));
            cmd("replace", "oe_replace", &[Text, Text, Text], Some(Text));
            cmd("concat", "oe_concat", &[Text, Text], Some(Text));
            cmd("repeat", "oe_repeat", &[Text, Int], Some(Text));
            cmd("reverse", "oe_reverse", &[Text], Some(Text));

            // --- Date / time -------------------------------------------------
            cmd("now", "oe_now", &[], Some(Int64));
            cmd("year", "oe_year", &[Int64], Some(Int));
            cmd("format_time", "oe_format_time", &[Int64, Text], Some(Text));
        }
        r
    }
}
