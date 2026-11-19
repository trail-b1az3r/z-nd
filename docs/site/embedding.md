# Embedding

Zsharpend is a Rust crate, `zsharpend`, with the same host API as Z#++: load a program with a function that reads its files, register what scripts may call, run it, and talk to it with calls and events.

```rust
use zsharpend::program::Options;
use zsharpend::{Program, Value};

let read = |path: &str| std::fs::read_to_string(format!("scripts/{path}")).ok();
let exts = whetstone::extenders();                        // nd, gfx3d
let refs: Vec<&dyn zsharpend::extend::Extender> = exts.iter().map(|e| e.as_ref()).collect();
let mut program = Program::load_with("main.zxx", &read, |it| {
    it.register("now", |_, _| Ok(Value::Int(42)));        // a host function
}, Options { extenders: &refs, extensions: vec![] })?;
program.run_main()?;
let answer = program.interp.call_global("score", vec![Value::Int(3)])?;
program.interp.emit("tick", None, vec![Value::Float(0.016)])?;
print!("{}", program.output());
```

| | |
|---|---|
| `Program::load(entry, read, setup)` | Load a program (Z#++ API). |
| `Program::load_with(entry, read, setup, options)` | With script extenders and extensions. |
| `Program::from_zxx(source)` | One `.zxx` source, no imports. |
| `interp.register(name, f)`, `interp.set_global(name, v)` | Give scripts functions and values. |
| `interp.call_global`, `interp.emit`, `interp.emit_values` | Call a function; send an event (`on name`) and collect what handlers return. |
| `interp.limits` | The sandbox's limits: steps, call depth, sizes, time. |

Scripts reach only what the host gives them: no files except through `read`, no processes, no network.

## StonedGem

StonedGem — the browser base under OyNIx — runs extension scripts with this API. A Whetstone project with `host = "stonedgem"` is checked against StonedGem's extension functions (`page_url`, `page_title`, `inject_css`, `run_js`, `notify`, `store_get`, `store_set`, `panel_set`) and its events (`page_load`, `message`, `panel`). `whetstone run` stands in for the browser — the functions print what they would do — so a script can be tried from a terminal:

```sh
whetstone new reader --host stonedgem
cd reader
whetstone run --page https://docs.example.com/ --title "Docs"
```

```text
try it: whetstone run --page https://example.com/
[stonedgem] notify(Opened Docs)
```

The same script, as `script.zsp` in an extension, runs in the browser. StonedGem's own extension runtime currently loads Z#++ (`.zsp`) scripts; `.zxx` scripts need a StonedGem built with the `zsharpend` crate in place of `stonedgem-zsharp`, which has the same API.
