//! **Whetstone** — the Zsharpend compiler.
//!
//! A project is a folder with a `.wtf` (Whetstone Translation File) that
//! says how to compile it (see [`wtf`]). Whetstone:
//!
//! 1. **reads** the `.wtf` and resolves what it names — script extenders,
//!    extensions, module folders, installed modules, the host;
//! 2. **parses** every file the program reaches (`.zxx`, and Z#++'s
//!    `.zsp`/`.zsx`/`.zsg`), following `import`s and `#include`s, and runs
//!    each extender's pass over the syntax tree;
//! 3. **checks** the whole program ([`check`]): undefined names,
//!    constants, arity, keyword arguments, enum variants and struct
//!    fields, printf formats, traits, headers — plus warnings;
//! 4. **links and bundles** it: every source it uses, with the project's
//!    settings, in one file (`build/<name>.wsb`, a Whetstone bundle) that
//!    runs without the project folder;
//! 5. **runs** it on the Zsharpend engine, with the extenders and host
//!    functions installed.

pub mod check;
pub mod wtf;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zsharpend::ast::{Item, Kind};
use zsharpend::extend::Extender;
use zsharpend::program::{module_candidates, resolve, Options};
use zsharpend::{Interp, Program, Value};

pub use check::{Diagnostic, Severity};

/// Script extenders built into this Whetstone.
pub fn extenders() -> Vec<Box<dyn Extender>> {
    vec![Box::new(zsharpend_nd::Nd), Box::new(zsharpend_3d::Gfx3d), Box::new(zsharpend_os::Os::from_process_args())]
}

fn find_extenders(names: &[String]) -> Result<Vec<Box<dyn Extender>>, String> {
    let all = extenders();
    let mut out: Vec<Box<dyn Extender>> = vec![];
    for n in names {
        match all.iter().position(|e| e.name() == n) {
            Some(_) => out.push(extenders().into_iter().find(|e| e.name() == n).unwrap()),
            None => {
                let have: Vec<String> = all.iter().map(|e| e.name().to_string()).collect();
                return Err(format!("there's no script extender `{n}` in this Whetstone (it has: {})", have.join(", ")));
            }
        }
    }
    Ok(out)
}

/// Where installed modules live: `$ZSHARPEND_HOME/modules`, or
/// `~/.zsharpend/modules`.
pub fn home() -> PathBuf {
    if let Some(h) = std::env::var_os("ZSHARPEND_HOME") {
        return PathBuf::from(h);
    }
    let base = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join(".zsharpend")
}

pub fn modules_dir() -> PathBuf {
    home().join("modules")
}

// ── Hosts ──────────────────────────────────────────────────────────────

/// What a host provides beyond the language: StonedGem's extension API.
pub fn host_globals(host: &str) -> Vec<&'static str> {
    match host {
        "stonedgem" => vec!["page_url", "page_title", "inject_css", "run_js", "notify", "store_get", "store_set", "panel_set"],
        _ => vec![],
    }
}

/// Stand-ins for a host's functions when Whetstone runs the program
/// itself (StonedGem's are printed as `[stonedgem] …`, so a script for the
/// browser can be tried from a terminal).
fn install_host(it: &mut Interp, host: &str, page: (String, String)) {
    if host != "stonedgem" {
        return;
    }
    let (url, title) = page;
    it.register("page_url", move |_, _| Ok(Value::str(&url)));
    it.register("page_title", move |_, _| Ok(Value::str(&title)));
    for name in ["inject_css", "run_js", "notify", "panel_set"] {
        it.register(name, move |it, a| {
            let args: Vec<String> = a.iter().map(|v| v.to_string()).collect();
            it.output.push(format!("[stonedgem] {name}({})\n", args.join(", ")));
            Ok(Value::Null)
        });
    }
    let store: std::rc::Rc<std::cell::RefCell<BTreeMap<String, Value>>> = Default::default();
    let s = store.clone();
    it.register("store_get", move |_, a| Ok(s.borrow().get(&a.first().map(|v| v.to_string()).unwrap_or_default()).cloned().unwrap_or(Value::Null)));
    it.register("store_set", move |_, a| {
        store.borrow_mut().insert(a.first().map(|v| v.to_string()).unwrap_or_default(), a.get(1).cloned().unwrap_or(Value::Null));
        Ok(Value::Null)
    });
}

