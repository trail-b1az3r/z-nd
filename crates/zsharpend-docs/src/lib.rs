//! Builds the Zsharpend documentation: Markdown in `docs/site/`, ordered by
//! `docs/site/nav.txt`, laid out the way Python's documentation is — a
//! related-links bar with breadcrumbs and index/modules/next/previous, a
//! sidebar with the page's contents and the neighbouring topics,
//! function signatures with permalinks, a module index, a general index
//! and search — and with every example run when the site is built.
//!
//! Directives (on their own lines):
//!
//! ```text
//! ::: module nd — n-dimensional arrays     (the page documents module `nd`)
//! ::: function nd.zeros(shape) -> ndarray  (a signature; body until `:::`)
//! ::: method ndarray.sum(axis=None)
//! ::: class Mesh
//! ::: data nd.pi
//! ::: note | ::: warning | ::: seealso     (boxes; body until `:::`)
//! :::
//! ```
//!
//! Code blocks: `zxx` (parsed), `zxx run` (run; output and canvas shown),
//! `zxx error` (must fail; the error shown), `zxx whetstone` (Whetstone's
//! check output for it as a one-file project), `zsp`/`zsx`/`zsg`, `toml`,
//! `rust`, `sh`, `text`; `include=path` takes the code from the repository.

pub mod highlight;

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use highlight::escape;
use pulldown_cmark::{CodeBlockKind, CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use zsharpend::ast::Kind;
use zsharpend::extend::Extender;
use zsharpend::program::Options as LoadOptions;
use zsharpend::Program;
#[cfg(test)]
use zsharpend::Value;

const STYLE: &str = include_str!("../assets/style.css");
const SCRIPT: &str = include_str!("../assets/site.js");
const SOURCE: &str = "https://github.com/trail-b1az3r/z-nd/blob/main/";
const VERSION: &str = "0.1";

pub struct Page {
    pub file: String,
    pub section: String,
    pub title: String,
    pub md: String,
}

impl Page {
    pub fn out(&self) -> String {
        self.file.trim_end_matches(".md").to_string() + ".html"
    }
}

#[derive(Default, Debug)]
pub struct Report {
    pub pages: usize,
    pub examples_run: usize,
    pub examples_checked: usize,
    pub index_entries: usize,
}

pub fn load_pages(root: &Path) -> Result<Vec<Page>, String> {
    let dir = root.join("docs/site");
    let nav = std::fs::read_to_string(dir.join("nav.txt")).map_err(|e| format!("docs/site/nav.txt: {e}"))?;
    let mut section = String::new();
    let mut pages = vec![];
    for line in nav.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some(s) = line.strip_prefix("# ") {
            section = s.to_string();
            continue;
        }
        let md = std::fs::read_to_string(dir.join(line)).map_err(|e| format!("docs/site/{line}: {e}"))?;
        let title = md.lines().find_map(|l| l.strip_prefix("# ")).ok_or(format!("docs/site/{line}: no `# Title`"))?.trim().to_string();
        pages.push(Page { file: line.to_string(), section: section.clone(), title, md });
    }
    Ok(pages)
}

// ── Examples ───────────────────────────────────────────────────────────

struct Ex<'a> {
    lang: &'a str,
    run: bool,
    error: bool,
    whetstone: bool,
    include: Option<String>,
}

fn parse_info(info: &str) -> Ex<'_> {
    let mut w = info.split_whitespace();
    let lang = w.next().unwrap_or("");
    let mut ex = Ex { lang, run: false, error: false, whetstone: false, include: None };
    for x in w {
        match x {
            "run" => ex.run = true,
            "error" => ex.error = true,
            "whetstone" => ex.whetstone = true,
            x if x.starts_with("include=") => ex.include = Some(x["include=".len()..].into()),
            _ => {}
        }
    }
    ex
}

struct Ran {
    output: String,
    png: Option<Vec<u8>>,
    error: Option<String>,
    label: &'static str,
}

