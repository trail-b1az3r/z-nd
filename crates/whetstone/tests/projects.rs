//! Whetstone end to end: projects, checks, bundles, installs, hosts.
//! (One test: they share ZSHARPEND_HOME.)

use std::path::Path;

use whetstone::{Bundle, Project, RunOptions, Severity};

fn write(dir: &Path, files: &[(&str, &str)]) {
    for (f, t) in files {
        let p = dir.join(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, t).unwrap();
    }
}

#[test]
fn whetstone() {
    let root = std::env::temp_dir().join(format!("whetstone-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::env::set_var("ZSHARPEND_HOME", root.join("home"));

    // ── A project with a local module, a Z#++ header, nd and an extension ──
    let app = root.join("app");
    write(
        &app,
        &[
            (
                "app.wtf",
                "[project]\nname = \"app\"\nversion = \"1.0.0\"\n\n[extenders]\nuse = [\"nd\"]\n\n[extensions]\nuse = [\"audit\"]\n",
            ),
            (
                "src/main.zxx",
                "import geometry\nimport nd\nimport \"shapes.zsx\"\n\nfn area(s: Size) -> int { return s.w * s.h }\n\nfn Main() {\n    let sizes = [Size { w: 2, h: 3 }, Size { w: 4, h: 5 }]\n    Printl([area(s) for s in sizes], geometry.double(21), nd.array([1, 2]) * 3)\n}\n",
            ),
            ("src/shapes.zsx", "struct Size { w: int, h: int }\nfunc area(s: Size) -> int\n"),
            ("modules/geometry/module.wtf", "[module]\nname = \"geometry\"\nentry = \"geometry.zxx\"\n"),
            ("modules/geometry/geometry.zxx", "fn double(x) { return x * 2 }\n"),
            ("extensions/audit/module.wtf", "[module]\nname = \"audit\"\nkind = \"extension\"\nentry = \"audit.zxx\"\n"),
            ("extensions/audit/audit.zxx", "on exit { Printl(\"audit: done\") }\n"),
        ],
    );
    let p = Project::open(&app).unwrap();
    let b = p.compile().unwrap();
    assert!(b.diagnostics.is_empty(), "{:?}", b.diagnostics.iter().map(|d| d.to_string()).collect::<Vec<_>>());
    assert_eq!(b.bundle.sources.keys().cloned().collect::<Vec<_>>(), ["extensions/audit/audit.zxx", "modules/geometry/mod.zxx", "src/main.zxx", "src/shapes.zsx"]);
    // The bundle runs without the project folder.
    let json = b.bundle.to_json();
    std::fs::remove_dir_all(&app).unwrap();
    let prog = Bundle::from_json(&json).unwrap().run(&RunOptions::default()).map_err(|b| b.1).unwrap();
    assert_eq!(prog.output(), "[6, 20] 42 array([3., 6.])\naudit: done\n");

    // ── Checks ──
    let bad = root.join("bad");
    write(
        &bad,
        &[
            ("bad.wtf", "[project]\nname = \"bad\"\n[build]\nstrict = true\n"),
            ("src/main.zxx", "fn f(a) { return a }\nfn Main() {\n    f(1, 2)\n    Printl(nope)\n    let quiet = 1\n}\nfn f(b) { return b }\n"),
        ],
    );
    let b = Project::open(&bad).unwrap().compile().unwrap();
    let msgs: Vec<String> = b.diagnostics.iter().map(|d| d.to_string()).collect();
    assert!(msgs.iter().any(|m| m == "src/main.zxx:3: error: `f` takes 1 argument(s), but 2 are given"), "{msgs:?}");
    assert!(msgs.iter().any(|m| m == "src/main.zxx:4: error: `nope` isn't defined"), "{msgs:?}");
    assert!(msgs.iter().any(|m| m == "src/main.zxx:5: warning: `quiet` is never used"), "{msgs:?}");
    assert!(msgs.iter().any(|m| m.contains("`f` is defined twice")), "{msgs:?}");
    assert!(!b.ok());
    assert_eq!(b.diagnostics.iter().filter(|d| d.severity == Severity::Error).count(), 3);

    // ── Installing ──
    let pkg = root.join("pkg/greet");
    write(&pkg, &[("module.wtf", "[module]\nname = \"greet\"\nversion = \"2.0.0\"\n"), ("mod.zxx", "fn hi(n) { return \"hi ${n}\" }\n")]);
    let m = whetstone::install(&pkg).unwrap();
    assert_eq!((m.name.as_str(), m.version.as_str()), ("greet", "2.0.0"));
    assert_eq!(whetstone::installed().len(), 1);
    let user = root.join("user");
    write(&user, &[("user.wtf", "[project]\nname = \"user\"\n"), ("src/main.zxx", "from greet import hi\nfn Main() { Printl(hi(\"Ada\")) }\n")]);
    let b = Project::open(&user).unwrap().compile().unwrap();
    assert!(b.ok());
    assert_eq!(b.bundle.run(&RunOptions::default()).map_err(|b| b.1).unwrap().output(), "hi Ada\n");
    whetstone::uninstall("greet").unwrap();
    assert!(Project::open(&user).unwrap().compile().err().unwrap().contains("no module `greet`"));

    // ── The StonedGem host ──
    let web = root.join("web");
    whetstone::new_project(&web, "web", "stonedgem").unwrap();
    let b = Project::open(&web).unwrap().compile().unwrap();
    assert!(b.ok(), "{:?}", b.diagnostics.iter().map(|d| d.to_string()).collect::<Vec<_>>());
    let out = b.bundle.run(&RunOptions { page_url: Some("https://example.com/x".into()), page_title: Some("X".into()), seed: None }).map_err(|b| b.1).unwrap().output();
    assert!(out.contains("[stonedgem] notify(Opened X)"), "{out}");
    // A browser-only function is unknown to a standalone project.
    let lone = root.join("lone");
    write(&lone, &[("lone.wtf", "[project]\nname = \"lone\"\n"), ("src/main.zxx", "fn Main() { notify(\"hi\") }\n")]);
    let b = Project::open(&lone).unwrap().compile().unwrap();
    assert!(b.diagnostics.iter().any(|d| d.message == "`notify` isn't defined"));

    // ── A Z#++ program builds unchanged ──
    let old = root.join("old");
    write(&old, &[("old.wtf", "[project]\nname = \"old\"\nentry = \"main.zsp\"\n"), ("main.zsp", "func Main()\n{\n    int loop = 2\n    Printl(loop * 21)\n}\n")]);
    let b = Project::open(&old).unwrap().compile().unwrap();
    assert!(b.ok());
    assert_eq!(b.bundle.run(&RunOptions::default()).map_err(|b| b.1).unwrap().output(), "42\n");
}