// ── Projects ───────────────────────────────────────────────────────────

pub struct Project {
    pub dir: PathBuf,
    pub wtf_path: PathBuf,
    pub wtf: wtf::ProjectFile,
}

/// A source the program reads, by the path the program knows it by.
pub type Sources = BTreeMap<String, String>;

impl Project {
    /// Open the project in `path` (a folder holding one `.wtf`, or the `.wtf` itself).
    pub fn open(path: &Path) -> Result<Project, String> {
        let wtf_path = if path.is_dir() { wtf::find_in(path)? } else { path.to_path_buf() };
        let dir = wtf_path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
        let text = std::fs::read_to_string(&wtf_path).map_err(|e| format!("{}: {e}", wtf_path.display()))?;
        let name = wtf_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        Ok(Project { dir, wtf: wtf::parse_project(&text, &name)?, wtf_path })
    }

    /// The real file behind a path the program uses.
    fn real_path(&self, virt: &str) -> Option<PathBuf> {
        let module_file = |folder: PathBuf, rest: &str| -> PathBuf {
            // `mod.zxx` means the module's entry (from its module.wtf).
            if rest == "mod.zxx" {
                if let Some(entry) = std::fs::read_to_string(folder.join("module.wtf")).ok().and_then(|t| wtf::parse_module(&t, "module.wtf").ok()).map(|m| m.module.entry) {
                    return folder.join(entry);
                }
            }
            folder.join(rest)
        };
        if let Some(rest) = virt.strip_prefix("@installed/") {
            let (name, file) = rest.split_once('/')?;
            if !wtf::valid_name(name) {
                return None;
            }
            return Some(module_file(modules_dir().join(name), file));
        }
        if let Some(rest) = virt.strip_prefix("modules/") {
            let (name, file) = rest.split_once('/')?;
            let folder = match self.wtf.modules.get(name) {
                Some(p) => self.dir.join(p),
                None => self.dir.join("modules").join(name),
            };
            return Some(module_file(folder, file));
        }
        if virt.contains("..") || virt.starts_with('/') {
            return None;
        }
        Some(self.dir.join(virt))
    }

    /// Read a source by its program path.
    pub fn read(&self, virt: &str) -> Option<String> {
        std::fs::read_to_string(self.real_path(virt)?).ok()
    }

    /// The extensions' entry files, by program path.
    pub fn extension_paths(&self) -> Result<Vec<String>, String> {
        let mut out = vec![];
        for n in &self.wtf.extensions.names {
            let local = self.dir.join("extensions").join(n);
            let (folder, virt_base) = if local.is_dir() { (local, format!("extensions/{n}")) } else { (modules_dir().join(n), format!("@installed/{n}")) };
            let mf = std::fs::read_to_string(folder.join("module.wtf")).map_err(|_| format!("extension `{n}`: no extensions/{n}/module.wtf and none installed"))?;
            let m = wtf::parse_module(&mf, &format!("{n}/module.wtf"))?;
            if m.module.kind != "extension" {
                return Err(format!("`{n}` is a module, not an extension (import it instead)"));
            }
            out.push(format!("{virt_base}/{}", m.module.entry));
        }
        Ok(out)
    }