/// The project (folder with a `.wtf`) a repository file belongs to.
fn project_of(file: &Path) -> Option<PathBuf> {
    let mut d = file.parent()?;
    loop {
        if whetstone::wtf::find_in(d).is_ok() {
            return Some(d.to_path_buf());
        }
        d = d.parent()?;
    }
}

fn run_program(root: &Path, ex: &Ex, code: &str) -> Result<Program, String> {
    // A file from an example project runs as Whetstone runs it.
    if let Some(p) = &ex.include {
        if let Some(dir) = project_of(&root.join(p)) {
            let b = whetstone::Project::open(&dir)?.compile()?;
            if !b.ok() {
                return Err(b.diagnostics.iter().map(|d| d.to_string()).collect::<Vec<_>>().join("\n"));
            }
            return b.bundle.run(&whetstone::RunOptions { seed: Some(42), ..Default::default() }).map_err(|e| e.1);
        }
    }
    let (name, dir) = match &ex.include {
        Some(p) => {
            let f = root.join(p);
            (f.file_name().and_then(|n| n.to_str()).unwrap_or("main.zxx").to_string(), f.parent().map(Path::to_path_buf).unwrap_or_default())
        }
        None => (format!("main.{}", ex.lang), PathBuf::new()),
    };
    let (code, n2) = (code.to_string(), name.clone());
    let read = move |p: &str| if p == n2 { Some(code.clone()) } else { std::fs::read_to_string(dir.join(p)).ok() };
    let exts = whetstone::extenders();
    let refs: Vec<&dyn Extender> = exts.iter().map(|b| b.as_ref()).collect();
    let mut p = Program::load_with(&name, &read, |it| it.seed(42), LoadOptions { extenders: &refs, ..Default::default() })?;
    match p.run_main() {
        Ok(()) => Ok(p),
        Err(e) => Err(format!("{}{e}", p.output())),
    }
}

fn check_example(root: &Path, ex: &Ex, code: &str, r: &mut Report) -> Result<Option<Ran>, String> {
    if ex.whetstone {
        // Whetstone's own diagnostics for this code as a one-file project.
        r.examples_checked += 1;
        let dir = std::env::temp_dir().join(format!("zsharpend-docs-{}-{}", std::process::id(), r.examples_checked));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("src")).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("example.wtf"), "[project]\nname = \"example\"\n\n[extenders]\nuse = [\"nd\", \"gfx3d\"]\n").map_err(|e| e.to_string())?;
        std::fs::write(dir.join("src/main.zxx"), code).map_err(|e| e.to_string())?;
        let b = whetstone::Project::open(&dir)?.compile()?;
        let _ = std::fs::remove_dir_all(&dir);
        let mut out: Vec<String> = b.diagnostics.iter().map(|d| d.to_string()).collect();
        out.push(format!("example 0.1.0: {} error(s), {} warning(s)", b.errors(), b.warnings()));
        return Ok(Some(Ran { output: out.join("\n"), png: None, error: None, label: "whetstone check" }));
    }
    match ex.lang {
        "zxx" | "zsp" if ex.run || ex.error => {
            r.examples_run += 1;
            match (run_program(root, ex, code), ex.error) {
                (Ok(p), false) => Ok(Some(Ran { output: p.output(), png: whetstone::canvas_png(&p), error: None, label: "Output" })),
                (Err(e), true) => Ok(Some(Ran { output: String::new(), png: None, error: Some(e), label: "Error" })),
                (Ok(_), true) => Err("marked `error` but it runs".into()),
                (Err(e), false) => Err(e),
            }
        }
        "zxx" | "zsp" | "zsx" => {
            r.examples_checked += 1;
            let kind = match ex.lang {
                "zxx" => Kind::Zxx,
                "zsx" => Kind::Header,
                _ => Kind::Program,
            };
            zsharpend::parser::parse(code, kind).map(|_| None)
        }
        "zsg" => {
            r.examples_checked += 1;
            zsharpend::zsg::Module::parse(code)?.validate().map(|_| None)
        }
        _ => Ok(None),
    }
}

