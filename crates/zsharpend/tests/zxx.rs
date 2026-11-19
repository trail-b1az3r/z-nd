//! Zsharpend (.zxx): the Python, Rust and HolyC features.

use zsharpend::Program;

fn run(src: &str) -> String {
    let mut p = Program::from_zxx(src).unwrap_or_else(|e| panic!("load: {e}"));
    p.run_main().unwrap_or_else(|e| panic!("run: {e}\noutput: {}", p.output()));
    p.output()
}

fn run_err(src: &str) -> String {
    match Program::from_zxx(src) {
        Err(e) => e,
        Ok(mut p) => p.run_main().err().unwrap_or_else(|| panic!("expected an error, got output {}", p.output())),
    }
}

#[test]
fn python_features() {
    let out = run(r#"
fn Main() {
    let xs = [1, 2, 3, 4, 5, 6]
    Printl([x * x for x in xs if x % 2 == 0])
    Printl({ str(x): x * 10 for x in 1..=3 })
    Printl(xs[1:4], xs[::2], xs[-2:], xs[::-1])
    Printl("Zsharpend"[1:6])
    let (a, b) = (1, 2)
    a, b = b, a
    Printl(a, b)
    for (i, v) in enumerate(["x", "y"]) { Print("${i}${v} ") }
    Printl("")
    for (k, v) in items({ one: 1, two: 2 }) { Print("${k}=${v} ") }
    Printl("")
    Printl(any([0, 0, 3]), all([1, 0]), 4 not in xs, None == null, True and not False)
    let grid = [[1, 2], [3, 4]]
    Printl(grid[1, 0])
    grid[0, 1] = 9
    Printl(grid)
    xs[1:3] = [20, 30, 40]
    Printl(xs)
    Printl(area(2, height=5), area(width=3, height=4))
}
fn area(width, height = 1) { return width * height }
"#);
    assert_eq!(
        out,
        "[4, 16, 36]\n{1: 10, 2: 20, 3: 30}\n[2, 3, 4] [1, 3, 5] [5, 6] [6, 5, 4, 3, 2, 1]\nsharp\n2 1\n0x 1y \none=1 two=2 \ntrue false false true true\n3\n[[1, 9], [3, 4]]\n[1, 20, 30, 40, 4, 5, 6]\n10 12\n"
    );
}

#[test]
fn decorators() {
    let out = run(r#"
fn twice(f) { return |x| f(f(x)) }
@twice
fn inc(x) { return x + 1 }
fn Main() { Printl(inc(5)) }
"#);
    assert_eq!(out, "7\n");
}

#[test]
fn rust_features() {
    let out = run(r#"
enum Shape {
    Circle(r),
    Rect(w, h),
    Empty,
}

trait Area {
    fn area(self) -> float
    fn describe(self) { return "area ${round(self.area())}" }
}

impl Area for Shape {
    fn area(self) -> float {
        return match self {
            Circle(r) => PI * r * r,
            Rect(w, h) => w * h,
            _ => 0.0,
        }
    }
}

fn parse_num(s) {
    if s == "" { return Err("empty") }
    return Ok(int(s))
}

fn total(a, b) {
    let x = parse_num(a)?
    let y = parse_num(b)?
    return Ok(x + y)
}

fn Main() {
    let mut shapes = [Shape::Circle(1), Rect(2, 3), Empty]
    for s in shapes { Printl(s.describe()) }
    Printl(total("2", "40"), total("2", ""))
    Printl(unwrap(total("1", "1")), total("", "1").unwrap_or(-1))
    let r = match total("5", "5") { Ok(v) => "got ${v}", Err(e) => "failed: ${e}" }
    Printl(r)
    var n = 0
    loop { n += 1; if n == 3 { break } }
    Printl(n)
    let maybe = None
    Printl(match maybe { Some(v) => v, _ => "nothing" })
    let byte: u8 = 300
    let small: i8 = 200
    Printl(byte, small)
}
"#);
    assert_eq!(out, "area 3\narea 6\narea 0\nOk { value: 42 } Err { error: empty }\n2 -1\ngot 10\n3\nnothing\n44 -56\n");
}

#[test]
fn trait_missing_method_is_an_error() {
    let e = run_err("trait T { fn need(self) }\nstruct S { x: int }\nimpl T for S { }\nfn Main() {}\n");
    assert!(e.contains("missing `need`"), "{e}");
}

#[test]
fn holyc_features() {
    let out = run(r#"
U0 Hello() {
    "Hello from HolyC style\n";
}

fn Main() {
    Hello;
    I64 count = 3
    F64 ratio = 0.5
    "%d items at %5.2f%% (%-4s|%04d|%x)\n", count, ratio * 100, "ok", 42, 255;
    for n in [0, 3, 7, 12] {
        switch n {
            case 0: "zero ";
            case 1...5: "small ";
            case 6, 7: "lucky ";
            default: "big ";
        }
    }
    "\n";
    Printl(format("%s=%d", "x", 7))
}
"#);
    assert_eq!(out, "Hello from HolyC style\n3 items at 50.00% (ok  |0042|ff)\nzero small lucky big \nx=7\n");
}

#[test]
fn holyc_function_declarations() {
    // HolyC writes the return type first: `I64 Square(I64 x)`.
    let out = run("I64 Square(I64 x) { return x * x }\nfn Main() { Printl(Square(9)) }\n");
    assert_eq!(out, "81\n");
}

#[test]
fn zsharp_plus_plus_files_are_unchanged() {
    // `loop`, `case` and `fn` are ordinary names in a .zsp file.
    let mut p = Program::from_source("func Main() { let loop = 1\n let case = 2\n let fn = 3\n Printl(loop + case + fn) }\n").unwrap();
    p.run_main().unwrap();
    assert_eq!(p.output(), "6\n");
}

#[test]
fn errors_are_clear() {
    assert!(run_err("fn f(a) { return a }\nfn Main() { f(b=1) }\n").contains("no parameter `b`"));
    assert!(run_err("fn Main() { let (a, b) = [1, 2, 3] }\n").contains("3 values to unpack into 2"));
    assert!(run_err("fn Main() { \"%d\\n\"; \"%d %d\\n\", 1; }\n").contains("more `%`"));
}

#[test]
fn holyc_include() {
    let files = |p: &str| match p {
        "main.zxx" => Some("#include \"util.zxx\"\nfn Main() { Printl(Twice(4)) }\n".to_string()),
        "util.zxx" => Some("I64 Twice(I64 x) { return x * 2 }\n".to_string()),
        _ => None,
    };
    let mut p = Program::load("main.zxx", &files, |_| {}).unwrap();
    p.run_main().unwrap();
    assert_eq!(p.output(), "8\n");
}
