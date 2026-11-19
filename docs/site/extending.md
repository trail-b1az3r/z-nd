# Extending Zsharpend

There are three ways to add to Zsharpend, from the simplest to the most powerful:

| | Written in | Adds | Used with |
|---|---|---|---|
| [Modules](#modules) | `.zxx` | functions, types, constants | `import name` |
| [Extensions](#extensions) | `.zxx` | code and event handlers in every program that lists them | `[extensions]` in the `.wtf` |
| [Script extenders](#script-extenders) | Rust | modules, globals, and passes over the syntax tree | `[extenders]` in the `.wtf` |

## Modules

A module is a `.zxx` file; its top-level names are what importers get. Put one next to the importing file (`stats.zxx` → `import stats`), in the project's `modules/<name>/` folder with a `module.wtf`, or install it. A module's code runs once, the first time anything imports it; its functions see its own names and the program's globals.

```zxx include=examples/shapes/modules/geometry/geometry.zxx
```

To share a module between projects, give its folder a [module.wtf](wtf.md#module.wtf) and install it:

```sh
whetstone install modules/geometry   # → ~/.zsharpend/modules/geometry
```

## Extensions

An extension is a `.zxx` file merged into the programs that list it, ahead of their own code: its functions become theirs, and its `on` handlers receive the events Whetstone (or another host) sends — `start` before `Main()`, `exit` after it, and the host's own events.

```text
// extensions/timing/ext.zxx    (module.wtf: kind = "extension")
var calls = 0
on start { Printl("[timing] starting") }
on exit { Printl("[timing] done") }
```

```toml
[extensions]
use = ["timing"]
```

## Script extenders

A script extender is a Rust type that implements `zsharpend::extend::Extender`. It's how `nd` and `gfx3d` are built, and it can do four things:

```rust
pub trait Extender {
    fn name(&self) -> &str;
    fn about(&self) -> &str { "" }
    /// Modules a program can `import` by name.
    fn modules(&self, it: &mut Interp) -> Vec<(String, Value)> { vec![] }
    /// Globals: functions, constants, host objects.
    fn install(&self, it: &mut Interp) {}
    /// The globals' names, so Whetstone's checks know them.
    fn globals(&self) -> Vec<String> { vec![] }
    /// A pass over each parsed program file, before it runs.
    fn transform(&self, path: &str, items: &mut Vec<Item>) -> Result<(), String> { Ok(()) }
}
```

A module is a map of functions (`zsharpend::extend::namespace`); functions are `Value::native(name, |interp, args| …)`; types that scripts hold — like `nd`'s arrays and `gfx3d`'s meshes — implement `HostObject`, which can also define operators (`binary`), subscripts (`index`, `set_index`) and how they print (`show`). The `transform` pass sees the syntax tree (`zsharpend::ast`): it can rewrite code, add items, or reject constructs with an error.

```rust
use zsharpend::extend::{namespace, Extender};
use zsharpend::{Interp, Value};

/// `import units` → units.km(3) == 3000.0
pub struct Units;

impl Extender for Units {
    fn name(&self) -> &str { "units" }
    fn about(&self) -> &str { "unit conversions" }
    fn modules(&self, _it: &mut Interp) -> Vec<(String, Value)> {
        let km = Value::native("units.km", |_, a| Ok(Value::Float(a[0].as_f64().unwrap_or(0.0) * 1000.0)));
        vec![("units".into(), namespace(vec![("km", km)]))]
    }
}
```

Extenders are compiled into the host that runs scripts: add the crate to Whetstone's `extenders()` list (or to your own host's `Options`), and list it in a project's `.wtf`. Rust has no safe way to load native plugins at run time, so there are no binary plugin files.