// ── Rendering ──────────────────────────────────────────────────────────

fn slug(s: &str) -> String {
    let mut o = String::new();
    for c in s.chars() {
        if c.is_alphanumeric() || c == '.' || c == '_' {
            o.extend(c.to_lowercase());
        } else if (c == ' ' || c == '-') && !o.ends_with('-') {
            o.push('-');
        }
    }
    o.trim_matches('-').to_string()
}

/// An entry in the general index.
#[derive(Clone)]
pub struct IndexEntry {
    pub name: String,
    pub what: String,
    pub url: String,
}

struct Ctx<'a> {
    root: &'a Path,
    page: &'a Page,
    report: &'a mut Report,
    images: Vec<(String, Vec<u8>)>,
    toc: Vec<(usize, String, String)>,
    index: Vec<IndexEntry>,
    module: Option<(String, String)>,
    used: HashMap<String, usize>,
    figure: usize,
    text: String,
}

fn md_to_html(cx: &mut Ctx, md: &str) -> Result<String, String> {
    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
    let mut events: Vec<Event> = vec![];
    let mut code: Option<(String, String)> = None;
    let mut heading: Option<(HeadingLevel, Vec<Event>)> = None;
    for ev in Parser::new_ext(md, opts) {
        if let Some((_, buf)) = &mut code {
            match ev {
                Event::Text(t) => buf.push_str(&t),
                Event::End(TagEnd::CodeBlock) => {
                    let (info, text) = code.take().unwrap();
                    let ex = parse_info(&info);
                    let text = match &ex.include {
                        Some(p) => std::fs::read_to_string(cx.root.join(p)).map_err(|e| format!("{}: include={p}: {e}", cx.page.file))?,
                        None => text,
                    };
                    let ran = check_example(cx.root, &ex, &text, cx.report).map_err(|e| format!("{}: example `{}`: {e}", cx.page.file, text.lines().next().unwrap_or("")))?;
                    let body = match ex.lang {
                        "zxx" | "zsp" | "zsx" | "zsg" => highlight::zsharp(&text),
                        "rust" => highlight::rust(&text),
                        "toml" => highlight::toml(&text),
                        _ => escape(&text),
                    };
                    let mut html = String::from("<div class=\"highlight\">");
                    if let Some(p) = &ex.include {
                        html.push_str(&format!("<div class=\"code-file\"><a href=\"{SOURCE}{p}\">{}</a></div>", escape(p)));
                    }
                    html.push_str(&format!("<button class=\"copybtn\" type=\"button\" title=\"Copy\">Copy</button><pre class=\"lang-{}\"><code>{}</code></pre>", escape(ex.lang), body.trim_end()));
                    if let Some(r) = ran {
                        match r.error {
                            Some(e) => html.push_str(&format!("<div class=\"result err\"><span>Error</span><pre>{}</pre></div>", escape(e.trim_end()))),
                            None => {
                                if !r.output.is_empty() {
                                    html.push_str(&format!("<div class=\"result\"><span>{}</span><pre>{}</pre></div>", r.label, escape(r.output.trim_end())));
                                }
                                if let Some(png) = r.png {
                                    cx.figure += 1;
                                    let f = format!("_images/{}-{}.png", cx.page.file.trim_end_matches(".md"), cx.figure);
                                    html.push_str(&format!("<div class=\"result canvas\"><span>Canvas</span><img src=\"{f}\" alt=\"What this program draws\" loading=\"lazy\"></div>"));
                                    cx.images.push((f, png));
                                }
                            }
                        }
                    }
                    html.push_str("</div>");
                    events.push(Event::Html(CowStr::from(html)));
                }
                _ => {}
            }
            continue;
        }
        if let Some((level, buf)) = &mut heading {
            if let Event::End(TagEnd::Heading(_)) = ev {
                let level = *level;
                let text: String = buf.iter().filter_map(|e| if let Event::Text(t) | Event::Code(t) = e { Some(t.to_string()) } else { None }).collect();
                let mut inner = String::new();
                pulldown_cmark::html::push_html(&mut inner, buf.drain(..));
                heading = None;
                let base = slug(&text);
                let n = cx.used.entry(base.clone()).or_insert(0);
                let id = if *n == 0 { base.clone() } else { format!("{base}-{n}") };
                *n += 1;
                let lv = level as usize;
                if lv >= 2 {
                    cx.toc.push((lv, id.clone(), text.clone()));
                }
                cx.text.push_str(&text);
                cx.text.push(' ');
                events.push(Event::Html(CowStr::from(format!("<h{lv} id=\"{id}\">{inner}<a class=\"headerlink\" href=\"#{id}\" title=\"Link to this heading\">¶</a></h{lv}>"))));
            } else {
                buf.push(ev);
            }
            continue;
        }
        match ev {
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => code = Some((info.to_string(), String::new())),
            Event::Start(Tag::CodeBlock(CodeBlockKind::Indented)) => code = Some(("text".into(), String::new())),
            Event::Start(Tag::Heading { level, .. }) => heading = Some((level, vec![])),
            Event::Start(Tag::Link { link_type, dest_url, title, id }) => {
                let d = dest_url.to_string();
                let dest = if !d.contains("://") && d.contains(".md") { d.replacen(".md", ".html", 1) } else { d };
                events.push(Event::Start(Tag::Link { link_type, dest_url: CowStr::from(dest), title, id }));
            }
            Event::Code(t) => {
                cx.text.push_str(&t);
                cx.text.push(' ');
                events.push(Event::Html(CowStr::from(format!("<code class=\"literal\">{}</code>", escape(&t)))));
            }
            Event::Text(t) => {
                cx.text.push_str(&t);
                cx.text.push(' ');
                events.push(Event::Text(t));
            }
            ev => events.push(ev),
        }
    }
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, events.into_iter());
    Ok(html)
}