    /// Compile: parse, check, and gather every source the program uses.
    pub fn compile(&self) -> Result<Build, String> {
        let exts = find_extenders(&self.wtf.extenders.names)?;
        let ext_refs: Vec<&dyn Extender> = exts.iter().map(|b| b.as_ref()).collect();
        let extension_paths = self.extension_paths()?;
        let mut sources = Sources::new();
        let mut units = vec![];
        let mut gpu_names = vec![];
        // Names the program can see without defining them.
        let mut it = Interp::new();
        zsharpend::extend::install(&mut it, &ext_refs);
        let builtin_modules: Vec<String> = it.modules.keys().cloned().collect();
        let mut known: std::collections::HashSet<String> = it.globals.borrow().vars.keys().cloned().collect();
        known.extend(it.structs.keys().cloned());
        known.extend(ext_refs.iter().flat_map(|e| e.globals()));
        known.extend(host_globals(&self.wtf.project.host).into_iter().map(String::from));
        let mut gather = Gather { project: self, sources: &mut sources, units: &mut units, gpu_names: &mut gpu_names, builtin_modules: &builtin_modules, extenders: &ext_refs, seen: Default::default() };
        for p in &extension_paths {
            gather.file(p, None)?;
        }
        gather.file(&self.wtf.project.entry, None)?;
        let diagnostics = check::check(&units, &check::Known { globals: known }, &gpu_names);
        Ok(Build {
            bundle: Bundle {
                format: BUNDLE_FORMAT.into(),
                name: self.wtf.project.name.clone(),
                version: self.wtf.project.version.clone(),
                host: self.wtf.project.host.clone(),
                entry: self.wtf.project.entry.clone(),
                extenders: self.wtf.extenders.names.clone(),
                extensions: extension_paths,
                sources,
                whetstone: env!("CARGO_PKG_VERSION").into(),
            },
            diagnostics,
            strict: self.wtf.build.strict,
        })
    }

    pub fn out_dir(&self) -> PathBuf {
        self.dir.join(&self.wtf.build.out)
    }
}

struct Gather<'a> {
    project: &'a Project,
    sources: &'a mut Sources,
    units: &'a mut Vec<check::Unit>,
    gpu_names: &'a mut Vec<String>,
    builtin_modules: &'a [String],
    extenders: &'a [&'a dyn Extender],
    seen: std::collections::HashSet<String>,
}

impl Gather<'_> {
    fn file(&mut self, path: &str, module: Option<&str>) -> Result<(), String> {
        if !self.seen.insert(path.to_string()) {
            return Ok(());
        }
        let kind = Kind::from_path(path).ok_or(format!("{path}: Zsharpend files end in .zxx (or .zsp, .zsx, .zsg)"))?;
        let src = self.project.read(path).ok_or(format!("{path}: not found"))?;
        self.sources.insert(path.to_string(), src.clone());
        if kind == Kind::Gpu {
            let m = zsharpend::zsg::Module::parse(&src).map_err(|e| format!("{path}: {e}"))?;
            m.validate().map_err(|e| format!("{path}: {e}"))?;
            self.gpu_names.extend(m.entry_names());
            return Ok(());
        }
        let mut items = zsharpend::parser::parse(&src, kind).map_err(|e| format!("{path}: {e}"))?;
        if kind.is_program() {
            for e in self.extenders {
                e.transform(path, &mut items).map_err(|m| format!("{path}: {m} (extender `{}`)", e.name()))?;
            }
        }
        for it in &items {
            match it {
                Item::Import { path: p, line } => {
                    let full = resolve(path, p).map_err(|e| format!("{path}:{line}: {e}"))?;
                    self.file(&full, module)?;
                }
                Item::ImportModule { name, line, .. } if !self.builtin_modules.contains(name) => {
                    let found = module_candidates(path, name).into_iter().find(|c| self.project.read(c).is_some());
                    let file = found.ok_or(format!("{path}:{line}: there's no module `{name}` (not built in, not in modules/{name}/, not installed — `whetstone install` adds one)"))?;
                    self.file(&file, Some(name))?;
                }
                _ => {}
            }
        }
        self.units.push(check::Unit { path: path.into(), kind, items, module: module.map(String::from) });
        Ok(())
    }
}

// ── Builds and bundles ─────────────────────────────────────────────────

pub const BUNDLE_FORMAT: &str = "whetstone-bundle/1";

