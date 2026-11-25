//! Compatibility suite: every program in tests/compat/ must keep behaving
//! exactly as recorded. NAME.zsp / NAME.zxx is run and its output compared
//! with NAME.out, or its load/run error must contain NAME.err. Files whose
//! names start with `_` are helpers (headers, includes), not programs.
//!
//! The roadmap requires each stage to keep these passing. To record a new
//! fixture's output: UPDATE_COMPAT=1 cargo test -p zsharpend --test compat
//! (then read the .out file before committing it).

use std::path::{Path, PathBuf};

use zsharpend::Program;

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/compat")
}

fn run(entry: &str) -> Result<String, String> {
    let root = dir();
    let read = move |p: &str| std::fs::read_to_string(root.join(p)).ok();
    let mut prog = Program::load(entry, &read, |_| {})?;
    match prog.run_main() {
        Ok(()) => Ok(prog.output()),
        Err(e) => Err(e),
    }
}

#[test]
fn compatibility_fixtures() {
    let update = std::env::var_os("UPDATE_COMPAT").is_some();
    let mut names: Vec<String> = std::fs::read_dir(dir())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| (n.ends_with(".zsp") || n.ends_with(".zxx")) && !n.starts_with('_'))
        .collect();
    names.sort();
    assert!(names.len() >= 10, "fixtures missing: {names:?}");
    let mut failures = vec![];
    for name in &names {
        let stem = &name[..name.rfind('.').unwrap()];
        let out_file = dir().join(format!("{stem}.out"));
        let err_file = dir().join(format!("{stem}.err"));
        let result = run(name);
        if err_file.exists() {
            let want = std::fs::read_to_string(&err_file).unwrap();
            match result {
                Err(e) if e.contains(want.trim()) => {}
                Err(e) => failures.push(format!("{name}: error {e:?} doesn't contain {:?}", want.trim())),
                Ok(out) => failures.push(format!("{name}: expected an error, got output {out:?}")),
            }
            continue;
        }
        match result {
            Ok(out) if update => std::fs::write(&out_file, out).unwrap(),
            Ok(out) => {
                let want = std::fs::read_to_string(&out_file).unwrap_or_else(|_| format!("<no {stem}.out>"));
                if out != want {
                    failures.push(format!("{name}:\n--- expected\n{want}--- got\n{out}"));
                }
            }
            Err(e) => failures.push(format!("{name}: {e}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