/// Split out `:::` directives, render each piece.
fn render(cx: &mut Ctx, md: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut plain = String::new();
    let lines: Vec<&str> = md.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i].trim_end();
        let Some(d) = l.strip_prefix("::: ") else {
            plain.push_str(lines[i]);
            plain.push('\n');
            i += 1;
            continue;
        };
        out.push_str(&md_to_html(cx, &plain)?);
        plain.clear();
        let (kind, arg) = d.split_once(' ').unwrap_or((d, ""));
        if kind == "module" {
            let (name, synopsis) = arg.split_once(" — ").unwrap_or((arg, ""));
            cx.module = Some((name.trim().into(), synopsis.trim().into()));
            cx.index.push(IndexEntry { name: name.trim().into(), what: "module".into(), url: format!("{}#module-{}", cx.page.out(), name.trim()) });
            out.push_str(&format!("<span id=\"module-{}\"></span>", escape(name.trim())));
            i += 1;
            continue;
        }
        // Body: until a line that is exactly `:::`.
        let mut body = String::new();
        i += 1;
        while i < lines.len() && lines[i].trim() != ":::" {
            body.push_str(lines[i]);
            body.push('\n');
            i += 1;
        }
        if i >= lines.len() {
            return Err(format!("{}: `::: {kind}` never closed with `:::`", cx.page.file));
        }
        i += 1;
        let inner = render(cx, &body)?;
        match kind {
            "function" | "method" | "class" | "data" => {
                let name: String = arg.split(['(', ' ']).next().unwrap_or(arg).to_string();
                let id = name.clone();
                let (prefix, rest) = match name.rsplit_once('.') {
                    Some((p, n)) => (format!("{p}."), n.to_string()),
                    None => (String::new(), name.clone()),
                };
                let sig_tail = &arg[name.len()..];
                let label = match kind {
                    "class" => "<em class=\"property\">class </em>",
                    "method" => "",
                    _ => "",
                };
                out.push_str(&format!(
                    "<dl class=\"{kind}\"><dt id=\"{id}\">{label}<code class=\"descclassname\">{}</code><code class=\"descname\">{}</code><code class=\"sig\">{}</code><a class=\"headerlink\" href=\"#{id}\" title=\"Link to this definition\">¶</a></dt><dd>{inner}</dd></dl>",
                    escape(&prefix),
                    escape(&rest),
                    highlight::zsharp(sig_tail)
                ));
                let what = match (kind, prefix.trim_end_matches('.')) {
                    ("function", "") => "built-in function".to_string(),
                    ("function", m) => format!("in module {m}"),
                    ("method", t) => format!("{t} method"),
                    ("class", m) if !m.is_empty() => format!("class in {m}"),
                    ("class", _) => "class".into(),
                    (_, m) if !m.is_empty() => format!("in module {m}"),
                    _ => kind.to_string(),
                };
                let shown = if matches!(kind, "function" | "method") { format!("{rest}()") } else { rest.clone() };
                cx.index.push(IndexEntry { name: shown, what, url: format!("{}#{id}", cx.page.out()) });
                cx.text.push_str(&name);
                cx.text.push(' ');
            }
            "note" | "warning" | "seealso" | "tip" => {
                let title = match kind {
                    "note" => "Note",
                    "warning" => "Warning",
                    "seealso" => "See also",
                    _ => "Tip",
                };
                out.push_str(&format!("<div class=\"admonition {kind}\"><p class=\"admonition-title\">{title}</p>{inner}</div>"));
            }
            other => return Err(format!("{}: unknown directive `::: {other}`", cx.page.file)),
        }
    }
    out.push_str(&md_to_html(cx, &plain)?);
    Ok(out)
}

