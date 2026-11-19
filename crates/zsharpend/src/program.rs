//! Loading a program: its `import`s (other .zsp files, .zsx headers and
//! .zsg GPU modules) resolved relative to the importing file, through a
//! read function the host supplies — scripts never touch the file system
//! themselves. Headers are checked: every function a .zsx declares must be
//! defined by the program (with the same parameters and types) or provided
//! by the host.

use std::collections::HashSet;
use std::rc::Rc;

use crate::ast::*;
use crate::interp::Interp;

pub struct Program {
    pub interp: Interp,
    /// Files loaded, in order.
    pub files: Vec<String>,
}

/// `a/b/../c.zsx` relative to `from`'s folder, never above the root.
pub fn resolve(from: &str, path: &str) -> Result<String, String> {
    if path.starts_with('/') || path.contains('\\') || path.contains(':') {
        return Err(format!("import \"{path}\": use a relative path like \"lib/ui.zsx\""));
    }
    let mut parts: Vec<&str> = from.split('/').collect();
    parts.pop();
    for seg in path.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(format!("import \"{path}\": goes outside the program's folder"));
                }
            }
            s => parts.push(s),
        }
    }
    Ok(parts.join("/"))
}

#[derive(Default)]
struct Collected {
    items: Vec<Item>,
    sigs: Vec<(String, Rc<FuncDef>)>,
    gpu: Vec<Rc<crate::zsg::Module>>,
    seen: HashSet<String>,
    files: Vec<String>,
}

fn collect(path: &str, read: &dyn Fn(&str) -> Option<String>, c: &mut Collected, depth: usize) -> Result<(), String> {
    if depth > 32 {
        return Err("imports nest too deeply".into());
    }
    if !c.seen.insert(path.to_string()) {
        return Ok(());
    }
    let kind = Kind::from_path(path).ok_or(format!("{path}: Z#++ files end in .zsp, .zsx or .zsg"))?;
    let src = read(path).ok_or(format!("{path}: not found"))?;
    c.files.push(path.to_string());
    match kind {
        Kind::Gpu => {
            let m = crate::zsg::Module::parse(&src).map_err(|e| format!("{path}: {e}"))?;
            c.gpu.push(Rc::new(m));
        }
        Kind::Header | Kind::Program | Kind::Zxx => {
            let items = crate::parser::parse(&src, kind).map_err(|e| format!("{path}: {e}"))?;
            for it in items {
                match it {
                    Item::Import { path: p, line } => {
                        let full = resolve(path, &p).map_err(|e| format!("{path}, line {line}: {e}"))?;
                        collect(&full, read, c, depth + 1)?;
                    }
                    Item::Func(f) if f.body.is_none() => c.sigs.push((path.to_string(), f)),
                    other => c.items.push(other),
                }
            }
        }
    }
    Ok(())
}

fn ty_str(t: &Option<Type>) -> Option<String> {
    t.as_ref().map(|t| if t.array { format!("[{}]", t.name) } else { t.name.clone() })
}

impl Program {
    /// Load `entry` (a .zsp) and everything it imports. `setup` registers
    /// host functions/objects before the program runs.
    pub fn load(entry: &str, read: &dyn Fn(&str) -> Option<String>, setup: impl FnOnce(&mut Interp)) -> Result<Program, String> {
        if !Kind::from_path(entry).is_some_and(Kind::is_program) {
            return Err(format!("{entry}: a program is a .zxx or .zsp file"));
        }
        let mut c = Collected::default();
        collect(entry, read, &mut c, 0)?;
        let mut it = Interp::new();
        setup(&mut it);
        for m in &c.gpu {
            it.add_gpu_module(m.clone());
        }
        // Header check: declared → defined (same shape) or provided by the host.
        for (file, sig) in &c.sigs {
            let def = c.items.iter().find_map(|i| match i {
                Item::Func(f) if f.name == sig.name && f.body.is_some() => Some(f.clone()),
                _ => None,
            });
            match def {
                Some(f) => {
                    let ps = |d: &FuncDef| d.params.iter().filter(|p| p.name != "self").map(|p| (p.name.clone(), ty_str(&p.ty))).collect::<Vec<_>>();
                    let (want, have) = (ps(sig), ps(&f));
                    if want.len() != have.len() {
                        return Err(format!("`{}` is declared in {file} with {} parameter(s) but defined with {}", sig.name, want.len(), have.len()));
                    }
                    for ((wn, wt), (_, ht)) in want.iter().zip(have.iter()) {
                        if let (Some(w), Some(h)) = (wt, ht) {
                            if w != h {
                                return Err(format!("`{}`: parameter `{wn}` is {w} in {file} but {h} in the definition", sig.name));
                            }
                        }
                    }
                    if let (Some(w), Some(h)) = (ty_str(&sig.ret), ty_str(&f.ret)) {
                        if w != h {
                            return Err(format!("`{}` returns {w} in {file} but {h} in the definition", sig.name));
                        }
                    }
                }
                None => {
                    if !matches!(it.global(&sig.name), Some(crate::value::Value::Native(_))) {
                        return Err(format!("`{}` is declared in {file} but never defined (and the host doesn't provide it)", sig.name));
                    }
                }
            }
        }
        it.load(&c.items)?;
        Ok(Program { interp: it, files: c.files })
    }

    /// A single .zsp source with no imports.
    pub fn from_source(src: &str) -> Result<Program, String> {
        let s = src.to_string();
        Program::load("main.zsp", &move |p: &str| if p == "main.zsp" { Some(s.clone()) } else { None }, |_| {})
    }

    /// A single `.zxx` (Zsharpend) source with no imports.
    pub fn from_zxx(src: &str) -> Result<Program, String> {
        let s = src.to_string();
        Program::load("main.zxx", &move |p: &str| if p == "main.zxx" { Some(s.clone()) } else { None }, |_| {})
    }

    /// Run `Main()` if the program defines it (Z#'s entry point).
    pub fn run_main(&mut self) -> Result<(), String> {
        self.interp.call_global("Main", vec![]).map(|_| ())
    }

    /// Everything printed so far.
    pub fn output(&self) -> String {
        self.interp.output.concat()
    }
}
