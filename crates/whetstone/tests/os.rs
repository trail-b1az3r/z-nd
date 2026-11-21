//! A project using the `os` extender, run through the real `whetstone`
//! binary: arguments after `--`, live output, pipelines, capture, exit codes.

use std::process::Command;

#[test]
fn os_program_runs_commands() {
    let dir = std::env::temp_dir().join(format!("whetstone-os-test-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("ostest.wtf"), "[project]\nname = \"ostest\"\n\n[extenders]\nuse = [\"os\"]\n").unwrap();
    std::fs::write(
        dir.join("src/main.zxx"),
        "import os\n\
         fn Main() {\n\
         \x20   Printl(\"args\", os.args())\n\
         \x20   let r = os.run([[\"printf\", \"b\\na\\n\"], [\"sort\"]], {capture: true})\n\
         \x20   os.write(r.output)\n\
         \x20   let missing = os.run([[\"no-such-command-xyz\"]], {})\n\
         \x20   Printl(missing.status, os.regex(\"^(a+)b$\", \"aab\"))\n\
         \x20   os.exit(3)\n\
         }\n",
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_whetstone")).arg("run").arg(&dir).arg("--").args(["x", "y"]).output().unwrap();
    std::fs::remove_dir_all(&dir).unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout), "args [\"x\", \"y\"]\na\nb\n127 [\"aab\", \"aa\"]\n");
    assert_eq!(out.status.code(), Some(3));
}
