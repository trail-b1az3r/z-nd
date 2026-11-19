//! **Script extenders**: native (Rust) plugins that extend the language —
//! the Zsharpend counterpart of game "script extenders". An extender can:
//!
//! * provide **modules** a program `import`s by name (`nd`, `gfx3d` are
//!   extenders);
//! * register **globals** (functions, constants, host objects);
//! * declare the **names** it defines, so Whetstone's compile-time checks
//!   know them;
//! * **transform** each parsed file before it runs — a pass over the syntax
//!   tree, for new statements, rewrites or project-wide rules.
//!
//! Extenders are compiled into the program that runs scripts (Whetstone,
//! StonedGem, your own host): Rust has no safe way to load native plugins
//! at run time, so an extender is a crate the host depends on and lists.

use crate::ast::Item;
use crate::interp::Interp;
use crate::value::Value;

pub trait Extender {
    /// Its name, as a `.wtf` lists it (`extenders = ["nd"]`).
    fn name(&self) -> &str;
    /// One line saying what it adds.
    fn about(&self) -> &str {
        ""
    }
    /// Modules it provides: `(name, namespace)`. Called once per program.
    fn modules(&self, _it: &mut Interp) -> Vec<(String, Value)> {
        vec![]
    }
    /// Register globals.
    fn install(&self, _it: &mut Interp) {}
    /// Global names it defines (for compile-time checks). Module names are
    /// taken from [`Extender::modules`].
    fn globals(&self) -> Vec<String> {
        vec![]
    }
    /// A pass over each parsed program file (`path`), before it runs.
    fn transform(&self, _path: &str, _items: &mut Vec<Item>) -> Result<(), String> {
        Ok(())
    }
}

/// Install extenders into an interpreter: their globals and modules.
pub fn install(it: &mut Interp, extenders: &[&dyn Extender]) {
    for e in extenders {
        e.install(it);
        for (name, m) in e.modules(it) {
            it.modules.insert(name, m);
        }
    }
}

/// Build a module namespace from `(name, value)` pairs.
pub fn namespace(items: Vec<(&str, Value)>) -> Value {
    Value::map(items.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
}
