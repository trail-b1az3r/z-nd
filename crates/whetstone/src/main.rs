//! `whetstone` — compile, check, run and manage Zsharpend projects.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use whetstone::{Bundle, Project, RunOptions};

const HELP: &str = "whetstone — the Zsharpend compiler

USAGE
  whetstone new <dir> [--host stonedgem]   start a project (a .wtf and src/main.zxx)
  whetstone check [project]                 parse and check; report errors and warnings
  whetstone build [project]                 check, then write build/<name>.wsb
  whetstone run [project | file.wsb]        build (if needed) and run
        --png <file>        save the canvas the program leaves in `canvas`/`screen`
        --page <url>        StonedGem host: the page, and a page_load event after Main
        --title <text>      StonedGem host: the page's title
        --seed <n>          seed random() for repeatable runs
  whetstone install <folder>                install a module or extension (it has a module.wtf)
  whetstone uninstall <name>
  whetstone list                            installed modules and extensions
  whetstone extenders                       script extenders built into this Whetstone

[project] is a folder with one .wtf, or the .wtf itself (default: the current folder).
Installed modules live in $ZSHARPEND_HOME/modules (default ~/.zsharpend/modules).";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("whetstone: {e}");
            ExitCode::from(1)
        }
    }
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

/// The first argument that isn't a flag or a flag's value.
fn positional(args: &[String]) -> Option<&String> {
    let mut skip = false;
    for a in args {
        if skip {
            skip = false;
            continue;
        }
        if a.starts_with("--") {
            skip = matches!(a.as_str(), "--png" | "--page" | "--title" | "--seed" | "--host");
            continue;
        }
        return Some(a);
    }
    None
}

fn compile(path: &Path) -> Result<(Project, whetstone::Build), String> {
    let p = Project::open(path)?;
    let b = p.compile()?;
    for d in &b.diagnostics {
        eprintln!("{d}");
    }
    Ok((p, b))
}

fn summary(b: &whetstone::Build) -> String {
    let (e, w) = (b.errors(), b.warnings());
    let plural = |n: usize, s: &str| format!("{n} {s}{}", if n == 1 { "" } else { "s" });
    format!("{}, {}", plural(e, "error"), plural(w, "warning"))
}

fn run(args: &[String]) -> Result<ExitCode, String> {
    let Some(cmd) = args.first() else {
        println!("{HELP}");
        return Ok(ExitCode::SUCCESS);
    };
    let rest = &args[1..];
    let project_path = || PathBuf::from(positional(rest).cloned().unwrap_or_else(|| ".".into()));
    match cmd.as_str() {
        "help" | "--help" | "-h" => println!("{HELP}"),
        "--version" | "version" => println!("whetstone {}", env!("CARGO_PKG_VERSION")),
        "new" => {
            let dir = PathBuf::from(positional(rest).ok_or("new: which folder?")?);
            let name = dir.file_name().map(|n| n.to_string_lossy().to_string()).ok_or("new: give a folder name")?;
            let host = flag(rest, "--host").unwrap_or_else(|| "standalone".into());
            whetstone::new_project(&dir, &name, &host)?;
            println!("created {} — try: cd {} && whetstone run", dir.display(), dir.display());
        }
        "check" => {
            let (p, b) = compile(&project_path())?;
            println!("{} {}: {}", p.wtf.project.name, p.wtf.project.version, summary(&b));
            if !b.ok() {
                return Ok(ExitCode::from(1));
            }
        }
        "build" => {
            let (p, b) = compile(&project_path())?;
            if !b.ok() {
                println!("{}: {} — nothing written", p.wtf.project.name, summary(&b));
                return Ok(ExitCode::from(1));
            }
            let out = p.out_dir();
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
            let file = out.join(format!("{}.wsb", p.wtf.project.name));
            std::fs::write(&file, b.bundle.to_json()).map_err(|e| e.to_string())?;
            println!("{}: {} — {} ({} source files)", p.wtf.project.name, summary(&b), file.display(), b.bundle.sources.len());
        }
        "run" => {
            let target = project_path();
            let bundle = if target.extension().is_some_and(|x| x == "wsb") {
                Bundle::from_json(&std::fs::read_to_string(&target).map_err(|e| format!("{}: {e}", target.display()))?)?
            } else {
                let (_, b) = compile(&target)?;
                if !b.ok() {
                    eprintln!("not running: {}", summary(&b));
                    return Ok(ExitCode::from(1));
                }
                b.bundle
            };
            let opts = RunOptions { page_url: flag(rest, "--page"), page_title: flag(rest, "--title"), seed: flag(rest, "--seed").and_then(|s| s.parse().ok()) };
            let (prog, err) = match bundle.run(&opts) {
                Ok(p) => (Some(p), None),
                Err(b) => {
                    let (p, e) = *b;
                    (p, Some(e))
                }
            };
            if let Some(p) = &prog {
                print!("{}", p.output());
                if let Some(png) = flag(rest, "--png") {
                    let bytes = whetstone::canvas_png(p).ok_or("--png: the program left no `canvas` or `screen`")?;
                    std::fs::write(&png, bytes).map_err(|e| e.to_string())?;
                    eprintln!("saved {png}");
                }
            }
            if let Some(e) = err {
                eprintln!("error: {e}");
                return Ok(ExitCode::from(1));
            }
        }
        "install" => {
            let src = PathBuf::from(positional(rest).ok_or("install: which folder?")?);
            let m = whetstone::install(&src)?;
            println!("installed {} {} ({}) into {}", m.name, m.version, m.kind, whetstone::modules_dir().join(&m.name).display());
        }
        "uninstall" => {
            let n = positional(rest).ok_or("uninstall: which module?")?;
            whetstone::uninstall(n)?;
            println!("removed {n}");
        }
        "list" => {
            let all = whetstone::installed();
            if all.is_empty() {
                println!("nothing installed in {}", whetstone::modules_dir().display());
            }
            for m in all {
                println!("{:<16} {:<8} {:<9} {}", m.name, m.version, m.kind, m.description);
            }
        }
        "extenders" => {
            for e in whetstone::extenders() {
                println!("{:<8} {}", e.name(), e.about());
            }
        }
        other => return Err(format!("unknown command `{other}` (try `whetstone help`)")),
    }
    Ok(ExitCode::SUCCESS)
}
