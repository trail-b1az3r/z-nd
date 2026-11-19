//! Whetstone's compile-time checks. Zsharpend runs programs dynamically,
//! so these find before a run what would otherwise fail (or silently
//! misbehave) during one:
//!
//! **Errors** — a name that isn't defined anywhere; assigning to a
//! constant; calling a script function with too many arguments or a
//! keyword it doesn't have; an unknown `Enum::Variant` or struct field; a
//! HolyC print whose `%` count doesn't match its values; a function
//! defined twice; `impl Trait for T` missing a method.
//!
//! **Warnings** — code after `return`/`break`/`throw`; a local variable
//! that's never used; a `match` on an enum that misses variants (and has
//! no `_`); a required argument left out; assigning to an undeclared name
//! in a `.zxx` file (Z#'s implicit declaration).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use zsharpend::ast::*;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub severity: Severity,
    pub file: String,
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        if self.line > 0 {
            write!(f, "{}:{}: {s}: {}", self.file, self.line, self.message)
        } else {
            write!(f, "{}: {s}: {}", self.file, self.message)
        }
    }
}

/// One parsed program file.
pub struct Unit {
    pub path: String,
    pub kind: Kind,
    pub items: Vec<Item>,
    /// Whose names it sees: `None` = the program, `Some(m)` = module `m`.
    pub module: Option<String>,
}

struct Var {
    constant: bool,
    used: bool,
    line: usize,
    local: bool,
}

struct Checker<'a> {
    diags: Vec<Diagnostic>,
    file: String,
    zxx: bool,
    scopes: Vec<HashMap<String, Var>>,
    globals: &'a HashSet<String>,
    consts: &'a HashSet<String>,
    funcs: &'a HashMap<String, Rc<FuncDef>>,
    enums: &'a HashMap<String, Vec<(String, Vec<String>)>>,
    structs: &'a HashMap<String, Vec<String>>,
    /// The line of the statement being checked (for expressions without one).
    cur: usize,
}

/// Names every program can see: built-ins, what the host and extenders add.
pub struct Known {
    pub globals: HashSet<String>,
}

/// A namespace's top-level names, constants and functions.
type Space = (HashSet<String>, HashSet<String>, HashMap<String, Rc<FuncDef>>);

