//! `zsharpend-docs [OUT]` — build the documentation site (default:
//! `target/zsharpend-docs`).

use std::path::{Path, PathBuf};

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let out = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| root.join("target/zsharpend-docs"));
    match zsharpend_docs::build(&root, &out) {
        Ok(r) => println!(
            "built {} pages into {} ({} examples run, {} checked, {} index entries)",
            r.pages,
            out.display(),
            r.examples_run,
            r.examples_checked,
            r.index_entries
        ),
        Err(e) => {
            eprintln!("zsharpend-docs: {e}");
            std::process::exit(1);
        }
    }
}