// ── Pages ──────────────────────────────────────────────────────────────

struct Rendered {
    html: String,
    toc: Vec<(usize, String, String)>,
    images: Vec<(String, Vec<u8>)>,
    index: Vec<IndexEntry>,
    module: Option<(String, String)>,
    text: String,
}

fn toc_html(toc: &[(usize, String, String)]) -> String {
    let mut o = String::from("<ul>");
    let mut open_sub = false;
    for (lv, id, t) in toc {
        if *lv == 2 {
            if open_sub {
                o.push_str("</ul></li>");
                open_sub = false;
            } else if o.ends_with("</a>") {
                o.push_str("</li>");
            }
            o.push_str(&format!("<li><a href=\"#{id}\">{}</a>", escape(t)));
        } else if *lv == 3 {
            if !open_sub {
                o.push_str("<ul>");
                open_sub = true;
            }
            o.push_str(&format!("<li><a href=\"#{id}\">{}</a></li>", escape(t)));
        }
    }
    if open_sub {
        o.push_str("</ul></li>");
    } else if o.ends_with("</a>") {
        o.push_str("</li>");
    }
    o.push_str("</ul>");
    o
}

struct Shell<'a> {
    title: String,
    crumbs: Vec<(String, String)>,
    prev: Option<(&'a str, &'a str)>,
    next: Option<(&'a str, &'a str)>,
    toc: String,
    source: Option<String>,
    body: String,
}

