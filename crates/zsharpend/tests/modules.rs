//! Modules (`import name`), script extenders and extensions.

use std::collections::HashMap;

use zsharpend::ast::{Expr, Item, Stmt};
use zsharpend::extend::{namespace, Extender};
use zsharpend::program::Options;
use zsharpend::{Interp, Program, Value};

fn files(map: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let m: HashMap<String, String> = map.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    move |p: &str| m.get(p).cloned()
}

#[test]
fn source_modules() {
    let read = files(&[
        (
            "main.zxx",
            "import geometry\nimport geometry as g\nfrom geometry import area, UNIT as U\nfrom stats import mean\nfn Main() { Printl(geometry.area(2, 3), g.UNIT, area(1, 1), U, mean([1, 2, 3])) }\n",
        ),
        ("modules/geometry/mod.zxx", "const UNIT = \"cm\"\nfn area(w, h) { return scale(w * h) }\nfn scale(x) { return x * FACTOR }\nlet FACTOR = 10\n"),
        ("stats.zxx", "fn mean(xs) { return sum(xs) / len(xs) }\n"),
    ]);
    let mut p = Program::load("main.zxx", &read, |_| {}).unwrap();
    p.run_main().unwrap();
    assert_eq!(p.output(), "60 cm 10 cm 2\n");
    // A module's own names don't leak into the program.
    assert!(p.interp.global("scale").is_none());
}

#[test]
fn installed_modules_and_missing_ones() {
    let read = files(&[("main.zxx", "import greet\nfn Main() { Printl(greet.hello(\"Ada\")) }\n"), ("@installed/greet/mod.zxx", "fn hello(n) { return \"hi ${n}\" }\n")]);
    let mut p = Program::load("main.zxx", &read, |_| {}).unwrap();
    p.run_main().unwrap();
    assert_eq!(p.output(), "hi Ada\n");
    let read = files(&[("main.zxx", "import nowhere\n")]);
    let e = Program::load("main.zxx", &read, |_| {}).err().unwrap();
    assert!(e.contains("no module `nowhere`"), "{e}");
}

/// An extender: a `units` module, a global, and a pass that rewrites
/// `debug(x)` calls away when the project is built for release.
struct Units;
impl Extender for Units {
    fn name(&self) -> &str {
        "units"
    }
    fn modules(&self, _it: &mut Interp) -> Vec<(String, Value)> {
        vec![("units".into(), namespace(vec![("km", Value::native("km", |_, a| Ok(Value::Float(a[0].as_f64().unwrap_or(0.0) * 1000.0))))]))]
    }
    fn install(&self, it: &mut Interp) {
        it.set_global("SPEED_OF_LIGHT", Value::Int(299_792_458));
    }
    fn globals(&self) -> Vec<String> {
        vec!["SPEED_OF_LIGHT".into()]
    }
    fn transform(&self, _path: &str, items: &mut Vec<Item>) -> Result<(), String> {
        for it in items.iter_mut() {
            if let Item::Func(f) = it {
                let mut f2 = (**f).clone();
                if let Some(body) = &mut f2.body {
                    body.retain(|s| !matches!(s, Stmt::Expr(Expr::Call(c, _, _), _) if matches!(&**c, Expr::Var(n, _) if n == "debug")));
                }
                *f = std::rc::Rc::new(f2);
            }
        }
        Ok(())
    }
}

#[test]
fn script_extenders() {
    let read = files(&[("main.zxx", "import units\nfn Main() {\n    debug(\"removed by the extender\")\n    Printl(units.km(3), SPEED_OF_LIGHT)\n}\n")]);
    let ext: [&dyn Extender; 1] = [&Units];
    let mut p = Program::load_with("main.zxx", &read, |_| {}, Options { extenders: &ext, ..Default::default() }).unwrap();
    p.run_main().unwrap();
    assert_eq!(p.output(), "3000.0 299792458\n");
}

#[test]
fn extensions_hook_into_the_program() {
    let read = files(&[
        ("main.zxx", "fn Main() { Printl(\"main\") }\n"),
        ("extensions/timing/ext.zxx", "var started = false\non start { started = true; Printl(\"timing: start\") }\nfn elapsed() { return started ? \"ran\" : \"not started\" }\n"),
    ]);
    let mut p = Program::load_with("main.zxx", &read, |_| {}, Options { extensions: vec!["extensions/timing/ext.zxx".into()], ..Default::default() }).unwrap();
    p.interp.emit("start", None, vec![]).unwrap();
    p.run_main().unwrap();
    assert_eq!(p.interp.call_global("elapsed", vec![]).unwrap().unwrap().to_string(), "ran");
    assert_eq!(p.output(), "timing: start\nmain\n");
}