pub fn check(units: &[Unit], known: &Known, gpu_names: &[String]) -> Vec<Diagnostic> {
    let mut diags = vec![];
    // Per namespace (program or module): the top-level names.
    let mut spaces: BTreeMap<Option<String>, Space> = BTreeMap::new();
    let mut enums: HashMap<String, Vec<(String, Vec<String>)>> = HashMap::new();
    let mut structs: HashMap<String, Vec<String>> = HashMap::new();
    let mut traits: HashMap<String, Vec<Rc<FuncDef>>> = HashMap::new();
    enums.insert("Result".into(), vec![("Ok".into(), vec!["value".into()]), ("Err".into(), vec!["error".into()])]);
    // Pass 1: declarations.
    for u in units {
        let (names, consts, funcs) = spaces.entry(u.module.clone()).or_default();
        let mut seen_here: HashMap<String, usize> = HashMap::new();
        for it in &u.items {
            match it {
                Item::Func(f) => {
                    if f.body.is_some() {
                        if let Some(prev) = seen_here.insert(f.name.clone(), f.line) {
                            diags.push(Diagnostic { severity: Severity::Error, file: u.path.clone(), line: f.line, message: format!("`{}` is defined twice (also on line {prev})", f.name) });
                        }
                        funcs.insert(f.name.clone(), f.clone());
                    }
                    names.insert(f.name.clone());
                }
                Item::Struct { name, fields, .. } => {
                    names.insert(name.clone());
                    structs.insert(name.clone(), fields.iter().map(|(f, _)| f.clone()).collect());
                }
                Item::Enum { name, variants, .. } => {
                    names.insert(name.clone());
                    for (v, fs) in variants {
                        names.insert(v.clone());
                        structs.insert(v.clone(), fs.clone());
                    }
                    enums.insert(name.clone(), variants.clone());
                }
                Item::Trait { name, methods, .. } => {
                    names.insert(name.clone());
                    traits.insert(name.clone(), methods.clone());
                }
                Item::ImportModule { name, alias, names: list, .. } => match list {
                    None => {
                        names.insert(alias.clone().unwrap_or(name.clone()));
                    }
                    Some(l) => {
                        for (n, a) in l {
                            names.insert(a.clone().unwrap_or(n.clone()));
                        }
                    }
                },
                Item::Stmt(s) => top_level_names(s, names, consts),
                _ => {}
            }
            collect_globals(it, names);
        }
    }
    // Everything sees the program's names (modules run with it as parent).
    let program = spaces.get(&None).map(|s| s.0.clone()).unwrap_or_default();
    // Pass 2: bodies.
    for u in units {
        if !u.kind.is_program() {
            continue;
        }
        let (own, consts, funcs) = spaces.get(&u.module).cloned().unwrap_or_default();
        let mut globals: HashSet<String> = known.globals.clone();
        globals.extend(program.iter().cloned());
        globals.extend(own);
        globals.extend(gpu_names.iter().cloned());
        let mut c = Checker {
            diags: vec![],
            file: u.path.clone(),
            zxx: u.kind == Kind::Zxx,
            scopes: vec![],
            globals: &globals,
            consts: &consts,
            funcs: &funcs,
            enums: &enums,
            structs: &structs,
            cur: 0,
        };
        for it in &u.items {
            c.item(it, &traits);
        }
        diags.extend(c.diags);
    }
    diags.sort_by(|a, b| (&a.file, a.line, &a.severity).cmp(&(&b.file, b.line, &b.severity)));
    diags
}

fn top_level_names(s: &Stmt, names: &mut HashSet<String>, consts: &mut HashSet<String>) {
    match s {
        Stmt::Let { name, constant, .. } => {
            names.insert(name.clone());
            if *constant {
                consts.insert(name.clone());
            }
        }
        Stmt::Destructure { names: ns, .. } => names.extend(ns.iter().cloned()),
        // Z#: a top-level assignment declares a global.
        Stmt::Assign { target: Expr::Var(n, _), .. } => {
            names.insert(n.clone());
        }
        _ => {}
    }
}

/// `global let x` anywhere in a function makes `x` a global.
fn collect_globals(it: &Item, names: &mut HashSet<String>) {
    fn stmts(b: &[Stmt], names: &mut HashSet<String>) {
        for s in b {
            match s {
                Stmt::Let { name, global: true, .. } => {
                    names.insert(name.clone());
                }
                Stmt::If { then, els, .. } => {
                    stmts(then, names);
                    if let Some(e) = els {
                        stmts(e, names);
                    }
                }
                Stmt::While { body, .. } | Stmt::For { body, .. } | Stmt::Block(body) => stmts(body, names),
                Stmt::Try { body, catch, .. } => {
                    stmts(body, names);
                    stmts(catch, names);
                }
                Stmt::Switch { cases, default, .. } => {
                    for c in cases {
                        stmts(&c.body, names);
                    }
                    if let Some(d) = default {
                        stmts(d, names);
                    }
                }
                _ => {}
            }
        }
    }
    match it {
        Item::Func(f) => {
            if let Some(b) = &f.body {
                stmts(b, names);
            }
        }
        Item::Impl { methods, .. } => {
            for m in methods {
                if let Some(b) = &m.body {
                    stmts(b, names);
                }
            }
        }
        Item::On { body, .. } => stmts(body, names),
        _ => {}
    }
}

fn printf_count(fmt: &str) -> Option<usize> {
    let c: Vec<char> = fmt.chars().collect();
    let (mut i, mut n) = (0, 0);
    while i < c.len() {
        if c[i] == '%' {
            if c.get(i + 1) == Some(&'%') {
                i += 2;
                continue;
            }
            n += 1;
        }
        i += 1;
    }
    Some(n)
}