fn shell(s: Shell) -> String {
    let link = |l: &Option<(&str, &str)>, key: &str, label: &str| match l {
        Some((url, t)) => format!("<li><a href=\"{url}\" title=\"{}\" accesskey=\"{key}\">{label}</a></li>", escape(t)),
        None => String::new(),
    };
    let crumbs: String = s.crumbs.iter().map(|(u, t)| format!("<li><a href=\"{u}\">{}</a></li>", escape(t))).collect();
    let here = if s.title.starts_with("Zsharpend") { String::new() } else { format!("<li class=\"here\">{}</li>", escape(&s.title)) };
    // Python-docs style: breadcrumbs on the left; index | modules | next | previous on the right.
    let related = format!(
        "<div class=\"related\" role=\"navigation\" aria-label=\"Related\"><div class=\"related-in\">\
<ul class=\"crumbs\"><li><span class=\"brand\">Z#end</span><a href=\"index.html\">Zsharpend {VERSION} documentation</a></li>{crumbs}{here}</ul>\
<ul class=\"rel-links\"><li><a href=\"genindex.html\" title=\"General Index\" accesskey=\"I\">index</a></li>\
<li><a href=\"modindex.html\" title=\"Module Index\">modules</a></li>{}{}</ul></div></div>",
        link(&s.next, "N", "next"),
        link(&s.prev, "P", "previous"),
    );
    let side_rel = |l: &Option<(&str, &str)>, h: &str| match l {
        Some((u, t)) => format!("<div class=\"side-block\"><h4>{h}</h4><p class=\"topless\"><a href=\"{u}\">{}</a></p></div>", escape(t)),
        None => String::new(),
    };
    let source = s.source.map(|p| format!("<div class=\"side-block\" role=\"note\"><h3>This page</h3><ul class=\"this-page-menu\"><li><a href=\"{SOURCE}docs/site/{p}\">Show source</a></li></ul></div>")).unwrap_or_default();
    format!(
        r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} — Zsharpend {VERSION} documentation</title>
<link rel="stylesheet" href="_static/style.css">
<script>try{{var t=localStorage.getItem('zxx-theme');if(t)document.documentElement.dataset.theme=t}}catch(e){{}}</script>
</head>
<body>
<header class="mobile-bar"><a href="index.html">Zsharpend {VERSION}</a><button class="menu" type="button" aria-expanded="false">Menu</button></header>
{related}
<div class="document">
  <aside class="sphinxsidebar" aria-label="Sidebar">
    <div class="sidebar-top">
      <form class="search" action="search.html" method="get" role="search">
        <h3>Quick search</h3>
        <input type="text" name="q" aria-label="Search the docs" placeholder="Search"><input type="submit" value="Go">
      </form>
      <div class="theme-choice"><label for="theme">Theme</label> <select id="theme"><option value="">Auto</option><option value="light">Light</option><option value="dark">Dark</option></select></div>
    </div>
    {toc}
    {prev_side}
    {next_side}
    {source}
  </aside>
  <main class="body" role="main" id="main">
{body}
  </main>
</div>
{related_bottom}
<footer class="footer">© 2026 the Zsharpend authors (MIT). Zsharpend grew from Z#++ and Sam-astro's Z#. Every example on this page was run by the Zsharpend engine when the page was built.</footer>
<script src="_static/searchindex.js"></script>
<script src="_static/site.js"></script>
</body>
</html>
"##,
        title = escape(&s.title),
        toc = if s.toc.is_empty() { String::new() } else { format!("<div class=\"side-block\"><h3>Table of Contents</h3>{}</div>", s.toc) },
        prev_side = side_rel(&s.prev, "Previous topic"),
        next_side = side_rel(&s.next, "Next topic"),
        related_bottom = related.replace("class=\"related\"", "class=\"related bottom\""),
        body = s.body,
    )
}

