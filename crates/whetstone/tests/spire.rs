//! Runs the spire shell example (examples/spire, written in Zsharpend with
//! the `os` extender) through the real `whetstone` binary.

use std::path::PathBuf;
use std::process::Command;

fn example() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/spire")
}

fn spire(args: &[&str]) -> (String, i32) {
    let out = Command::new(env!("CARGO_BIN_EXE_whetstone"))
        .arg("run")
        .arg(example())
        .arg("--")
        .args(args)
        .env("HOME", std::env::temp_dir().join("spire-zxx-home"))
        .output()
        .expect("whetstone runs");
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    (text, out.status.code().unwrap_or(-1))
}

#[test]
fn spire_runs_its_test_script() {
    let script = example().join("tests/basics.fish");
    let expected = std::fs::read_to_string(example().join("tests/basics.expected")).unwrap();
    let (out, _) = spire(&[script.to_str().unwrap()]);
    assert_eq!(out, expected);
}

#[test]
fn spire_dash_c_and_exit_status() {
    let (out, code) = spire(&["-c", "echo hi | string upper; exit 3"]);
    assert_eq!(out, "HI\n");
    assert_eq!(code, 3);
}

#[test]
fn spire_reports_unknown_commands() {
    let (out, code) = spire(&["-c", "gti status"]);
    assert!(out.contains("command not found: gti"), "{out}");
    assert!(out.contains("did you mean git?") || !std::path::Path::new("/usr/bin/git").exists(), "{out}");
    assert_eq!(code, 127);
}
