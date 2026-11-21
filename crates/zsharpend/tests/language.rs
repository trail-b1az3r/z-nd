//! Z#++ end to end.

use std::collections::HashMap;

use zsharpend::{Program, Value};

fn run(src: &str) -> String {
    let mut p = Program::from_source(src).unwrap_or_else(|e| panic!("load: {e}"));
    p.run_main().unwrap_or_else(|e| panic!("run: {e}\noutput: {}", p.output()));
    p.output()
}

fn run_err(src: &str) -> String {
    match Program::from_source(src) {
        Err(e) => e,
        Ok(mut p) => p.run_main().err().unwrap_or_else(|| panic!("expected an error, got output {}", p.output())),
    }
}

#[test]
fn zsharp_readme_example_runs_unchanged() {
    // Sam-astro's README example (comments on their own lines, as Z# requires).
    let src = r#"
// Comments are indicated by two forward slashes
func Main()
{
    int i = 0
    string s = "r"

    i += 2
    i -= 1
    i /= 3
    i *= 2

    while i < 10
    {
        i += 1
    }

    if s == "r"
    {
        Printl(s + " is r")
    }

    int functionNumber = ExampleFunction("A", s)
    ExampleFunction(1, 3)

    GlobalFunction()
    Printl(x)
    Printl(y)
}

func ExampleFunction(inputA, inputB)
{
    Printl("In A is: " + inputA)
    Printl("In B is: " + inputB)
    return 4
}

func GlobalFunction()
{
    global int x = 12
    global string y = "Y String"
}
"#;
    assert_eq!(run(src), "r is r\nIn A is: A\nIn B is: r\nIn A is: 1\nIn B is: 3\n12\nY String\n");
}

#[test]
fn zsharp_namespaces_and_vec2() {
    let out = run("func Main()\n{\n    Vec2 p = NVec2(3, 4)\n    Printl(ZS.Math.Sqrt(p.x * p.x + p.y * p.y))\n    Printl(ZS.Math.Round(2.6))\n}\n");
    assert_eq!(out, "5.0\n3\n");
}