pub fn build(root: &Path, out: &Path) -> Result<Report, String> {
    let pages = load_pages(root)?;
    let mut report = Report { pages: pages.len(), ..Default::default() };
    std::fs::create_dir_all(out.join("_images")).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(out.join("_static")).map_err(|e| e.to_string())?;
    let mut rendered = vec![];
    for p in &pages {
        let mut cx = Ctx { root, page: p, report: &mut report, images: vec![], toc: vec![], index: vec![], module: None, used: HashMap::new(), figure: 0, text: String::new() };
        let html = render(&mut cx, &p.md)?;
        rendered.push(Rendered { html, toc: cx.toc, images: cx.images, index: cx.index, module: cx.module, text: cx.text });
    }
    let mut all_index: Vec<IndexEntry> = vec![];
    let mut modules: Vec<(String, String, String)> = vec![];
    let mut search: Vec<serde_json::Value> = vec![];
    for (i, (p, r)) in pages.iter().zip(rendered.iter()).enumerate() {
        let prev = i.checked_sub(1).map(|j| (pages[j].out(), pages[j].title.clone()));
        let next = pages.get(i + 1).map(|n| (n.out(), n.title.clone()));
        let crumbs = if p.file == "index.md" { vec![] } else { section_crumb(&pages, p) };
        let html = shell(Shell {
            title: p.title.clone(),
            crumbs,
            prev: prev.as_ref().map(|(a, b)| (a.as_str(), b.as_str())),
            next: next.as_ref().map(|(a, b)| (a.as_str(), b.as_str())),
            toc: if r.toc.is_empty() { String::new() } else { toc_html(&r.toc) },
            source: Some(p.file.clone()),
            body: r.html.clone(),
        });
        std::fs::write(out.join(p.out()), html).map_err(|e| e.to_string())?;
        for (f, png) in &r.images {
            std::fs::write(out.join(f), png).map_err(|e| e.to_string())?;
        }
        all_index.extend(r.index.iter().cloned());
        if let Some((m, syn)) = &r.module {
            modules.push((m.clone(), syn.clone(), format!("{}#module-{m}", p.out())));
        }
        let text: String = r.text.split_whitespace().collect::<Vec<_>>().join(" ");
        search.push(serde_json::json!({
            "title": p.title, "url": p.out(), "section": p.section,
            "headings": r.toc.iter().map(|(_, id, t)| serde_json::json!([id, t])).collect::<Vec<_>>(),
            "text": text.chars().take(20000).collect::<String>(),
        }));
    }
    report.index_entries = all_index.len();
    let objects: Vec<serde_json::Value> = all_index.iter().map(|e| serde_json::json!([e.name, e.what, e.url])).collect();
    std::fs::write(out.join("_static/searchindex.js"), format!("window.ZXX_SEARCH = {};\n", serde_json::json!({ "pages": search, "objects": objects }))).map_err(|e| e.to_string())?;
    std::fs::write(out.join("genindex.html"), genindex(&all_index)).map_err(|e| e.to_string())?;
    std::fs::write(out.join("modindex.html"), modindex(&modules)).map_err(|e| e.to_string())?;
    std::fs::write(out.join("search.html"), search_page()).map_err(|e| e.to_string())?;
    std::fs::write(out.join("_static/style.css"), STYLE).map_err(|e| e.to_string())?;
    std::fs::write(out.join("_static/site.js"), SCRIPT).map_err(|e| e.to_string())?;
    std::fs::write(out.join(".nojekyll"), "").map_err(|e| e.to_string())?;
    Ok(report)
}

fn section_crumb(pages: &[Page], p: &Page) -> Vec<(String, String)> {
    // The section's first page stands for the section.
    match pages.iter().find(|q| q.section == p.section) {
        Some(first) if first.file != p.file => vec![(first.out(), first.title.clone())],
        _ => vec![],
    }
}

fn genindex(entries: &[IndexEntry]) -> String {
    let mut by_letter: BTreeMap<char, Vec<&IndexEntry>> = BTreeMap::new();
    for e in entries {
        let c = e.name.trim_start_matches(['_', '.', '@']).chars().next().unwrap_or('_').to_ascii_uppercase();
        by_letter.entry(c).or_default().push(e);
    }
    let mut body = String::from("<h1 id=\"index\">Index</h1>\n<div class=\"genindex-jumpbox\">");
    body.push_str(&by_letter.keys().map(|c| format!("<a href=\"#{c}\"><strong>{c}</strong></a>")).collect::<Vec<_>>().join(" | "));
    body.push_str("</div>");
    for (c, mut list) in by_letter {
        list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.what.cmp(&b.what)));
        list.dedup_by(|a, b| a.name == b.name && a.what == b.what);
        body.push_str(&format!("<h2 id=\"{c}\">{c}</h2><table class=\"indextable genindextable\"><tr><td><ul>"));
        for e in list {
            body.push_str(&format!("<li><a href=\"{}\">{}</a> ({})</li>", e.url, escape(&e.name), escape(&e.what)));
        }
        body.push_str("</ul></td></tr></table>");
    }
    shell(Shell { title: "Index".into(), crumbs: vec![], prev: None, next: None, toc: String::new(), source: None, body })
}