/// A compiled program: every source it uses and how to run it.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Bundle {
    pub format: String,
    pub name: String,
    pub version: String,
    pub host: String,
    pub entry: String,
    pub extenders: Vec<String>,
    pub extensions: Vec<String>,
    pub sources: Sources,
    pub whetstone: String,
}

pub struct Build {
    pub bundle: Bundle,
    pub diagnostics: Vec<Diagnostic>,
    pub strict: bool,
}

impl Build {
    pub fn errors(&self) -> usize {
        self.diagnostics.iter().filter(|d| d.severity == Severity::Error).count()
    }
    pub fn warnings(&self) -> usize {
        self.diagnostics.len() - self.errors()
    }
    /// Whether it may be bundled and run.
    pub fn ok(&self) -> bool {
        self.errors() == 0 && !(self.strict && self.warnings() > 0)
    }
}

impl Bundle {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn from_json(text: &str) -> Result<Bundle, String> {
        let b: Bundle = serde_json::from_str(text).map_err(|e| format!("not a Whetstone bundle: {e}"))?;
        if b.format != BUNDLE_FORMAT {
            return Err(format!("bundle format {} (this Whetstone reads {BUNDLE_FORMAT})", b.format));
        }
        Ok(b)
    }

    /// Load the program (it isn't run yet: call `run_main`).
    pub fn load(&self, opts: &RunOptions) -> Result<Program, String> {
        let exts = find_extenders(&self.extenders)?;
        let ext_refs: Vec<&dyn Extender> = exts.iter().map(|b| b.as_ref()).collect();
        let sources = self.sources.clone();
        let read = move |p: &str| sources.get(p).cloned();
        let host = self.host.clone();
        let page = (opts.page_url.clone().unwrap_or_else(|| "https://example.com/".into()), opts.page_title.clone().unwrap_or_else(|| "Example".into()));
        let seed = opts.seed;
        Program::load_with(
            &self.entry,
            &read,
            |it| {
                install_host(it, &host, page);
                if let Some(s) = seed {
                    it.seed(s);
                }
            },
            Options { extenders: &ext_refs, extensions: self.extensions.clone() },
        )
    }

    /// Run it: extensions' `on start`, `Main()`, host events, `on exit`.
    /// On failure: the program as far as it got (for its output) and the error.
    pub fn run(&self, opts: &RunOptions) -> Result<Program, Box<(Option<Program>, String)>> {
        let mut p = self.load(opts).map_err(|e| Box::new((None, e)))?;
        let r = (|| -> Result<(), String> {
            p.interp.emit("start", None, vec![])?;
            p.run_main()?;
            if self.host == "stonedgem" {
                if let Some(u) = &opts.page_url {
                    let title = opts.page_title.clone().unwrap_or_default();
                    p.interp.emit("page_load", Some(u), vec![Value::str(u), Value::str(&title)])?;
                }
            }
            p.interp.emit("exit", None, vec![])?;
            Ok(())
        })();
        match r {
            Ok(()) => Ok(p),
            Err(e) => Err(Box::new((Some(p), e))),
        }
    }
}

#[derive(Default, Clone)]
pub struct RunOptions {
    /// StonedGem host: the page `page_url()`/`page_title()` report, and a
    /// `page_load` event sent after `Main()`.
    pub page_url: Option<String>,
    pub page_title: Option<String>,
    pub seed: Option<u64>,
}

/// The canvas a program left in `canvas` or `screen`, as PNG.
pub fn canvas_png(p: &Program) -> Option<Vec<u8>> {
    ["canvas", "screen"].iter().find_map(|g| match p.interp.global(g) {
        Some(Value::Object(o)) => o.as_any().downcast_ref::<zsharpend::canvas::Canvas>().and_then(|c| c.png().ok()),
        _ => None,
    })
}

// ── Installing ─────────────────────────────────────────────────────────

