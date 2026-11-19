//! Loading a program: its `import`s (other .zsp files, .zsx headers and
//! .zsg GPU modules) resolved relative to the importing file, through a
//! read function the host supplies — scripts never touch the file system
//! themselves. Headers are checked: every function a .zsx declares must be
//! defined by the program (with the same parameters and types) or provided
//! by the host.

use std::collections::HashSet;
use std::rc::Rc;

use crate::ast::*;
use crate::extend::Extender;
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
    /// `.zxx` modules by name, dependencies first.
    modules: Vec<(String, Vec<Item>)>,
    module_names: HashSet<String>,
}

/// Where `import name` looks for a `.zxx` module, in order: next to the
/// importing file, in the project's `modules/` folder, then among installed
/// modules (`@installed/…`, which the host maps to its module folder).
pub fn module_candidates(from: &str, name: &str) -> Vec<String> {
    let mut v = vec![];
    if let Ok(p) = resolve(from, &format!("{name}.zxx")) {
        v.push(p);
    }
    v.push(format!("modules/{name}/mod.zxx"));
    v.push(format!("@installed/{name}/mod.zxx"));
    v
}

struct Ctx<'a> {
    read: &'a dyn Fn(&str) -> Option<String>,
    extenders: &'a [&'a dyn Extender],
    builtin_modules: HashSet<String>,
}

fn collect(path: &str, cx: &Ctx, c: &mut Collected, out: &mut Vec<Item>, depth: usize) -> Result<(), String> {
    if depth > 32 {
        return Err("imports nest too deeply".into());
    }
    if !c.seen.insert(path.to_string()) {
        return Ok(());
    }
    let kind = Kind::from_path(path).ok_or(format!("{path}: Zsharpend files end in .zxx (or Z#++'s .zsp, .zsx, .zsg)"))?;
    let src = (cx.read)(path).ok_or(format!("{path}: not found"))?;
    c.files.push(path.to_string());
    match kind {
        Kind::Gpu => {
            let m = crate::zsg::Module::parse(&src).map_err(|e| format!("{path}: {e}"))?;
            c.gpu.push(Rc::new(m));
        }
        Kind::Header | Kind::Program | Kind::Zxx => {
            let mut items = crate::parser::parse(&src, kind).map_err(|e| format!("{path}: {e}"))?;
            if kind.is_program() {
                for e in cx.extenders {
                    e.transform(path, &mut items).map_err(|m| format!("{path}: {} (extender `{}`)", m, e.name()))?;
                }
            }
            for it in items {
                match it {
                    Item::Import { path: p, line } => {
                        let full = resolve(path, &p).map_err(|e| format!("{path}, line {line}: {e}"))?;
                        collect(&full, cx, c, out, depth + 1)?;
                    }
                    Item::ImportModule { ref name, line, .. } => {
                        if !cx.builtin_modules.contains(name) && !c.module_names.contains(name) {
                            let file = module_candidates(path, name).into_iter().find(|p| (cx.read)(p).is_some()).ok_or(format!(
                                "{path}, line {line}: there's no module `{name}` (not built in, not in modules/{name}/, not installed)"
                            ))?;
                            c.module_names.insert(name.clone());
                            let mut m = vec![];
                            collect(&file, cx, c, &mut m, depth + 1)?;
                            c.modules.push((name.clone(), m));
                        }
                        out.push(it);
                    }
                    Item::Func(f) if f.body.is_none() => c.sigs.push((path.to_string(), f)),
                    other => out.push(other),
                }
            }
        }
    }
    Ok(())
}

/// How to load a program beyond its entry file.
#[derive(Default)]
pub struct Options<'a> {
    /// Script extenders to install (and to run over each file).
    pub extenders: &'a [&'a dyn Extender],
    /// Extensions: `.zxx` files merged into the program before the entry
    /// (their functions and `on` handlers become the program's).
    pub extensions: Vec<String>,
}

fn ty_str(t: &Option<Type>) -> Option<String> {
    t.as_ref().map(|t| if t.array { format!("[{}]", t.name) } else { t.name.clone() })
}

impl Program {
    /// Load `entry` (a .zsp) and everything it imports. `setup` registers
    /// host functions/objects before the program runs.
    pub fn load(entry: &str, read: &dyn Fn(&str) -> Option<String>, setup: impl FnOnce(&mut Interp)) -> Result<Program, String> {
        Program::load_with(entry, read, setup, Options::default())
    }

    /// [`Program::load`] with script extenders and extensions.
    pub fn load_with(entry: &str, read: &dyn Fn(&str) -> Option<String>, setup: impl FnOnce(&mut Interp), opts: Options) -> Result<Program, String> {
        if !Kind::from_path(entry).is_some_and(Kind::is_program) {
            return Err(format!("{entry}: a program is a .zxx or .zsp file"));
        }
        let mut it = Interp::new();
        crate::extend::install(&mut it, opts.extenders);
        setup(&mut it);
        let cx = Ctx { read, extenders: opts.extenders, builtin_modules: it.modules.keys().cloned().collect() };
        let mut c = Collected::default();
        let mut items = vec![];
        for ext in &opts.extensions {
            collect(ext, &cx, &mut c, &mut items, 0)?;
        }
        collect(entry, &cx, &mut c, &mut items, 0)?;
        c.items = items;
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
        for (name, items) in &c.modules {
            it.load_module(name, items)?;
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
