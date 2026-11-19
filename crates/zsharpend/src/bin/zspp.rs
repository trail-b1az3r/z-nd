//! `zspp` — run and check Z#++ files from a terminal.
//!
//! ```text
//! zspp run app.zsp [--png out.png]   run Main(); save the `screen`/`canvas` canvas
//! zspp check file.zsp|.zsx|.zsg      parse and check (headers, GPU types)
//! zspp wgsl shaders.zsg              print the validated WGSL
//! ```

use std::path::{Path, PathBuf};

use zsharpend::ast::Kind;
use zsharpend::{Program, Value};

fn usage() -> ! {
    eprintln!("usage: zspp run <file.zsp> [--png <out.png>] | zspp check <file> | zspp wgsl <file.zsg>");
    std::process::exit(2)
}

/// Files are read relative to the entry file's folder.
fn reader(root: PathBuf) -> impl Fn(&str) -> Option<String> {
    move |p: &str| std::fs::read_to_string(root.join(p)).ok()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(cmd), Some(file)) = (args.first(), args.get(1)) else { usage() };
    let path = Path::new(file);
    let root = path.parent().map(Path::to_path_buf).unwrap_or_default();
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or(file).to_string();
    let result: Result<(), String> = (|| match cmd.as_str() {
        "run" => {
            let mut p = Program::load(&name, &reader(root.clone()), |_| {})?;
            let r = p.run_main();
            print!("{}", p.output());
            r?;
            if let Some(i) = args.iter().position(|a| a == "--png") {
                let out = args.get(i + 1).ok_or("--png needs a file name")?;
                let canvas = ["screen", "canvas"].iter().find_map(|g| match p.interp.global(g) {
                    Some(Value::Object(o)) if o.type_name() == "Canvas" => Some(o),
                    _ => None,
                });
                let o = canvas.ok_or("no `screen` or `canvas` canvas to save")?;
                let c = o.as_any().downcast_ref::<zsharpend::canvas::Canvas>().unwrap();
                std::fs::write(out, c.png()?).map_err(|e| e.to_string())?;
                eprintln!("saved {out}");
            }
            Ok(())
        }
        "check" => {
            match Kind::from_path(&name) {
                Some(Kind::Program | Kind::Zxx) => {
                    Program::load(&name, &reader(root.clone()), |_| {})?;
                }
                Some(Kind::Header) => {
                    zsharpend::parser::parse(&std::fs::read_to_string(path).map_err(|e| e.to_string())?, Kind::Header)?;
                }
                Some(Kind::Gpu) => {
                    zsharpend::zsg::Module::parse(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)?.validate()?;
                }
                None => return Err(format!("{name}: not a .zsp, .zsx or .zsg file")),
            }
            println!("{name}: ok");
            Ok(())
        }
        "wgsl" => {
            let m = zsharpend::zsg::Module::parse(&std::fs::read_to_string(path).map_err(|e| e.to_string())?)?;
            print!("{}", m.validate()?);
            Ok(())
        }
        _ => usage(),
    })();
    if let Err(e) = result {
        eprintln!("zspp: {e}");
        std::process::exit(1);
    }
}