/// Install a module or extension folder (with a `module.wtf`) into the
/// module folder. Returns its name.
pub fn install(src: &Path) -> Result<wtf::ModuleInfo, String> {
    let text = std::fs::read_to_string(src.join("module.wtf")).map_err(|_| format!("{}: no module.wtf (a module needs one)", src.display()))?;
    let m = wtf::parse_module(&text, "module.wtf")?;
    let entry = src.join(&m.module.entry);
    if !entry.exists() {
        return Err(format!("{}: its entry {} is missing", src.display(), m.module.entry));
    }
    // Check it parses before installing.
    let code = std::fs::read_to_string(&entry).map_err(|e| e.to_string())?;
    let kind = Kind::from_path(&m.module.entry).ok_or("the entry must be a .zxx file")?;
    zsharpend::parser::parse(&code, kind).map_err(|e| format!("{}: {e}", m.module.entry))?;
    let dest = modules_dir().join(&m.module.name);
    if dest.exists() {
        std::fs::remove_dir_all(&dest).map_err(|e| e.to_string())?;
    }
    copy_dir(src, &dest)?;
    Ok(m.module)
}

/// Installed modules and extensions.
pub fn installed() -> Vec<wtf::ModuleInfo> {
    let Ok(rd) = std::fs::read_dir(modules_dir()) else { return vec![] };
    let mut v: Vec<wtf::ModuleInfo> = rd
        .filter_map(|e| e.ok())
        .filter_map(|e| std::fs::read_to_string(e.path().join("module.wtf")).ok())
        .filter_map(|t| wtf::parse_module(&t, "module.wtf").ok().map(|m| m.module))
        .collect();
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

pub fn uninstall(name: &str) -> Result<(), String> {
    if !wtf::valid_name(name) {
        return Err(format!("`{name}` isn't a module name"));
    }
    let d = modules_dir().join(name);
    if !d.exists() {
        return Err(format!("`{name}` isn't installed"));
    }
    std::fs::remove_dir_all(d).map_err(|e| e.to_string())
}

fn copy_dir(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for e in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let e = e.map_err(|e| e.to_string())?;
        let (from, to) = (e.path(), dst.join(e.file_name()));
        let ft = e.file_type().map_err(|e| e.to_string())?;
        if ft.is_symlink() {
            continue; // never follow links out of the module
        }
        if ft.is_dir() {
            if e.file_name() == ".git" || e.file_name() == "build" {
                continue;
            }
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

// ── New projects ───────────────────────────────────────────────────────

/// Create a project folder `dir` named `name`.
pub fn new_project(dir: &Path, name: &str, host: &str) -> Result<(), String> {
    if !wtf::valid_name(name) {
        return Err(format!("a project name is letters, digits, - and _ (\"{name}\")"));
    }
    if dir.exists() && std::fs::read_dir(dir).map(|mut d| d.next().is_some()).unwrap_or(false) {
        return Err(format!("{} isn't empty", dir.display()));
    }
    std::fs::create_dir_all(dir.join("src")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(dir.join("modules")).map_err(|e| e.to_string())?;
    let wtf_text = format!(
        "# Whetstone Translation File: how to compile this project.\n[project]\nname = \"{name}\"\nversion = \"0.1.0\"\nentry = \"src/main.zxx\"\nhost = \"{host}\"\n\n[build]\nout = \"build\"\nstrict = false\n\n[extenders]\nuse = [\"nd\"]\n"
    );
    std::fs::write(dir.join(format!("{name}.wtf")), wtf_text).map_err(|e| e.to_string())?;
    let main = if host == "stonedgem" {
        "// A StonedGem extension script: runs in the browser on page loads.\non page_load \"*://*/*\" (url, title) {\n    notify(\"Opened ${title}\")\n}\n\nfn Main() {\n    Printl(\"try it: whetstone run --page https://example.com/\")\n}\n".to_string()
    } else {
        format!("import nd\n\nfn Main() {{\n    let xs = nd.linspace(0, 1, 5)\n    Printl(\"Hello from {name}!\", xs * 10)\n}}\n")
    };
    std::fs::write(dir.join("src/main.zxx"), main).map_err(|e| e.to_string())?;
    std::fs::write(dir.join(".gitignore"), "build/\n").map_err(|e| e.to_string())?;
    Ok(())
}
