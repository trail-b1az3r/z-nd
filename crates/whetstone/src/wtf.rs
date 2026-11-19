//! `.wtf` — the **Whetstone Translation File**. Every Zsharpend project has
//! one: it's what Whetstone needs to compile the project's `.zxx` files —
//! the entry, the host it's built for, the script extenders and extensions
//! it uses, where its modules are, and how strict the build is. Installable
//! modules and extensions carry one too (`module.wtf`).
//!
//! ```toml
//! [project]
//! name = "orbit"
//! version = "0.1.0"
//! entry = "src/main.zxx"
//! host = "standalone"          # or "stonedgem"
//!
//! [build]
//! out = "build"
//! strict = false               # warnings fail the build when true
//!
//! [extenders]
//! use = ["nd", "gfx3d"]
//!
//! [extensions]
//! use = ["timing"]             # extensions/timing/ or an installed one
//!
//! [modules]
//! shared = "../shared/geometry"   # extra module folders (name = path)
//! ```

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ProjectFile {
    pub project: Project,
    #[serde(default)]
    pub build: Build,
    #[serde(default)]
    pub extenders: Uses,
    #[serde(default)]
    pub extensions: Uses,
    /// Module name → folder (relative to the project).
    #[serde(default)]
    pub modules: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default = "default_entry")]
    pub entry: String,
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Build {
    #[serde(default = "default_out")]
    pub out: String,
    #[serde(default)]
    pub strict: bool,
}

impl Default for Build {
    fn default() -> Self {
        Build { out: default_out(), strict: false }
    }
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct Uses {
    #[serde(default, rename = "use")]
    pub names: Vec<String>,
}

/// `module.wtf` in an installable module or extension.
#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ModuleFile {
    pub module: ModuleInfo,
    #[serde(default)]
    pub extenders: Uses,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ModuleInfo {
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    /// `"module"` (imported by name) or `"extension"` (hooks into programs).
    #[serde(default = "default_kind")]
    pub kind: String,
    /// The file `import name` loads (or the extension's file).
    #[serde(default = "default_module_entry")]
    pub entry: String,
    #[serde(default)]
    pub description: String,
}

fn default_version() -> String {
    "0.1.0".into()
}
fn default_entry() -> String {
    "src/main.zxx".into()
}
fn default_host() -> String {
    "standalone".into()
}
fn default_out() -> String {
    "build".into()
}
fn default_kind() -> String {
    "module".into()
}
fn default_module_entry() -> String {
    "mod.zxx".into()
}

pub const HOSTS: &[&str] = &["standalone", "stonedgem"];

pub fn parse_project(text: &str, file: &str) -> Result<ProjectFile, String> {
    let p: ProjectFile = toml::from_str(text).map_err(|e| format!("{file}: {e}"))?;
    if !HOSTS.contains(&p.project.host.as_str()) {
        return Err(format!("{file}: host must be one of {}, not \"{}\"", HOSTS.join(", "), p.project.host));
    }
    if !p.project.entry.ends_with(".zxx") && !p.project.entry.ends_with(".zsp") {
        return Err(format!("{file}: the entry is a .zxx (or .zsp) file, not \"{}\"", p.project.entry));
    }
    if !valid_name(&p.project.name) {
        return Err(format!("{file}: a project name is letters, digits, - and _ (\"{}\")", p.project.name));
    }
    Ok(p)
}

pub fn parse_module(text: &str, file: &str) -> Result<ModuleFile, String> {
    let m: ModuleFile = toml::from_str(text).map_err(|e| format!("{file}: {e}"))?;
    if !matches!(m.module.kind.as_str(), "module" | "extension") {
        return Err(format!("{file}: kind is \"module\" or \"extension\", not \"{}\"", m.module.kind));
    }
    if !valid_name(&m.module.name) {
        return Err(format!("{file}: a module name is letters, digits and _ (\"{}\")", m.module.name));
    }
    Ok(m)
}

pub fn valid_name(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') && !s.starts_with(|c: char| c.is_ascii_digit())
}

/// The one `.wtf` in `dir` (other than `module.wtf`).
pub fn find_in(dir: &Path) -> Result<std::path::PathBuf, String> {
    let mut found: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "wtf") && p.file_name().is_some_and(|n| n != "module.wtf"))
        .collect();
    found.sort();
    match found.len() {
        0 => Err(format!("{}: no .wtf file (create one with `whetstone new`)", dir.display())),
        1 => Ok(found.remove(0)),
        _ => Err(format!("{}: more than one .wtf file; pass the one to use", dir.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_checks() {
        let p = parse_project("[project]\nname = \"orbit\"\n\n[extenders]\nuse = [\"nd\"]\n", "orbit.wtf").unwrap();
        assert_eq!(p.project.entry, "src/main.zxx");
        assert_eq!(p.extenders.names, vec!["nd"]);
        assert!(parse_project("[project]\nname = \"x\"\nhost = \"mars\"\n", "x.wtf").unwrap_err().contains("host must be"));
        assert!(parse_project("[project]\nname = \"x\"\ncolour = 1\n", "x.wtf").unwrap_err().contains("colour"));
        let m = parse_module("[module]\nname = \"geometry\"\n", "module.wtf").unwrap();
        assert_eq!((m.module.kind.as_str(), m.module.entry.as_str()), ("module", "mod.zxx"));
    }
}