impl Checker<'_> {
    fn err(&mut self, line: usize, m: String) {
        let line = if line == 0 { self.cur } else { line };
        self.diags.push(Diagnostic { severity: Severity::Error, file: self.file.clone(), line, message: m });
    }
    fn warn(&mut self, line: usize, m: String) {
        let line = if line == 0 { self.cur } else { line };
        self.diags.push(Diagnostic { severity: Severity::Warning, file: self.file.clone(), line, message: m });
    }

    fn push(&mut self) {
        self.scopes.push(HashMap::new());
    }
    fn pop(&mut self) {
        if let Some(s) = self.scopes.pop() {
            let mut unused: Vec<(usize, String)> = s.into_iter().filter(|(n, v)| v.local && !v.used && !n.starts_with('_') && n != "self").map(|(n, v)| (v.line, n)).collect();
            unused.sort();
            for (line, n) in unused {
                self.warn(line, format!("`{n}` is never used"));
            }
        }
    }
    fn declare(&mut self, name: &str, constant: bool, line: usize, local: bool) {
        if let Some(s) = self.scopes.last_mut() {
            s.insert(name.into(), Var { constant, used: false, line, local });
        }
    }
    fn lookup(&mut self, name: &str) -> Option<&mut Var> {
        self.scopes.iter_mut().rev().find_map(|s| s.get_mut(name))
    }
    fn use_name(&mut self, name: &str, line: usize) {
        if let Some(v) = self.lookup(name) {
            v.used = true;
            return;
        }
        if self.globals.contains(name) || name == "_" {
            return;
        }
        self.err(line, format!("`{name}` isn't defined"));
    }

    fn item(&mut self, it: &Item, traits: &HashMap<String, Vec<Rc<FuncDef>>>) {
        match it {
            Item::Func(f) => self.func(f, false),
            Item::Impl { name, methods, line, of_trait } => {
                if let Some(t) = of_trait {
                    match traits.get(t) {
                        None => self.err(*line, format!("there's no trait `{t}`")),
                        Some(req) => {
                            for r in req {
                                if r.body.is_none() && !methods.iter().any(|m| m.name == r.name) {
                                    self.err(*line, format!("`impl {t} for {name}` is missing `{}`", r.name));
                                }
                            }
                        }
                    }
                }
                for m in methods {
                    self.func(m, true);
                }
            }
            Item::Trait { methods, .. } => {
                for m in methods.iter().filter(|m| m.body.is_some()) {
                    self.func(m, true);
                }
            }
            Item::On { params, body, line, .. } => {
                self.push();
                for p in params {
                    self.declare(p, false, *line, false);
                }
                self.block(body);
                self.pop();
            }
            Item::Stmt(s) => {
                // Top-level statements run in the global scope.
                self.push();
                self.stmt(s, true);
                self.scopes.pop();
            }
            _ => {}
        }
    }

    fn func(&mut self, f: &FuncDef, _method: bool) {
        let Some(body) = &f.body else { return };
        self.push();
        for p in &f.params {
            if let Some(d) = &p.default {
                self.expr(d);
            }
            self.declare(&p.name, false, f.line, false);
        }
        self.block(body);
        self.pop();
    }

    fn block(&mut self, b: &[Stmt]) {
        let mut done = false;
        for s in b {
            if done {
                self.warn(stmt_line(s), "this code never runs (it comes after a return, break, continue or throw)".into());
                break;
            }
            self.stmt(s, false);
            done = matches!(s, Stmt::Return(..) | Stmt::Break(_) | Stmt::Continue(_) | Stmt::Throw(..));
        }
    }

    fn scoped(&mut self, b: &[Stmt]) {
        self.push();
        self.block(b);
        self.pop();
    }

    fn stmt(&mut self, s: &Stmt, top: bool) {
        if stmt_line(s) > 0 {
            self.cur = stmt_line(s);
        }
        match s {
            Stmt::Let { name, value, global, constant, line, .. } => {
                if let Some(v) = value {
                    self.expr(v);
                }
                if !global {
                    self.declare(name, *constant, *line, !top);
                }
            }
            Stmt::Assign { target, value, line, op } => {
                self.expr(value);
                match target {
                    Expr::Var(n, l) => {
                        let consts = self.consts;
                        let globals = self.globals;
                        let found = self.lookup(n).map(|v| {
                            if *op != "=" {
                                v.used = true;
                            }
                            v.constant
                        });
                        match found {
                            Some(constant) => {
                                if constant {
                                    self.err(*l, format!("`{n}` is a constant"));
                                }
                            }
                            None if consts.contains(n) => self.err(*l, format!("`{n}` is a constant")),
                            None if globals.contains(n) => {}
                            None => {
                                if *op != "=" {
                                    self.err(*l, format!("`{n}` isn't defined"));
                                } else {
                                    if self.zxx && !top {
                                        self.warn(*line, format!("`{n}` isn't declared; this assignment declares it (write `let {n} = …`)"));
                                    }
                                    self.declare(n, false, *line, !top);
                                }
                            }
                        }
                    }
                    other => self.expr(other),
                }
            }
            Stmt::Expr(e, _) | Stmt::Throw(e, _) => self.expr(e),
            Stmt::Bare(e, line) => {
                if let Expr::Var(n, _) = e {
                    if !self.funcs.contains_key(n) && self.lookup(n).is_some() {
                        self.warn(*line, format!("`{n};` does nothing (it's not a function)"));
                    }
                }
                self.expr(e);
            }
            Stmt::Return(v, _) => {
                if let Some(v) = v {
                    self.expr(v);
                }
            }
            Stmt::If { cond, then, els, .. } => {
                self.expr(cond);
                self.scoped(then);
                if let Some(e) = els {
                    self.scoped(e);
                }
            }
            Stmt::While { cond, body, .. } => {
                self.expr(cond);
                self.scoped(body);
            }
            Stmt::For { var, index, iter, body, line } => {
                self.expr(iter);
                self.push();
                self.declare(var, false, *line, false);
                if let Some(i) = index {
                    self.declare(i, false, *line, false);
                }
                self.block(body);
                self.pop();
            }
            Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::Try { body, var, catch, line } => {
                self.scoped(body);
                self.push();
                if let Some(v) = var {
                    self.declare(v, false, *line, false);
                }
                self.block(catch);
                self.pop();
            }
            Stmt::Block(b) => self.scoped(b),
            Stmt::Destructure { names, value, declare, line } => {
                self.expr(value);
                if let Expr::List(items) = value {
                    if items.len() != names.len() {
                        self.err(*line, format!("{} values to unpack into {} names", items.len(), names.len()));
                    }
                }
                for n in names {
                    if *declare {
                        self.declare(n, false, *line, !top && !n.starts_with("__"));
                    } else if self.lookup(n).is_none() && !self.globals.contains(n) {
                        self.declare(n, false, *line, !top);
                    }
                }
            }
            Stmt::Print { fmt, args, line } => {
                self.expr(fmt);
                for a in args {
                    self.expr(a);
                }
                if let Expr::Str(segs) = fmt {
                    if !args.is_empty() && segs.iter().all(|s| matches!(s, StrSeg::Lit(_))) {
                        let text: String = segs.iter().map(|s| if let StrSeg::Lit(t) = s { t.as_str() } else { "" }).collect();
                        if let Some(n) = printf_count(&text) {
                            if n != args.len() {
                                self.err(*line, format!("the format has {n} `%` but {} value(s) follow", args.len()));
                            }
                        }
                    }
                }
            }
            Stmt::Switch { value, cases, default, .. } => {
                self.expr(value);
                for c in cases {
                    for (a, b) in &c.pats {
                        self.expr(a);
                        if let Some(b) = b {
                            self.expr(b);
                        }
                    }
                    self.scoped(&c.body);
                }
                if let Some(d) = default {
                    self.scoped(d);
                }
            }
        }
    }

    fn call_name(e: &Expr) -> Option<&str> {
        match e {
            Expr::Var(n, _) => Some(n),
            Expr::Path(_, n, _) => Some(n),
            _ => None,
        }
    }

    /// A destructuring pattern: binds lower-case names, checks the rest.
    fn pattern(&mut self, p: &Expr, line: usize) {
        if let Expr::Call(callee, args, l) = p {
            if let Some(name) = Self::call_name(callee) {
                let is_ctor = name == "Some" || self.structs.contains_key(name) || matches!(name, "Ok" | "Err");
                if is_ctor {
                    if let Expr::Path(..) = &**callee {
                        self.expr(callee);
                    }
                    if let Some(fields) = self.structs.get(name) {
                        if args.len() > fields.len() {
                            self.err(*l, format!("`{name}` has {} field(s), the pattern has {}", fields.len(), args.len()));
                        }
                    }
                    for a in args {
                        match a {
                            Expr::Var(n, _) if n.starts_with(|c: char| c.is_lowercase() || c == '_') => {
                                if n != "_" {
                                    self.declare(n, false, line, true);
                                }
                            }
                            other => self.pattern(other, line),
                        }
                    }
                    return;
                }
            }
        }
        self.expr(p);
    }

    fn expr(&mut self, e: &Expr) {
        match e {
            Expr::Null | Expr::Bool(_) | Expr::Int(_) | Expr::Float(_) | Expr::Color(_) => {}
            Expr::Str(segs) => {
                for s in segs {
                    if let StrSeg::Expr(x) = s {
                        self.expr(x);
                    }
                }
            }
            Expr::Var(n, line) => self.use_name(n, *line),
            Expr::List(xs) => xs.iter().for_each(|x| self.expr(x)),
            Expr::Map(kv) => {
                for (k, v) in kv {
                    self.expr(k);
                    self.expr(v);
                }
            }
            Expr::StructLit(name, fields) => match self.structs.get(name).cloned() {
                None if !self.globals.contains(name) => self.err(0, format!("there's no struct `{name}`")),
                None => fields.iter().for_each(|(_, v)| self.expr(v)),
                Some(decl) => {
                    for (f, v) in fields {
                        if !decl.contains(f) {
                            self.err(0, format!("{name} has no field `{f}`"));
                        }
                        self.expr(v);
                    }
                }
            },
            Expr::Unary(_, x) => self.expr(x),
            Expr::Binary(_, a, b) => {
                self.expr(a);
                self.expr(b);
            }
            Expr::Ternary(a, b, c) => {
                self.expr(a);
                self.expr(b);
                self.expr(c);
            }
            Expr::Call(callee, args, line) => {
                self.expr(callee);
                for a in args {
                    match a {
                        Expr::Named(_, v) => self.expr(v),
                        a => self.expr(a),
                    }
                }
                // Arity of the program's own functions (when not shadowed by a local).
                if let Expr::Var(n, _) = &**callee {
                    let shadowed = self.scopes.iter().any(|s| s.contains_key(n));
                    if let (false, Some(f)) = (shadowed, self.funcs.get(n).cloned()) {
                        let params: Vec<&Param> = f.params.iter().filter(|p| p.name != "self").collect();
                        let positional = args.iter().filter(|a| !matches!(a, Expr::Named(..))).count();
                        if positional > params.len() {
                            self.err(*line, format!("`{n}` takes {} argument(s), but {positional} are given", params.len()));
                        }
                        let named: Vec<&String> = args.iter().filter_map(|a| if let Expr::Named(k, _) = a { Some(k) } else { None }).collect();
                        for k in &named {
                            if !params.iter().any(|p| &&p.name == k) {
                                self.err(*line, format!("`{n}` has no parameter `{k}`"));
                            }
                        }
                        let missing: Vec<&str> =
                            params.iter().enumerate().filter(|(i, p)| *i >= positional && p.default.is_none() && !named.contains(&&p.name)).map(|(_, p)| p.name.as_str()).collect();
                        if self.zxx && !missing.is_empty() {
                            self.warn(*line, format!("`{n}` is called without `{}` (it will be null)", missing.join("`, `")));
                        }
                    }
                }
            }
            Expr::Index(o, i) => {
                self.expr(o);
                self.expr(i);
            }
            Expr::Field(o, _, _) => self.expr(o),
            Expr::Lambda(f) => self.func(f, false),
            Expr::Range(a, b, _) => {
                self.expr(a);
                self.expr(b);
            }
            Expr::Match(v, arms) => {
                self.expr(v);
                let mut covered: Vec<String> = vec![];
                let mut wildcard = false;
                for arm in arms {
                    self.push();
                    if arm.pats.is_empty() {
                        wildcard = true;
                    }
                    for p in &arm.pats {
                        match p {
                            Expr::Call(c, _, _) => {
                                if let Some(n) = Self::call_name(c) {
                                    covered.push(n.to_string());
                                }
                            }
                            Expr::Var(n, _) | Expr::Path(_, n, _) => covered.push(n.clone()),
                            _ => {}
                        }
                        self.pattern(p, 0);
                    }
                    if let Some(g) = &arm.guard {
                        self.expr(g);
                    }
                    self.block(&arm.body);
                    self.pop();
                }
                if !wildcard && !covered.is_empty() {
                    // An enum whose variants the arms name: are all covered?
                    if let Some((en, vs)) = self.enums.iter().find(|(_, vs)| covered.iter().all(|c| vs.iter().any(|(v, _)| v == c))) {
                        let missing: Vec<&str> = vs.iter().map(|(v, _)| v.as_str()).filter(|v| !covered.iter().any(|c| c == v)).collect();
                        if !missing.is_empty() {
                            let en = en.clone();
                            let m = missing.join(", ");
                            self.warn(0, format!("this match on {en} doesn't handle {m} (add those arms or `_ =>`)"));
                        }
                    }
                }
            }
            Expr::Comp { elem, value, vars, iter, cond } => {
                self.expr(iter);
                self.push();
                for v in vars {
                    self.declare(v, false, 0, false);
                }
                if let Some(c) = cond {
                    self.expr(c);
                }
                self.expr(elem);
                if let Some(v) = value {
                    self.expr(v);
                }
                self.pop();
            }
            Expr::Subscript(o, subs) => {
                self.expr(o);
                for s in subs {
                    match s {
                        Sub::At(e) => self.expr(e),
                        Sub::Slice(a, b, c) => {
                            for x in [a, b, c].into_iter().flatten() {
                                self.expr(x);
                            }
                        }
                    }
                }
            }
            Expr::Try(x, _) => self.expr(x),
            Expr::Named(_, v) => self.expr(v),
            Expr::Path(en, v, line) => match self.enums.get(en) {
                None => self.err(*line, format!("there's no enum `{en}`")),
                Some(vs) if !vs.iter().any(|(n, _)| n == v) => {
                    let names: Vec<&str> = vs.iter().map(|(n, _)| n.as_str()).collect();
                    self.err(*line, format!("`{en}` has no variant `{v}` (it has {})", names.join(", ")));
                }
                _ => {}
            },
        }
    }
}

fn stmt_line(s: &Stmt) -> usize {
    match s {
        Stmt::Let { line, .. }
        | Stmt::Assign { line, .. }
        | Stmt::If { line, .. }
        | Stmt::While { line, .. }
        | Stmt::For { line, .. }
        | Stmt::Try { line, .. }
        | Stmt::Destructure { line, .. }
        | Stmt::Print { line, .. }
        | Stmt::Switch { line, .. } => *line,
        Stmt::Expr(_, l) | Stmt::Return(_, l) | Stmt::Break(l) | Stmt::Continue(l) | Stmt::Throw(_, l) | Stmt::Bare(_, l) => *l,
        Stmt::Block(_) => 0,
    }
}
