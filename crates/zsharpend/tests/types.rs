//! Roadmap stage 2: runtime type objects, reflection, conversion and
//! checked annotations.

use zsharpend::Program;

fn run(src: &str) -> String {
    let mut p = Program::from_zxx(src).unwrap_or_else(|e| panic!("{e}"));
    p.run_main().unwrap_or_else(|e| panic!("{e}"));
    p.output()
}

fn run_err(src: &str) -> String {
    match Program::from_zxx(src) {
        Err(e) => e,
        Ok(mut p) => p.run_main().expect_err("expected an error"),
    }
}

#[test]
fn reflection() {
    let out = run(r#"
struct P { x: int }
fn Main() {
    Printl(type_of(1), type_of(1.5), type_of("s"), type_of([1]), type_of({}), type_of(null), type_of(P { x: 1 }), type_of(text))
    Printl(type(1), type("s"), type_of("s") == "string", type_of("s") == text, type_of(1) == int, type_of(1) != float)
    let t = Type("map<text, list<int>>")
    Printl(t, t.name, t.base, t.params, t.params[1].params[0])
}
"#);
    assert_eq!(out, "int float text list map null P type\nint string true true true true\nmap<text, list<int>> map<text, list<int>> map [text, list<int>] int\n");
}

#[test]
fn is_checks_values() {
    let out = run(r#"
struct P { x: int }
enum Shape { Circle(r), Empty }
trait Named { fn nm(self) { return "n" } }
impl Named for P { }
fn Main() {
    Printl(is(1, int), 1.is(number), 1.5.is(int), "a".is(char), "ab".is(char), 255.is(byte), 256.is(byte))
    Printl([1, 2].is("list<int>"), [1, "a"].is("list<int>"), { a: [1] }.is("map<text, list<int>>"), (1, "x").is("(int, text)"))
    Printl(P { x: 1 }.is(P), Shape::Circle(1).is(Shape), Empty.is("Shape"), P { x: 1 }.is("Named"), 5.is(P))
    Printl(null.is("int?"), 5.is("int?"), Ok(1).is("result<int, text>"), Err(1).is("result<int, text>"), 5.is(dynamic))
    Printl(is_type(1, "number"), is_type("s", "String"), is_type(null, "none"), is_type(1, "Integer"), is_type(P { x: 1 }, "P"))
}
"#);
    assert_eq!(
        out,
        "true true false true false true false\ntrue false true true\ntrue true true true false\ntrue true true false true\ntrue true true true true\n"
    );
}

#[test]
fn as_and_try_as_convert_safely() {
    let out = run(r#"
fn Main() {
    Printl("42".as(int), "4.5".as(float), "4.5".as(number), "7".as(number), 2.0.as(int), 5.as(text), "yes".as(bool), 65.as(char))
    Printl(["1", "2"].as("list<int>"), (0..3).as("list<float>"), "ab".as(list), { a: "1" }.as("map<text, int>"), null.as("int?"))
    Printl("x".try_as(int), "12".try_as(int), 300.try_as(byte), 3.5.try_as(int))
    try { "twelve".as(int) } catch e { Printl(e) }
}
"#);
    assert_eq!(
        out,
        "42 4.5 4.5 7 2 5 true A\n[1, 2] [0.0, 1.0, 2.0] [\"a\", \"b\"] {a: 1} null\n\
         Err { error: can't convert string (\"x\") to int } Ok { value: 12 } Err { error: can't convert int (300) to byte } Err { error: can't convert float (3.5) to int }\n\
         line 6: can't convert string (\"twelve\") to int\n"
    );
}

#[test]
fn native_annotations_are_checked() {
    let ok = run(r#"
struct P { x: int }
fn mean(xs: list<number>) -> float { return sum(xs) / len(xs) }
fn Main() {
    let a: text = "hi"
    let b: number = 3
    let c: map<text, list<int>> = { a: [1], b: [] }
    let d: int? = null
    let e: (int, text) = (1, "x")
    let f: list<float> = [1, 2.5]
    let g: byte
    let h: list<P> = [P { x: 1 }]
    let i: SomeHostType = 5
    Printl(a, b, c, d, e, f, g, h[0].x, i, mean([1, 2.0]))
}
"#);
    assert_eq!(ok, "hi 3 {a: [1], b: []} null [1, \"x\"] [1, 2.5] 0 1 5 1.5\n");
    for (decl, msg) in [
        ("let n: text = 5", "expected text, got int (5)"),
        ("let n: list<int> = [1, \"a\"]", "expected list<int>, got list"),
        ("let n: map<text, int> = { a: \"x\" }", "expected map<text, int>"),
        ("let n: byte = 300", "expected byte, got int (300)"),
        ("let n: char = \"ab\"", "expected char"),
        ("let n: P = 5", "expected P, got int"),
        ("let n: int? = \"s\"", "expected optional<int>"),
        ("let n: (int, text) = (1, 2)", "expected (int, text)"),
        ("Printl(f(3))", "expected text, got int (3)"),
    ] {
        let src = format!("struct P {{ x: int }}\nfn f(x: number) -> text {{ return x }}\nfn Main() {{\n    {decl}\n}}\n");
        let e = run_err(&src);
        assert!(e.contains(msg), "{decl}: {e}");
    }
}

#[test]
fn original_annotations_still_convert() {
    let out = run(r#"
fn Main() {
    let a: int = 2.9
    let b: string = 42
    let c: u8 = 300
    let d: bool = "yes"
    Printl(a, b, c, d, type(b))
}
"#);
    assert_eq!(out, "2 42 44 true string\n");
}
