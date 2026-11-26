//! Roadmap stage 3: native declarations (`name: Type = v`, `name := v`),
//! types in expressions, kept variable types and checks before running —
//! with the lexer and parser cases that must not change meaning.

use zsharpend::ast::{Item, Kind, Stmt};
use zsharpend::lexer::{lex, Tok};
use zsharpend::parser::parse;
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

/// The statements of `fn Main` in a .zxx source.
fn main_body(src: &str) -> Vec<Stmt> {
    let items = parse(src, Kind::Zxx).unwrap_or_else(|e| panic!("{e}"));
    items
        .into_iter()
        .find_map(|i| match i {
            Item::Func(f) if f.name == "Main" => f.body.clone(),
            _ => None,
        })
        .unwrap()
}

#[test]
fn lexer_has_walrus() {
    let toks: Vec<Tok> = lex("x := 1\na: int").unwrap().into_iter().map(|t| t.tok).collect();
    assert!(toks.contains(&Tok::Op(":=")), "{toks:?}");
    assert!(toks.contains(&Tok::Op(":")), "{toks:?}");
}

#[test]
fn native_declarations_parse() {
    let body = main_body("fn Main() {\n    a: text = \"x\"\n    b := 2\n    c: map<text, list<int>>= {}\n    d: int?\n}\n");
    let shapes: Vec<(String, Option<String>, bool, bool)> = body
        .iter()
        .map(|s| match s {
            Stmt::Let { name, ty, value, native, .. } => (name.clone(), ty.as_ref().map(|t| t.name.clone()), value.is_some(), *native),
            other => panic!("not a declaration: {other:?}"),
        })
        .collect();
    assert_eq!(
        shapes,
        vec![
            ("a".into(), Some("text".into()), true, true),
            ("b".into(), None, true, true),
            ("c".into(), Some("map<text, list<int>>".into()), true, true),
            ("d".into(), Some("int?".into()), false, true),
        ]
    );
}

#[test]
fn look_alikes_keep_their_meaning() {
    // `default:` in a switch, a ternary, `?` error propagation, comparisons
    // in arguments, map literals and slices all parse as they did.
    let out = run(r#"
fn half(s) {
    let n = int(s)
    if n % 2 == 1 { return Err("odd") }
    return Ok(n / 2)
}
fn quarter(s) {
    let h = half(s)?
    return half(str(h))
}
fn Main() {
    limit := 3
    other := 1
    flag := true
    pick := flag ? limit : other
    switch pick { case 1: "one "; default: "other "; }
    Printl(max(limit < other, other > 0), pick, quarter("8"), quarter("6"))
    xs := [1, 2, 3, 4]
    Printl(xs[1:3], { a: 1 }, xs[:2])
}
"#);
    assert_eq!(out, "other true 3 Ok { value: 2 } Err { error: odd }\n[2, 3] {a: 1} [1, 2]\n");
}

#[test]
fn zsp_is_unchanged() {
    assert!(Program::from_source("func Main() {\n    x := 1\n}\n").is_err());
    assert!(Program::from_source("func Main() {\n    x: int = 1\n}\n").is_err());
}

#[test]
fn variables_keep_their_type() {
    let out = run(r#"
struct P { x: int }
fn Main() {
    name: text = "ada"
    n := 1
    ratio: float = 1
    maybe: int?
    xs: list<int>
    n += 2
    ratio = 3
    maybe = 4
    xs = [5]
    let loose = 1
    loose = "anything"
    let legacy: int = 2.5
    legacy = "still converting rules"
    Printl(name, n, ratio, maybe, xs, loose, legacy)
    let n = "redeclared"
    Printl(n)
}
"#);
    assert_eq!(out, "ada 3 3.0 4 [5] anything still converting rules\nredeclared\n");
    for (stmt, msg) in [
        ("n := 1\n    n = \"x\"", "`n` is int, can't assign string (x)"),
        ("s: text = \"a\"\n    s = 1", "`s` is text, can't assign int (1)"),
        ("let s: text = \"a\"\n    s = 1", "`s` is text"),
        ("p := P { x: 1 }\n    p = 2", "`p` is P"),
        ("x: int = 1.5", "expected int, got float (1.5)"),
        ("f: float = \"1\"", "expected float, got string"),
        ("n := 1\n    n += 0.5", "`n` is int, can't assign float (1.5)"),
    ] {
        let e = run_err(&format!("struct P {{ x: int }}\nfn Main() {{\n    {stmt}\n}}\n"));
        assert!(e.contains(msg), "{stmt}: {e}");
    }
}

#[test]
fn types_in_expressions() {
    let out = run(r#"
fn Main() {
    xs := [1, 2]
    Printl(xs.is(list<int>), ["a"].is(list<int>), { a: [1] }.is(map<text, list<int>>))
    Printl(null.is(int?), 5.is(text?), (1, "a").is((int, text)), Ok(1).is(result<int, text>))
    t := map<text, int>
    Printl(t, t.accepts({ a: 1 }), ["1"].as(list<int>), Type(list<list<int>>))
}
"#);
    assert_eq!(out, "true false true\ntrue false true true\nmap<text, int> true [1] list<list<int>>\n");
}

#[test]
fn literal_declarations_are_checked_before_running() {
    let e = run_err(
        r#"
fn never_called() {
    n: int = "seven"
    if true { let s: text = 5 }
    let legacy: int = "only checked when it runs"
    m: map<text, int> = { a: "x" }
    computed: int = str(5)
}
fn Main() { Printl("ran") }
"#,
    );
    assert_eq!(e, "line 3: expected int, got string (seven)\nline 4: expected text, got int (5)\nline 6: expected map<text, int>, got map ({a: x})");
}