fn modindex(mods: &[(String, String, String)]) -> String {
    let mut m = mods.to_vec();
    m.sort();
    let mut body = String::from("<h1>Module Index</h1><table class=\"indextable modindextable\">");
    let mut letter = ' ';
    for (name, syn, url) in &m {
        let c = name.chars().next().unwrap_or(' ').to_ascii_lowercase();
        if c != letter {
            letter = c;
            body.push_str(&format!("<tr class=\"cap\"><td></td><td><strong>{c}</strong></td><td></td></tr>"));
        }
        body.push_str(&format!("<tr><td></td><td><a href=\"{url}\"><code class=\"xref\">{}</code></a></td><td><em>{}</em></td></tr>", escape(name), escape(syn)));
    }
    body.push_str("</table>");
    shell(Shell { title: "Module Index".into(), crumbs: vec![], prev: None, next: None, toc: String::new(), source: None, body })
}

fn search_page() -> String {
    let body = "<h1 id=\"search-documentation\">Search</h1><p>Searching for several words only shows pages that contain all of them.</p>\
<form class=\"search-page\" action=\"\" method=\"get\"><input type=\"text\" name=\"q\" aria-label=\"Search\" autofocus> <input type=\"submit\" value=\"Search\"></form>\
<div id=\"search-results\" aria-live=\"polite\"></div>"
        .to_string();
    shell(Shell { title: "Search".into(), crumbs: vec![], prev: None, next: None, toc: String::new(), source: None, body })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn site_builds_and_examples_run() {
        let out = std::env::temp_dir().join(format!("zsharpend-docs-{}", std::process::id()));
        let r = build(&repo(), &out).unwrap_or_else(|e| panic!("{e}"));
        assert!(r.pages >= 10 && r.examples_run >= 20 && r.index_entries >= 40, "{r:?}");
        for f in ["index.html", "genindex.html", "modindex.html", "search.html", "_static/searchindex.js"] {
            assert!(out.join(f).exists(), "{f}");
        }
    }

    /// Every built-in global and every nd/gfx3d function is documented.
    #[test]
    fn library_reference_is_complete() {
        let site = repo().join("docs/site");
        let builtins = std::fs::read_to_string(site.join("library-builtins.md")).unwrap();
        let missing: Vec<&str> = zsharpend::stdlib::GLOBALS.iter().copied().filter(|g| !builtins.contains(&format!("::: function {g}(")) && !builtins.contains(&format!("::: data {g}")) && !builtins.contains(&format!("::: class {g}"))).collect();
        assert!(missing.is_empty(), "built-ins missing from library-builtins.md: {missing:?}");
        for (file, m) in [("library-nd.md", zsharpend_nd_module()), ("library-gfx3d.md", gfx3d_module())] {
            let text = std::fs::read_to_string(site.join(file)).unwrap();
            let Value::Map(map) = m else { panic!() };
            let missing: Vec<String> = map.borrow().keys().filter(|k| !text.contains(&format!(" {k}(")) && !text.contains(&format!(".{k}")) && !text.contains(&format!("::: data {k}"))).cloned().collect();
            assert!(missing.is_empty(), "{file} doesn't document {missing:?}");
        }
    }

    fn zsharpend_nd_module() -> Value {
        let mut it = zsharpend::Interp::new();
        whetstone::extenders().into_iter().find(|e| e.name() == "nd").unwrap().modules(&mut it).remove(0).1
    }
    fn gfx3d_module() -> Value {
        let mut it = zsharpend::Interp::new();
        whetstone::extenders().into_iter().find(|e| e.name() == "gfx3d").unwrap().modules(&mut it).remove(0).1
    }
}