#[test]
fn zsharp_plus_plus_features() {
    let src = r#"
struct Point { x: float, y: float }
impl Point {
    func len(self) -> float { return sqrt(self.x ** 2 + self.y ** 2) }
    func moved(self, dx) { return Point { x: self.x + dx, y: self.y } }
}
const NAME = "Z#++"

func Main() {
    let p = Point { x: 3, y: 4 }
    Printl("len ${p.len()} of ${NAME}")
    Printl(p.moved(1).x)

    let xs = [1, 2, 3, 4, 5]
    let total = xs |> filter(|v| v % 2 == 1) |> map(|v| v * 10) |> sum()
    Printl(total)
    Printl(xs.map(|v| v * v).join(","))

    let ages = { ada: 36, alan: 41 }
    for name in ages { Printl("${name}=${ages[name]}") }

    for i, v in ["a", "b"] { Print("${i}${v} ") }
    Printl("")

    let size = match 7 { 0 => "zero", 1..5 => "small", 5 | 6 | 7 => "medium", _ => "big" }
    Printl(size)

    let maybe = null
    Printl(maybe?.name ?? "nobody")
    Printl(total > 50 ? "big" : "small")

    try { throw "oops" } catch e { Printl("caught ${e}") }
    try { let z = 1 / 0 } catch e { Printl(e) }

    let add = func(a, b = 10) { return a + b }
    Printl(add(1))
    Printl(add(1, 2))

    var n = 0
    while true { n += 1; if n >= 3 { break } }
    Printl(n)
    Printl("Z#" in "Z#++")
    Printl(#2f7cf6)
    Printl(rgb(255, 0, 0).r)
}
"#;
    let out = run(src);
    assert_eq!(
        out,
        "len 5.0 of Z#++\n4.0\n90\n1,4,9,16,25\nada=36\nalan=41\n0a 1b \nmedium\nnobody\nbig\ncaught oops\ndivision by zero\n11\n3\n3\ntrue\n#2f7cf6\n255\n"
    );
}

#[test]
fn events() {
    let mut p = Program::from_source(
        "let seen = []\non load { seen.push(\"load\") }\non page_load \"*://example.com/*\" (url) { seen.push(url) }\n",
    )
    .unwrap();
    assert_eq!(p.interp.emit("load", None, vec![]).unwrap(), 1);
    assert_eq!(p.interp.emit("page_load", Some("https://example.com/a"), vec![Value::str("https://example.com/a")]).unwrap(), 1);
    assert_eq!(p.interp.emit("page_load", Some("https://other.org/"), vec![Value::str("x")]).unwrap(), 0);
    assert_eq!(p.interp.global("seen").unwrap().to_string(), "[\"load\", \"https://example.com/a\"]");
}

#[test]
fn sandbox() {
    assert!(run_err("func Main() { while true { } }").contains("ran out of steps"));
    assert!(run_err("func f(n) { return f(n + 1) }\nfunc Main() { f(0) }").contains("nested deeper"));
    assert!(run_err("func Main() { ZS.System.Command(\"rm -rf /\") }").contains("can't run programs"));
    assert!(run_err("func Main() { let s = \"x\" * 100000000 }").contains("too large"));
    assert!(run_err("func Main() { Canvas(100000, 10) }").contains("outside"));
    assert!(run_err("func Main() { undefined_thing() }").contains("isn't defined"));
    assert!(run_err("const K = 1\nfunc Main() { K = 2 }").contains("constant"));
}

fn files(map: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let m: HashMap<String, String> = map.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    move |p: &str| m.get(p).cloned()
}

#[test]
fn headers_are_checked() {
    let header = "struct Size { w: int, h: int }\nfunc area(s: Size) -> int\nextern func host_now() -> int\n";
    let ok = files(&[("app/main.zsp", "import \"lib/shapes.zsx\"\nfunc area(s: Size) -> int { return s.w * s.h }\nfunc Main() { Printl(area(Size { w: 3, h: 4 })) ; Printl(host_now()) }\n"), ("app/lib/shapes.zsx", header)]);
    let mut p = Program::load("app/main.zsp", &ok, |it| it.register("host_now", |_, _| Ok(Value::Int(42)))).unwrap();
    p.run_main().unwrap();
    assert_eq!(p.output(), "12\n42\n");

    let missing = files(&[("main.zsp", "import \"shapes.zsx\"\nfunc Main() {}\n"), ("shapes.zsx", header)]);
    let e = Program::load("main.zsp", &missing, |it| it.register("host_now", |_, _| Ok(Value::Null))).err().unwrap();
    assert!(e.contains("`area` is declared in shapes.zsx but never defined"), "{e}");

    let wrong = files(&[("main.zsp", "import \"shapes.zsx\"\nfunc area(s: Size, extra) -> int { return 0 }\n"), ("shapes.zsx", header)]);
    let e = Program::load("main.zsp", &wrong, |it| it.register("host_now", |_, _| Ok(Value::Null))).err().unwrap();
    assert!(e.contains("with 1 parameter(s) but defined with 2"), "{e}");

    let no_host = files(&[("main.zsp", "import \"shapes.zsx\"\nfunc area(s: Size) -> int { return 0 }\n"), ("shapes.zsx", header)]);
    assert!(Program::load("main.zsp", &no_host, |_| {}).err().unwrap().contains("host_now"));

    let escape = files(&[("main.zsp", "import \"../secret.zsx\"\n")]);
    assert!(Program::load("main.zsp", &escape, |_| {}).err().unwrap().contains("outside"));
}

#[test]
fn gpu_and_2d() {
    let src = files(&[
        (
            "demo.zsp",
            r#"
import "fx.zsg"
func Main() {
    let out = double.run({ xs: [1, 2, 3] }, { k: 2 })
    Printl(out.xs)
    let c = Canvas(64, 32)
    c.clear(#000000)
    c.rect(0, 0, 32, 32, #ff0000)
    c.circle(48, 16, 8, #00ff00)
    c.shade(ramp, 0, 0, 64, 4)
    c.text(2, 20, "Z#", 7, #ffffff)
    Printl(c.pixel(16, 16))
    Printl(c.pixel(48, 16))
    Printl(c.pixel(63, 0).r > 200)
    Printl(c.width)
    global let canvas = c
}
"#,
        ),
        (
            "fx.zsg",
            "kernel double(buf xs: [float], uniform k: float) { xs[gid] = xs[gid] * k }\nfragment ramp(uv: vec2) -> color { return vec4(uv.x, uv.x, uv.x, 1.0) }\n",
        ),
    ]);
    let mut p = Program::load("demo.zsp", &src, |_| {}).unwrap();
    p.run_main().unwrap();
    assert_eq!(p.output(), "[2.0, 4.0, 6.0]\n#ff0000\n#00ff00\ntrue\n64\n");
    // The host can read the pixels back.
    let Some(Value::Object(o)) = p.interp.global("canvas") else { panic!("no canvas") };
    let c = o.as_any().downcast_ref::<zsharpend::canvas::Canvas>().unwrap();
    assert_eq!(c.rgba().len(), 64 * 32 * 4);
    assert!(c.png().unwrap().starts_with(b"\x89PNG"));
}

#[test]
fn zsharp_graphics_init() {
    let mut p = Program::from_source("func Main() { ZS.Graphics.Init(\"Pong\", 200, 100) }\nfunc Update(dt) { screen.rect(0, 0, 10, 10, #ffffff) }\n").unwrap();
    p.run_main().unwrap();
    assert_eq!(p.interp.graphics, Some(("Pong".into(), 200, 100)));
    p.interp.call_global("Update", vec![Value::Float(0.016)]).unwrap();
}

#[test]
fn string_escapes_for_terminal_codes() {
    // \e, \xHH and \u{…} (for terminal escape sequences)
    assert_eq!(run("func Main() {\n Printl(len(\"\\e[1m\"), len(\"\\x1b\"), \"\\x41\\u{42}\", len(\"\\u{1F600}\"))\n}"), "4 1 AB 1\n");
}
