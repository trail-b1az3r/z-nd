//! The interpreter: a sandboxed tree walker.
//!
//! Scripts can only reach what the host registers (`Interp::register`,
//! host objects); there is no file, shell or network access. Every
//! statement and expression costs a step, calls are limited in depth, and
//! lists/strings in length, so a script can't hang or exhaust its host.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::ast::*;
use crate::types::TypeSpec;
use crate::value::*;

/// How a statement ended.
pub enum Ctrl {
    Return(Value),
    Break,
    Continue,
    /// A script error or `throw`: `try`/`catch` can catch it.
    Throw(Value),
    /// A sandbox limit: can't be caught.
    Fatal(String),
}

type Flow<T> = Result<T, Ctrl>;

fn throw<T>(msg: impl Into<String>) -> Flow<T> {
    Err(Ctrl::Throw(Value::str(&msg.into())))
}

#[derive(Clone, Debug)]
pub struct Limits {
    /// Statements and expressions a run may evaluate.
    pub steps: u64,
    /// Nested calls.
    pub depth: usize,
    /// Items in a list / characters in a string.
    pub max_len: usize,
    /// Wall-clock time per run.
    pub time: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { steps: 20_000_000, depth: 64, max_len: 10_000_000, time: Duration::from_secs(5) }
    }
}

pub struct Handler {
    pub event: String,
    pub filter: Option<String>,
    pub params: Vec<String>,
    pub body: Rc<Vec<Stmt>>,
}

pub struct Interp {
    pub globals: Env,
    pub structs: HashMap<String, Vec<(String, Option<Type>)>>,
    pub methods: HashMap<String, HashMap<String, Rc<FuncDef>>>,
    pub handlers: Vec<Handler>,
    /// Kernels and fragments from imported .zsg modules, by name.
    pub gpu: HashMap<String, Rc<crate::zsg::Module>>,
    /// What `Print`/`Printl` wrote.
    pub output: Vec<String>,
    pub limits: Limits,
    steps: u64,
    depth: usize,
    started: Instant,
    rng: u64,
    /// `ZS.Graphics.Init` was called: the host should call `Start()` and `Update(dt)`.
    pub graphics: Option<(String, u32, u32)>,
    /// Enums (`.zxx`): name → variants and their fields.
    pub enums: HashMap<String, Vec<(String, Vec<String>)>>,
    /// Variant name → its enum.
    pub variant_of: HashMap<String, String>,
    /// Traits (`.zxx`): name → methods (with or without default bodies).
    pub traits: HashMap<String, Vec<Rc<FuncDef>>>,
    /// Type name → the traits it implements (`impl Area for Shape`).
    pub implemented: HashMap<String, Vec<String>>,
    /// Modules a program can `import` by name: from script extenders (Rust)
    /// or from `.zxx` module sources, as namespaces (maps).
    pub modules: HashMap<String, Value>,
}

impl Default for Interp {
    fn default() -> Self {
        Self::new()
    }
}

impl Interp {
    pub fn new() -> Self {
        let mut it = Interp {
            globals: new_scope(None),
            structs: HashMap::new(),
            methods: HashMap::new(),
            handlers: vec![],
            gpu: HashMap::new(),
            output: vec![],
            limits: Limits::default(),
            steps: 0,
            depth: 0,
            started: Instant::now(),
            rng: 0x9E37_79B9_7F4A_7C15,
            graphics: None,
            enums: HashMap::new(),
            variant_of: HashMap::new(),
            traits: HashMap::new(),
            implemented: HashMap::new(),
            modules: HashMap::new(),
        };
        crate::stdlib::install(&mut it);
        it
    }

    /// Make `name` callable from scripts.
    pub fn register(&mut self, name: &str, f: impl Fn(&mut Interp, Vec<Value>) -> Result<Value, String> + 'static) {
        self.set_global(name, Value::native(name, f));
    }

    pub fn set_global(&mut self, name: &str, v: Value) {
        self.globals.borrow_mut().vars.insert(name.into(), v);
    }

    pub fn global(&self, name: &str) -> Option<Value> {
        self.globals.borrow().vars.get(name).cloned()
    }

    pub fn seed(&mut self, s: u64) {
        self.rng = s.max(1);
    }

    pub fn random(&mut self) -> f64 {
        // xorshift64*
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }

    fn reset_budget(&mut self) {
        self.steps = 0;
        self.started = Instant::now();
    }

    fn tick(&mut self) -> Flow<()> {
        self.steps += 1;
        if self.steps > self.limits.steps {
            return Err(Ctrl::Fatal(format!("the script ran out of steps ({} allowed)", self.limits.steps)));
        }
        if self.steps % 4096 == 0 && self.started.elapsed() > self.limits.time {
            return Err(Ctrl::Fatal(format!("the script ran longer than {:?}", self.limits.time)));
        }
        Ok(())
    }

    pub fn check_len(&self, n: usize) -> Result<(), String> {
        if n > self.limits.max_len {
            Err(format!("too large: {n} items (limit {})", self.limits.max_len))
        } else {
            Ok(())
        }
    }

    // ── Loading ────────────────────────────────────────────────────────

    /// Register items (functions, structs, impls, handlers, GPU items) and run
    /// top-level statements in order. Returns the error as text.
    pub fn load(&mut self, items: &[Item]) -> Result<(), String> {
        let g = self.globals.clone();
        self.load_into(items, &g)
    }

    /// Load a `.zxx` module's items into a namespace of its own and make it
    /// importable as `name`. Its functions see each other (and the
    /// program's globals); importers see what it defines at top level.
    pub fn load_module(&mut self, name: &str, items: &[Item]) -> Result<(), String> {
        let env = new_scope(Some(self.globals.clone()));
        self.load_into(items, &env)?;
        let exports: BTreeMap<String, Value> = env.borrow().vars.iter().filter(|(k, _)| !k.starts_with('_')).map(|(k, v)| (k.clone(), v.clone())).collect();
        self.modules.insert(name.to_string(), Value::map(exports));
        Ok(())
    }

    fn load_into(&mut self, items: &[Item], env: &Env) -> Result<(), String> {
        self.reset_budget();
        // Types first (structs, enums, traits), so `impl` and functions can
        // refer to ones declared further down.
        let early = |i: &Item| matches!(i, Item::Struct { .. } | Item::Enum { .. } | Item::Trait { .. });
        let ordered: Vec<&Item> = items.iter().filter(|i| early(i)).chain(items.iter().filter(|i| !early(i))).collect();
        for it in ordered {
            match it {
                Item::Func(f) if f.body.is_some() => {
                    let c = Value::Func(Rc::new(Closure { def: f.clone(), env: env.clone() }));
                    env.borrow_mut().vars.insert(f.name.clone(), c);
                }
                Item::Struct { name, fields, .. } => {
                    self.structs.insert(name.clone(), fields.clone());
                }
                Item::Impl { name, methods, of_trait, line } => {
                    // Methods of an enum apply to each of its variants.
                    let mut types = vec![name.clone()];
                    if let Some(vs) = self.enums.get(name) {
                        types.extend(vs.iter().map(|(v, _)| v.clone()));
                    }
                    let mut all = methods.clone();
                    if let Some(t) = of_trait {
                        let Some(req) = self.traits.get(t).cloned() else { return Err(format!("line {line}: there's no trait `{t}`")) };
                        for r in &req {
                            if !methods.iter().any(|m| m.name == r.name) {
                                if r.body.is_some() {
                                    all.push(r.clone()); // the trait's default
                                } else {
                                    return Err(format!("line {line}: `impl {t} for {name}` is missing `{}`", r.name));
                                }
                            }
                        }
                    }
                    for ty in types {
                        if let Some(t) = of_trait {
                            self.implemented.entry(ty.clone()).or_default().push(t.clone());
                        }
                        let m = self.methods.entry(ty).or_default();
                        for f in &all {
                            m.insert(f.name.clone(), f.clone());
                        }
                    }
                }
                Item::Enum { name, variants, .. } => {
                    for (v, fields) in variants {
                        self.structs.insert(v.clone(), fields.iter().map(|f| (f.clone(), None)).collect());
                        self.variant_of.insert(v.clone(), name.clone());
                        let val = if fields.is_empty() {
                            Value::Struct(Rc::new(RefCell::new(StructVal { name: v.clone(), fields: vec![] })))
                        } else {
                            let n = v.clone();
                            Value::native(v, move |it, args| it.construct(&n, args))
                        };
                        self.set_global(v, val);
                    }
                    self.enums.insert(name.clone(), variants.clone());
                    // The enum's name is a namespace of its variants (so a
                    // module can export it, and `Shape.Circle(1)` works too).
                    let ns: BTreeMap<String, Value> = variants.iter().filter_map(|(v, _)| self.global(v).map(|x| (v.clone(), x))).collect();
                    env.borrow_mut().vars.insert(name.clone(), Value::map(ns));
                }
                Item::Trait { name, methods, .. } => {
                    self.traits.insert(name.clone(), methods.clone());
                }
                Item::ImportModule { name, alias, names, line } => {
                    let Some(m) = self.modules.get(name).cloned() else {
                        return Err(format!("line {line}: there's no module `{name}` (built in, in the project, or installed)"));
                    };
                    match names {
                        None => {
                            env.borrow_mut().vars.insert(alias.as_deref().unwrap_or(name).to_string(), m);
                        }
                        Some(list) => {
                            for (n, a) in list {
                                let v = match &m {
                                    Value::Map(mm) => mm.borrow().get(n).cloned(),
                                    Value::Object(o) => o.field(n),
                                    _ => None,
                                };
                                let v = v.ok_or(format!("line {line}: module `{name}` has no `{n}`"))?;
                                env.borrow_mut().vars.insert(a.as_deref().unwrap_or(n).to_string(), v);
                            }
                        }
                    }
                }
                Item::On { event, filter, params, body, .. } => {
                    let filter = match filter {
                        Some(e) => Some(self.eval(e, &self.globals.clone()).map_err(|c| self.describe(c))?.to_string()),
                        None => None,
                    };
                    self.handlers.push(Handler { event: event.clone(), filter, params: params.clone(), body: Rc::new(body.clone()) });
                }
                _ => {}
            }
        }
        for it in items {
            if let Item::Stmt(s) = it {
                self.exec(s, env).map_err(|c| self.describe(c))?;
            }
        }
        Ok(())
    }

    /// Make a .zsg module's kernels and fragments available to scripts by name.
    pub fn add_gpu_module(&mut self, m: Rc<crate::zsg::Module>) {
        for name in m.entry_names() {
            self.gpu.insert(name.clone(), m.clone());
            self.set_global(&name, Value::Object(Rc::new(crate::zsg::ShaderRef { name: name.clone(), module: m.clone() })));
        }
    }

    /// Text for an error that escaped.
    pub fn describe(&self, c: Ctrl) -> String {
        match c {
            Ctrl::Throw(v) => v.to_string(),
            Ctrl::Fatal(m) => m,
            Ctrl::Return(_) => "`return` outside a function".into(),
            Ctrl::Break => "`break` outside a loop".into(),
            Ctrl::Continue => "`continue` outside a loop".into(),
        }
    }

    /// Call a script function by name (e.g. `Main`, `Update`), if defined.
    pub fn call_global(&mut self, name: &str, args: Vec<Value>) -> Result<Option<Value>, String> {
        let Some(f) = self.global(name) else { return Ok(None) };
        self.reset_budget();
        self.call_value(&f, args, 0).map(Some).map_err(|c| self.describe(c))
    }

    /// Run the `on <event>` handlers; `subject` is matched against each
    /// handler's filter (a glob such as `*://example.com/*`). Returns how many ran.
    pub fn emit(&mut self, event: &str, subject: Option<&str>, args: Vec<Value>) -> Result<usize, String> {
        self.emit_values(event, subject, args).map(|v| v.len())
    }

    /// Like [`Interp::emit`], returning each handler's `return` value
    /// (`null` for a handler that doesn't return one).
    pub fn emit_values(&mut self, event: &str, subject: Option<&str>, args: Vec<Value>) -> Result<Vec<Value>, String> {
        self.reset_budget();
        let hs: Vec<(Vec<String>, Rc<Vec<Stmt>>)> = self
            .handlers
            .iter()
            .filter(|h| h.event == event)
            .filter(|h| match (&h.filter, subject) {
                (Some(f), Some(s)) => glob(f, s),
                (Some(_), None) => false,
                (None, _) => true,
            })
            .map(|h| (h.params.clone(), h.body.clone()))
            .collect();
        let mut out = Vec::with_capacity(hs.len());
        for (params, body) in &hs {
            let env = new_scope(Some(self.globals.clone()));
            for (i, p) in params.iter().enumerate() {
                env.borrow_mut().vars.insert(p.clone(), args.get(i).cloned().unwrap_or(Value::Null));
            }
            match self.exec_block(body, &env) {
                Ok(()) => out.push(Value::Null),
                Err(Ctrl::Return(v)) => out.push(v),
                Err(c) => return Err(self.describe(c)),
            }
        }
        Ok(out)
    }

    // ── Statements ─────────────────────────────────────────────────────

    fn exec_block(&mut self, b: &[Stmt], env: &Env) -> Flow<()> {
        for s in b {
            self.exec(s, env)?;
        }
        Ok(())
    }

    /// Run a block; its value is the last expression statement's.
    fn block_value(&mut self, b: &[Stmt], env: &Env) -> Flow<Value> {
        let mut last = Value::Null;
        for s in b {
            match s {
                Stmt::Expr(e, _) => last = self.eval(e, env)?,
                other => {
                    self.exec(other, env)?;
                    last = Value::Null;
                }
            }
        }
        Ok(last)
    }

    fn declare(&mut self, env: &Env, name: &str, v: Value, constant: bool) {
        let mut s = env.borrow_mut();
        s.vars.insert(name.to_string(), v);
        s.types.remove(name);
        if constant {
            s.consts.push(name.to_string());
        }
    }

    fn coerce(&self, ty: &Type, v: Value, line: usize) -> Flow<Value> {
        if ty.array && !ty.name.contains('<') {
            let list = Type { name: format!("list<{}>", ty.name), array: true };
            return self.coerce_spec(&list, v, line);
        }
        let bad = |want: &str, v: &Value| throw(format!("line {line}: expected {want}, got {} ({})", v.type_name(), v));
        Ok(match (ty.name.as_str(), &v) {
            (_, Value::Null) => v,
            ("int", Value::Float(f)) => Value::Int(*f as i64),
            ("int", Value::Int(_)) => v,
            ("int", _) => return bad("int", &v),
            ("float", Value::Int(i)) => Value::Float(*i as f64),
            ("float", Value::Float(_)) => v,
            ("float", _) => return bad("float", &v),
            ("string", Value::Str(_)) => v,
            ("string", _) => Value::str(&v.to_string()),
            ("bool", _) => Value::Bool(v.truthy()),
            ("list", Value::List(_)) | ("map", Value::Map(_)) | ("color", Value::Color(_)) => v,
            ("list" | "map" | "color", _) => return bad(&ty.name, &v),
            // Sized integers (Rust and HolyC names) wrap to their width.
            (t @ ("u8" | "U8" | "u16" | "U16" | "u32" | "U32" | "i8" | "I8" | "i16" | "I16" | "i32" | "I32"), x) => {
                let Some(i) = x.as_i64() else { return bad(t, &v) };
                Value::Int(match t.to_ascii_lowercase().as_str() {
                    "u8" => i as u8 as i64,
                    "u16" => i as u16 as i64,
                    "u32" => i as u32 as i64,
                    "i8" => i as i8 as i64,
                    "i16" => i as i16 as i64,
                    _ => i as i32 as i64,
                })
            }
            ("i64" | "I64" | "u64" | "U64" | "usize" | "isize", x) => match x.as_i64() {
                Some(i) => Value::Int(i),
                None => return bad(&ty.name, &v),
            },
            ("f64" | "F64" | "f32" | "F32", x) => match x.as_f64() {
                Some(f) => Value::Float(f),
                None => return bad(&ty.name, &v),
            },
            ("Bool", _) => Value::Bool(v.truthy()),
            ("U0", _) => Value::Null,
            ("str" | "String", _) => Value::str(&v.to_string()),
            _ => return self.coerce_spec(ty, v, line),
        })
    }

    /// The type an annotation checks, or `None` for the original converting
    /// names (`int`, `string`, `u8`…) and names this program doesn't define.
    pub fn checked_spec(&self, ty: &Type) -> Option<TypeSpec> {
        const CONVERTING: &[&str] = &[
            "int", "float", "string", "bool", "list", "map", "color", "u8", "U8", "u16", "U16", "u32", "U32", "i8", "I8", "i16", "I16", "i32", "I32",
            "i64", "I64", "u64", "U64", "usize", "isize", "f64", "F64", "f32", "F32", "Bool", "U0", "str", "String",
        ];
        let name = if ty.array && !ty.name.contains('<') { format!("list<{}>", ty.name) } else { ty.name.clone() };
        if CONVERTING.contains(&name.as_str()) {
            return None;
        }
        TypeSpec::parse(&name).ok().filter(|t| self.knows(t))
    }

    /// The type a native declaration (`name: T = v`) checks: every name,
    /// old or new, except names the program doesn't define.
    fn native_spec(&self, ty: &Type) -> Option<TypeSpec> {
        let name = if ty.array && !ty.name.contains('<') { format!("list<{}>", ty.name) } else { ty.name.clone() };
        TypeSpec::parse(&name).ok().filter(|t| self.knows(t))
    }

    /// Checks, before `Main` runs, every declaration whose value is a
    /// literal against its checked type (`n: int = "seven"`,
    /// `let s: text = 5`), so the mistake is reported even on a path that
    /// hasn't run yet. Original converting annotations aren't checked here.
    pub fn check_declarations(&self, items: &[Item]) -> Result<(), String> {
        let mut errors = vec![];
        let mut funcs: Vec<&FuncDef> = vec![];
        for item in items {
            match item {
                Item::Func(f) => funcs.push(f),
                Item::Impl { methods, .. } | Item::Trait { methods, .. } => funcs.extend(methods.iter().map(|m| &**m)),
                Item::On { body, .. } => self.check_stmts(body, &mut errors),
                Item::Stmt(s) => self.check_stmts(std::slice::from_ref(s), &mut errors),
                _ => {}
            }
        }
        for f in funcs {
            if let Some(body) = &f.body {
                self.check_stmts(body, &mut errors);
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("\n"))
        }
    }

    fn check_stmts(&self, stmts: &[Stmt], errors: &mut Vec<String>) {
        for s in stmts {
            match s {
                Stmt::Let { ty: Some(ty), value: Some(e), native, line, .. } => {
                    let spec = if *native { self.native_spec(ty) } else { self.checked_spec(ty) };
                    if let (Some(t), Some(v)) = (spec, literal(e)) {
                        if !t.accepts(&v, Some(self)) {
                            errors.push(format!("line {line}: expected {t}, got {} ({v})", v.type_name()));
                        }
                    }
                }
                Stmt::If { then, els, .. } => {
                    self.check_stmts(then, errors);
                    if let Some(e) = els {
                        self.check_stmts(e, errors);
                    }
                }
                Stmt::While { body, .. } | Stmt::For { body, .. } | Stmt::Block(body) => self.check_stmts(body, errors),
                Stmt::Try { body, catch, .. } => {
                    self.check_stmts(body, errors);
                    self.check_stmts(catch, errors);
                }
                Stmt::Switch { cases, default, .. } => {
                    for c in cases {
                        self.check_stmts(&c.body, errors);
                    }
                    if let Some(d) = default {
                        self.check_stmts(d, errors);
                    }
                }
                _ => {}
            }
        }
    }

    /// `type_of`: a value's type, with enum variants reported as their enum.
    pub fn type_of(&self, v: &Value) -> TypeSpec {
        match TypeSpec::of(v) {
            TypeSpec::Named(n) => TypeSpec::Named(self.variant_of.get(&n).cloned().unwrap_or(n)),
            t => t,
        }
    }

    /// Does type `ty` (a struct or enum variant) implement trait `t`?
    pub fn implements(&self, ty: &str, t: &str) -> bool {
        let has = |n: &str| self.implemented.get(n).is_some_and(|ts| ts.iter().any(|x| x == t));
        has(ty) || self.variant_of.get(ty).is_some_and(|e| has(e))
    }

    /// Annotations beyond the original names (`text`, `number`, `byte`,
    /// `list<int>`, `map<text, int>`, `int?`, struct names…) are checked,
    /// not converted: `let n: text = 5` is an error. An int is accepted
    /// where a float is expected. Names that aren't types this program
    /// knows (host objects, typos in old code) are let through as before.
    fn coerce_spec(&self, ty: &Type, v: Value, line: usize) -> Flow<Value> {
        let Some(spec) = self.checked_spec(ty) else { return Ok(v) };
        if spec.accepts(&v, Some(self)) {
            return Ok(spec.widen(v));
        }
        throw(format!("line {line}: expected {spec}, got {} ({})", v.type_name(), v))
    }

    /// Can every name in `spec` be checked? Unknown names are host types or
    /// old annotations that never meant anything; those aren't enforced.
    fn knows(&self, spec: &TypeSpec) -> bool {
        match spec {
            TypeSpec::Named(n) => self.structs.contains_key(n) || self.enums.contains_key(n) || self.traits.contains_key(n),
            t => t.params().iter().all(|p| self.knows(p)),
        }
    }

    pub fn exec(&mut self, s: &Stmt, env: &Env) -> Flow<()> {
        self.tick()?;
        match s {
            Stmt::Let { name, ty, value, global, constant, native, line } => {
                let mut v = match value {
                    Some(e) => self.eval(e, env)?,
                    None if ty.as_ref().is_some_and(|t| t.array) => Value::list(vec![]),
                    None => match ty.as_ref().map(|t| t.name.as_str()) {
                        Some("int") => Value::Int(0),
                        Some("float") => Value::Float(0.0),
                        Some("string") => Value::str(""),
                        Some("bool") => Value::Bool(false),
                        Some("list") => Value::list(vec![]),
                        Some("map") => Value::map(BTreeMap::new()),
                        Some("text") => Value::str(""),
                        Some("number" | "byte") => Value::Int(0),
                        Some(t) if t.starts_with("array") => Value::list(vec![]),
                        Some(t) if t.starts_with("map<") => Value::map(BTreeMap::new()),
                        _ => Value::Null,
                    },
                };
                // The type this variable keeps: a checked annotation, or the
                // first value's type for `:=`.
                let keep = match ty {
                    Some(t) if *native => self.native_spec(t),
                    Some(t) => {
                        v = self.coerce(t, v, *line)?;
                        self.checked_spec(t)
                    }
                    None if *native && !matches!(v, Value::Null) => Some(self.type_of(&v)),
                    None => None,
                };
                if let (Some(t), true) = (&keep, *native) {
                    if !t.accepts(&v, Some(self)) {
                        return throw(format!("line {line}: expected {t}, got {} ({v})", v.type_name()));
                    }
                    v = t.widen(v);
                }
                let target = if *global { self.globals.clone() } else { env.clone() };
                self.declare(&target, name, v, *constant);
                if let Some(t) = keep {
                    target.borrow_mut().types.insert(name.clone(), Rc::new(t));
                }
                Ok(())
            }
            Stmt::Assign { target, op, value, line } => {
                let rhs = self.eval(value, env)?;
                let v = if *op == "=" {
                    rhs
                } else {
                    let cur = self.eval(target, env)?;
                    self.binary(&op[..1], cur, rhs, *line)?
                };
                self.assign(target, v, env, *line)
            }
            Stmt::Expr(e, _) => {
                self.eval(e, env)?;
                Ok(())
            }
            Stmt::If { cond, then, els, .. } => {
                if self.eval(cond, env)?.truthy() {
                    self.exec_block(then, &new_scope(Some(env.clone())))
                } else if let Some(e) = els {
                    self.exec_block(e, &new_scope(Some(env.clone())))
                } else {
                    Ok(())
                }
            }
            Stmt::While { cond, body, .. } => {
                while self.eval(cond, env)?.truthy() {
                    match self.exec_block(body, &new_scope(Some(env.clone()))) {
                        Err(Ctrl::Break) => break,
                        Err(Ctrl::Continue) | Ok(()) => {}
                        Err(e) => return Err(e),
                    }
                }
                Ok(())
            }
            Stmt::For { var, index, iter, body, line } => {
                let items: Vec<Value> = match self.eval(iter, env)? {
                    Value::Range(a, b) => {
                        // Lazily, so big ranges don't allocate.
                        let mut i = a;
                        while i < b {
                            let scope = new_scope(Some(env.clone()));
                            match index {
                                Some(ix) => {
                                    self.declare(&scope, var, Value::Int(i - a), false);
                                    self.declare(&scope, ix, Value::Int(i), false);
                                }
                                None => self.declare(&scope, var, Value::Int(i), false),
                            }
                            match self.exec_block(body, &scope) {
                                Err(Ctrl::Break) => break,
                                Err(Ctrl::Continue) | Ok(()) => {}
                                Err(e) => return Err(e),
                            }
                            i += 1;
                        }
                        return Ok(());
                    }
                    Value::Int(n) => (0..n).map(Value::Int).collect(),
                    Value::List(l) => l.borrow().clone(),
                    Value::Map(m) => m.borrow().keys().map(|k| Value::str(k)).collect(),
                    Value::Str(s) => s.chars().map(|c| Value::str(&c.to_string())).collect(),
                    other => return throw(format!("line {line}: can't loop over {}", other.type_name())),
                };
                for (i, v) in items.into_iter().enumerate() {
                    let scope = new_scope(Some(env.clone()));
                    match index {
                        Some(ix) => {
                            self.declare(&scope, var, Value::Int(i as i64), false);
                            self.declare(&scope, ix, v, false);
                        }
                        None => self.declare(&scope, var, v, false),
                    }
                    match self.exec_block(body, &scope) {
                        Err(Ctrl::Break) => break,
                        Err(Ctrl::Continue) | Ok(()) => {}
                        Err(e) => return Err(e),
                    }
                }
                Ok(())
            }
            Stmt::Return(v, _) => {
                let v = match v {
                    Some(e) => self.eval(e, env)?,
                    None => Value::Null,
                };
                Err(Ctrl::Return(v))
            }
            Stmt::Break(_) => Err(Ctrl::Break),
            Stmt::Continue(_) => Err(Ctrl::Continue),
            Stmt::Throw(e, _) => {
                let v = self.eval(e, env)?;
                Err(Ctrl::Throw(v))
            }
            Stmt::Try { body, var, catch, .. } => match self.exec_block(body, &new_scope(Some(env.clone()))) {
                Err(Ctrl::Throw(v)) => {
                    let scope = new_scope(Some(env.clone()));
                    if let Some(n) = var {
                        self.declare(&scope, n, v, false);
                    }
                    self.exec_block(catch, &scope)
                }
                other => other,
            },
            Stmt::Block(b) => self.exec_block(b, &new_scope(Some(env.clone()))),
            Stmt::Destructure { names, value, declare, line } => {
                let v = self.eval(value, env)?;
                let items = match &v {
                    Value::List(l) => l.borrow().clone(),
                    Value::Struct(st) => st.borrow().fields.iter().map(|(_, v)| v.clone()).collect(),
                    other => return throw(format!("line {line}: can't unpack {} into {} names", other.type_name(), names.len())),
                };
                if items.len() != names.len() {
                    return throw(format!("line {line}: {} values to unpack into {} names", items.len(), names.len()));
                }
                for (n, x) in names.iter().zip(items) {
                    if *declare {
                        self.declare(env, n, x, false);
                    } else {
                        self.assign(&Expr::Var(n.clone(), *line), x, env, *line)?;
                    }
                }
                Ok(())
            }
            Stmt::Print { fmt, args, line } => {
                let f = self.eval(fmt, env)?.to_string();
                let mut a = Vec::with_capacity(args.len());
                for x in args {
                    a.push(self.eval(x, env)?);
                }
                let text = if a.is_empty() { f } else { crate::stdlib::printf(&f, &a).map_err(|e| Ctrl::Throw(Value::str(&format!("line {line}: {e}"))))? };
                self.output.push(text);
                Ok(())
            }
            Stmt::Bare(e, line) => {
                let v = self.eval(e, env)?;
                if matches!(v, Value::Func(_) | Value::Native(_)) {
                    self.call_value(&v, vec![], *line)?;
                }
                Ok(())
            }
            Stmt::Switch { value, cases, default, .. } => {
                let v = self.eval(value, env)?;
                let mut body = None;
                'find: for c in cases {
                    for (a, b) in &c.pats {
                        let av = self.eval(a, env)?;
                        let hit = match b {
                            Some(b) => {
                                let bv = self.eval(b, env)?;
                                match (v.as_f64(), av.as_f64(), bv.as_f64()) {
                                    (Some(x), Some(lo), Some(hi)) => x >= lo && x <= hi,
                                    _ => false,
                                }
                            }
                            None => av.equals(&v),
                        };
                        if hit {
                            body = Some(&c.body);
                            break 'find;
                        }
                    }
                }
                let body = body.or(default.as_ref());
                match body {
                    Some(b) => match self.exec_block(b, &new_scope(Some(env.clone()))) {
                        Err(Ctrl::Break) | Ok(()) => Ok(()),
                        Err(e) => Err(e),
                    },
                    None => Ok(()),
                }
            }
        }
    }

    /// The items `for` and comprehensions walk.
    fn iter_items(&mut self, v: Value, line: usize) -> Flow<Vec<Value>> {
        Ok(match v {
            Value::Range(a, b) => {
                self.check_len((b - a).max(0) as usize).map_err(|m| Ctrl::Throw(Value::str(&m)))?;
                (a..b).map(Value::Int).collect()
            }
            Value::Int(n) => (0..n).map(Value::Int).collect(),
            Value::List(l) => l.borrow().clone(),
            Value::Map(m) => m.borrow().keys().map(|k| Value::str(k)).collect(),
            Value::Str(s) => s.chars().map(|c| Value::str(&c.to_string())).collect(),
            Value::Object(o) => match o.call(self, "tolist", vec![]) {
                Ok(Value::List(l)) => l.borrow().clone(),
                _ => return throw(format!("line {line}: can't loop over {}", o.type_name())),
            },
            other => return throw(format!("line {line}: can't loop over {}", other.type_name())),
        })
    }

    /// Bind loop/comprehension variables: one name takes the item, several unpack it.
    fn bind_vars(&mut self, scope: &Env, vars: &[String], item: Value, line: usize) -> Flow<()> {
        if vars.len() == 1 {
            self.declare(scope, &vars[0], item, false);
            return Ok(());
        }
        let parts = match &item {
            Value::List(l) => l.borrow().clone(),
            other => return throw(format!("line {line}: can't unpack {} into {} names", other.type_name(), vars.len())),
        };
        if parts.len() != vars.len() {
            return throw(format!("line {line}: {} values to unpack into {} names", parts.len(), vars.len()));
        }
        for (n, p) in vars.iter().zip(parts) {
            self.declare(scope, n, p, false);
        }
        Ok(())
    }

    /// Match `v` against a pattern, binding names into `scope`. Patterns:
    /// values and ranges (compared), `Variant(a, b)` / `Enum::Variant(a)` /
    /// `Some(x)` / `Ok(v)` / `Err(e)` (destructured; `_` skips a field).
    fn pattern(&mut self, p: &Expr, v: &Value, env: &Env, scope: &Env) -> Flow<bool> {
        let ctor = |e: &Expr| -> Option<String> {
            match e {
                Expr::Var(n, _) => Some(n.clone()),
                Expr::Path(_, n, _) => Some(n.clone()),
                _ => None,
            }
        };
        if let Expr::Call(callee, args, line) = p {
            if let Some(name) = ctor(callee) {
                if name == "Some" {
                    if matches!(v, Value::Null) {
                        return Ok(false);
                    }
                    if let Some(a) = args.first() {
                        return self.sub_pattern(a, v, env, scope);
                    }
                    return Ok(true);
                }
                if self.structs.contains_key(&name) || matches!(name.as_str(), "Ok" | "Err") {
                    let Value::Struct(st) = v else { return Ok(false) };
                    if st.borrow().name != name {
                        return Ok(false);
                    }
                    let fields: Vec<Value> = st.borrow().fields.iter().map(|(_, x)| x.clone()).collect();
                    if args.len() > fields.len() {
                        return throw(format!("line {line}: `{name}` has {} field(s), the pattern has {}", fields.len(), args.len()));
                    }
                    for (a, f) in args.iter().zip(fields.iter()) {
                        if !self.sub_pattern(a, f, env, scope)? {
                            return Ok(false);
                        }
                    }
                    return Ok(true);
                }
            }
        }
        let pv = self.eval(p, env)?;
        Ok(match (&pv, v) {
            (Value::Range(a, b), x) => x.as_i64().is_some_and(|x| x >= *a && x < *b) && !matches!(x, Value::Range(..)),
            _ => pv.equals(v),
        })
    }

    fn sub_pattern(&mut self, p: &Expr, v: &Value, env: &Env, scope: &Env) -> Flow<bool> {
        match p {
            Expr::Var(n, _) if n == "_" => Ok(true),
            // A lower-case name in a destructuring pattern binds the field.
            Expr::Var(n, _) if n.starts_with(|c: char| c.is_lowercase() || c == '_') => {
                self.declare(scope, n, v.clone(), false);
                Ok(true)
            }
            other => self.pattern(other, v, env, scope),
        }
    }

    fn assign(&mut self, target: &Expr, v: Value, env: &Env, line: usize) -> Flow<()> {
        match target {
            Expr::Var(name, _) => {
                let mut cur = Some(env.clone());
                while let Some(s) = cur {
                    if s.borrow().vars.contains_key(name) {
                        if s.borrow().consts.contains(name) {
                            return throw(format!("line {line}: `{name}` is a constant"));
                        }
                        let kept = s.borrow().types.get(name).cloned();
                        if let Some(t) = kept {
                            if !t.accepts(&v, Some(self)) {
                                return throw(format!("line {line}: `{name}` is {t}, can't assign {} ({v})", v.type_name()));
                            }
                            s.borrow_mut().vars.insert(name.clone(), t.widen(v));
                            return Ok(());
                        }
                        s.borrow_mut().vars.insert(name.clone(), v);
                        return Ok(());
                    }
                    cur = s.borrow().parent.clone();
                }
                // Z#: assigning an undeclared name declares it here.
                self.declare(env, name, v, false);
                Ok(())
            }
            Expr::Index(obj, idx) => {
                let o = self.eval(obj, env)?;
                let i = self.eval(idx, env)?;
                match o {
                    Value::List(l) => {
                        let mut l = l.borrow_mut();
                        let n = l.len() as i64;
                        let k = i.as_i64().ok_or(Ctrl::Throw(Value::str("a list index must be a number")))?;
                        let k = if k < 0 { n + k } else { k };
                        if k < 0 || k >= n {
                            return throw(format!("line {line}: index {k} is outside the list (length {n})"));
                        }
                        l[k as usize] = v;
                        Ok(())
                    }
                    Value::Map(m) => {
                        m.borrow_mut().insert(i.to_string(), v);
                        Ok(())
                    }
                    Value::Object(obj) => match obj.set_index(self, &[Index::At(i)], v) {
                        Some(r) => r.map_err(|m| Ctrl::Throw(Value::str(&format!("line {line}: {m}")))),
                        None => throw(format!("line {line}: can't assign into {}", obj.type_name())),
                    },
                    other => throw(format!("line {line}: can't index into {}", other.type_name())),
                }
            }
            Expr::Subscript(obj, subs) => {
                let o = self.eval(obj, env)?;
                let parts = self.eval_subs(subs, env)?;
                match (&o, parts.as_slice()) {
                    (Value::Object(ob), _) => match ob.set_index(self, &parts, v) {
                        Some(r) => r.map_err(|m| Ctrl::Throw(Value::str(&format!("line {line}: {m}")))),
                        None => throw(format!("line {line}: can't assign into {}", ob.type_name())),
                    },
                    // `xs[1:3] = [a, b, c]` replaces that run (Python).
                    (Value::List(l), [Index::Slice(a, b, None)]) => {
                        let new = match &v {
                            Value::List(n) => n.borrow().clone(),
                            other => return throw(format!("line {line}: assign a list to a slice, not {}", other.type_name())),
                        };
                        let mut l = l.borrow_mut();
                        let ix = slice_indices(l.len(), *a, *b, None).map_err(|m| Ctrl::Throw(Value::str(&m)))?;
                        let (s0, s1) = match (ix.first(), ix.last()) {
                            (Some(f), Some(e)) => (*f, e + 1),
                            _ => {
                                let s = a.map(|x| if x < 0 { (l.len() as i64 + x).max(0) } else { x.min(l.len() as i64) }).unwrap_or(0) as usize;
                                (s, s)
                            }
                        };
                        l.splice(s0..s1, new);
                        self.check_len(l.len()).map_err(|m| Ctrl::Throw(Value::str(&m)))
                    }
                    (Value::List(_), parts) => {
                        // `grid[i, j] = v` on nested lists
                        let (last, init) = parts.split_last().unwrap();
                        let mut cur = o.clone();
                        for p in init {
                            let Index::At(i) = p else { return throw(format!("line {line}: can't assign through a slice")) };
                            cur = self.index(cur, i.clone())?;
                        }
                        let Index::At(i) = last else { return throw(format!("line {line}: can't assign a slice there")) };
                        match cur {
                            Value::List(l) => {
                                let mut l = l.borrow_mut();
                                let n = l.len() as i64;
                                let k = i.as_i64().ok_or(Ctrl::Throw(Value::str("a list index must be a number")))?;
                                let k = if k < 0 { n + k } else { k };
                                if k < 0 || k >= n {
                                    return throw(format!("line {line}: index {k} is outside the list (length {n})"));
                                }
                                l[k as usize] = v;
                                Ok(())
                            }
                            Value::Map(m) => {
                                m.borrow_mut().insert(i.to_string(), v);
                                Ok(())
                            }
                            other => throw(format!("line {line}: can't index into {}", other.type_name())),
                        }
                    }
                    (other, _) => throw(format!("line {line}: can't assign into {}", other.type_name())),
                }
            }
            Expr::Field(obj, name, _) => match self.eval(obj, env)? {
                Value::Struct(s) => {
                    let mut s = s.borrow_mut();
                    match s.fields.iter_mut().find(|(k, _)| k == name) {
                        Some(slot) => {
                            slot.1 = v;
                            Ok(())
                        }
                        None => throw(format!("line {line}: {} has no field `{name}`", s.name)),
                    }
                }
                Value::Map(m) => {
                    m.borrow_mut().insert(name.clone(), v);
                    Ok(())
                }
                other => throw(format!("line {line}: can't set `{name}` on {}", other.type_name())),
            },
            _ => throw(format!("line {line}: can't assign to that")),
        }
    }

    // ── Expressions ────────────────────────────────────────────────────

    fn lookup(&self, name: &str, env: &Env) -> Option<Value> {
        let mut cur = Some(env.clone());
        while let Some(s) = cur {
            if let Some(v) = s.borrow().vars.get(name) {
                return Some(v.clone());
            }
            cur = s.borrow().parent.clone();
        }
        None
    }

    pub fn eval(&mut self, e: &Expr, env: &Env) -> Flow<Value> {
        self.tick()?;
        Ok(match e {
            Expr::Null => Value::Null,
            Expr::Bool(b) => Value::Bool(*b),
            Expr::Int(i) => Value::Int(*i),
            Expr::Float(f) => Value::Float(*f),
            Expr::Color(c) => Value::Color(*c),
            Expr::Str(segs) => {
                let mut s = String::new();
                for seg in segs {
                    match seg {
                        StrSeg::Lit(l) => s.push_str(l),
                        StrSeg::Expr(e) => s.push_str(&self.eval(e, env)?.to_string()),
                    }
                }
                self.check_len(s.len()).map_err(|m| Ctrl::Throw(Value::str(&m)))?;
                Value::str(&s)
            }
            Expr::Var(name, line) => match self.lookup(name, env) {
                Some(v) => v,
                None => {
                    if self.structs.contains_key(name) {
                        let n = name.clone();
                        return Ok(Value::native(name, move |it, args| it.construct(&n, args)));
                    }
                    return throw(format!("line {line}: `{name}` isn't defined"));
                }
            },
            Expr::List(items) => {
                let mut v = Vec::with_capacity(items.len());
                for i in items {
                    v.push(self.eval(i, env)?);
                }
                Value::list(v)
            }
            Expr::Map(kv) => {
                let mut m = BTreeMap::new();
                for (k, v) in kv {
                    let k = self.eval(k, env)?.to_string();
                    m.insert(k, self.eval(v, env)?);
                }
                Value::map(m)
            }
            Expr::StructLit(name, fields) => {
                let Some(decl) = self.structs.get(name).cloned() else { return throw(format!("there's no struct `{name}`")) };
                let mut vals = vec![];
                for (f, ty) in &decl {
                    let v = match fields.iter().find(|(k, _)| k == f) {
                        Some((_, e)) => self.eval(e, env)?,
                        None => Value::Null,
                    };
                    let v = match ty {
                        Some(t) => self.coerce(t, v, 0)?,
                        None => v,
                    };
                    vals.push((f.clone(), v));
                }
                if let Some((bad, _)) = fields.iter().find(|(k, _)| !decl.iter().any(|(f, _)| f == k)) {
                    return throw(format!("{name} has no field `{bad}`"));
                }
                Value::Struct(Rc::new(RefCell::new(StructVal { name: name.clone(), fields: vals })))
            }
            Expr::Unary(op, x) => {
                let v = self.eval(x, env)?;
                match (*op, v) {
                    ("-", Value::Int(i)) => Value::Int(i.checked_neg().ok_or(Ctrl::Throw(Value::str("number too large")))?),
                    ("-", Value::Float(f)) => Value::Float(-f),
                    ("-", Value::Object(o)) => match o.negate(self) {
                        Some(r) => r.map_err(|m| Ctrl::Throw(Value::str(&m)))?,
                        None => return throw(format!("can't negate {}", o.type_name())),
                    },
                    ("!", v) => Value::Bool(!v.truthy()),
                    (_, v) => return throw(format!("can't negate {}", v.type_name())),
                }
            }
            Expr::Binary(op, a, b) => match *op {
                "&&" => {
                    let l = self.eval(a, env)?;
                    if !l.truthy() {
                        l
                    } else {
                        self.eval(b, env)?
                    }
                }
                "||" => {
                    let l = self.eval(a, env)?;
                    if l.truthy() {
                        l
                    } else {
                        self.eval(b, env)?
                    }
                }
                "??" => {
                    let l = self.eval(a, env)?;
                    if matches!(l, Value::Null) {
                        self.eval(b, env)?
                    } else {
                        l
                    }
                }
                _ => {
                    let l = self.eval(a, env)?;
                    let r = self.eval(b, env)?;
                    self.binary(op, l, r, 0)?
                }
            },
            Expr::Ternary(c, a, b) => {
                if self.eval(c, env)?.truthy() {
                    self.eval(a, env)?
                } else {
                    self.eval(b, env)?
                }
            }
            Expr::Range(a, b, incl) => {
                let a = self.eval(a, env)?.as_i64().ok_or(Ctrl::Throw(Value::str("a range needs whole numbers")))?;
                let b = self.eval(b, env)?.as_i64().ok_or(Ctrl::Throw(Value::str("a range needs whole numbers")))?;
                Value::Range(a, if *incl { b + 1 } else { b })
            }
            Expr::Index(o, i) => {
                let o = self.eval(o, env)?;
                let i = self.eval(i, env)?;
                self.index(o, i)?
            }
            Expr::Field(o, name, opt) => {
                let o = self.eval(o, env)?;
                if *opt && matches!(o, Value::Null) {
                    return Ok(Value::Null);
                }
                self.field(&o, name)?
            }
            Expr::Lambda(def) => Value::Func(Rc::new(Closure { def: def.clone(), env: env.clone() })),
            Expr::Match(v, arms) => {
                let v = self.eval(v, env)?;
                for arm in arms {
                    let scope = new_scope(Some(env.clone()));
                    let mut hit = arm.pats.is_empty();
                    for p in &arm.pats {
                        if self.pattern(p, &v, env, &scope)? {
                            hit = true;
                            break;
                        }
                    }
                    if hit {
                        if let Some(g) = &arm.guard {
                            if !self.eval(g, &scope)?.truthy() {
                                continue;
                            }
                        }
                        return self.block_value(&arm.body, &scope);
                    }
                }
                Value::Null
            }
            Expr::Comp { elem, value, vars, iter, cond } => {
                let line = 0;
                let src = self.eval(iter, env)?;
                let items = self.iter_items(src, line)?;
                let mut list = vec![];
                let mut map = BTreeMap::new();
                for item in items {
                    let scope = new_scope(Some(env.clone()));
                    self.bind_vars(&scope, vars, item, line)?;
                    if let Some(c) = cond {
                        if !self.eval(c, &scope)?.truthy() {
                            continue;
                        }
                    }
                    match value {
                        Some(vexpr) => {
                            let k = self.eval(elem, &scope)?.to_string();
                            map.insert(k, self.eval(vexpr, &scope)?);
                        }
                        None => list.push(self.eval(elem, &scope)?),
                    }
                }
                if value.is_some() {
                    Value::map(map)
                } else {
                    Value::list(list)
                }
            }
            Expr::Subscript(o, subs) => {
                let o = self.eval(o, env)?;
                let parts = self.eval_subs(subs, env)?;
                self.subscript(o, &parts)?
            }
            Expr::Try(x, line) => {
                let v = self.eval(x, env)?;
                match &v {
                    Value::Null => return Err(Ctrl::Return(Value::Null)),
                    Value::Struct(st) if st.borrow().name == "Err" => return Err(Ctrl::Return(v.clone())),
                    Value::Struct(st) if st.borrow().name == "Ok" => st.borrow().fields.first().map(|f| f.1.clone()).unwrap_or(Value::Null),
                    _ => {
                        let _ = line;
                        v
                    }
                }
            }
            Expr::Named(n, _) => return throw(format!("`{n}=…` only goes in a call's arguments")),
            Expr::Path(e, v, line) => {
                let ok = self.enums.get(e).is_some_and(|vs| vs.iter().any(|(n, _)| n == v));
                if !ok {
                    return throw(format!("line {line}: `{e}` has no variant `{v}`"));
                }
                self.global(v).unwrap_or(Value::Null)
            }
            Expr::Call(callee, args, line) => {
                // Method call: obj.name(args)
                if let Expr::Field(obj, name, opt) = &**callee {
                    let o = self.eval(obj, env)?;
                    if *opt && matches!(o, Value::Null) {
                        return Ok(Value::Null);
                    }
                    let mut a = Vec::with_capacity(args.len());
                    let mut named = BTreeMap::new();
                    for x in args {
                        match x {
                            Expr::Named(n, v) => {
                                let v = self.eval(v, env)?;
                                named.insert(n.clone(), v);
                            }
                            x => a.push(self.eval(x, env)?),
                        }
                    }
                    // Keyword arguments to a method arrive as a final map.
                    if !named.is_empty() {
                        a.push(Value::map(named));
                    }
                    return self.call_method(o, name, a, *line);
                }
                let f = self.eval(callee, env)?;
                let mut a = Vec::with_capacity(args.len());
                let mut named: Vec<(String, Value)> = vec![];
                for x in args {
                    match x {
                        Expr::Named(n, v) => {
                            let v = self.eval(v, env)?;
                            named.push((n.clone(), v));
                        }
                        x => a.push(self.eval(x, env)?),
                    }
                }
                if named.is_empty() {
                    self.call_value(&f, a, *line)?
                } else {
                    self.call_named(&f, a, named, *line)?
                }
            }
        })
    }

    pub fn construct(&mut self, name: &str, args: Vec<Value>) -> Result<Value, String> {
        let decl = self.structs.get(name).cloned().ok_or(format!("there's no struct `{name}`"))?;
        if args.len() > decl.len() {
            return Err(format!("{name} has {} fields, got {} values", decl.len(), args.len()));
        }
        let fields = decl.iter().enumerate().map(|(i, (f, _))| (f.clone(), args.get(i).cloned().unwrap_or(Value::Null))).collect();
        Ok(Value::Struct(Rc::new(RefCell::new(StructVal { name: name.into(), fields }))))
    }

    pub fn call_value(&mut self, f: &Value, args: Vec<Value>, line: usize) -> Flow<Value> {
        match f {
            Value::Func(c) => self.call_closure(c.clone(), None, args, line),
            Value::Native(n) => {
                let n = n.clone();
                (n.f)(self, args).map_err(|m| Ctrl::Throw(Value::str(&if line > 0 { format!("line {line}: {m}") } else { m })))
            }
            other => throw(format!("line {line}: {} can't be called", other.type_name())),
        }
    }

    /// A call with keyword arguments: a script function binds them to its
    /// parameters by name; a built-in (or extender) function gets them as
    /// a final map.
    pub fn call_named(&mut self, f: &Value, args: Vec<Value>, named: Vec<(String, Value)>, line: usize) -> Flow<Value> {
        match f {
            Value::Func(c) => {
                let def = c.def.clone();
                let params: Vec<&Param> = def.params.iter().filter(|p| p.name != "self").collect();
                if args.len() > params.len() {
                    return throw(format!("line {line}: `{}` takes {} argument(s), got {}", def.name, params.len(), args.len()));
                }
                let mut slots: Vec<Option<Value>> = args.into_iter().map(Some).collect();
                slots.resize(params.len(), None);
                for (n, v) in named {
                    let Some(i) = params.iter().position(|p| p.name == n) else {
                        return throw(format!("line {line}: `{}` has no parameter `{n}`", def.name));
                    };
                    if slots[i].is_some() {
                        return throw(format!("line {line}: `{n}` was given twice"));
                    }
                    slots[i] = Some(v);
                }
                self.call_closure_slots(c.clone(), None, slots, line)
            }
            _ => {
                let mut a = args;
                a.push(Value::map(named.into_iter().collect()));
                self.call_value(f, a, line)
            }
        }
    }

    fn call_closure(&mut self, c: Rc<Closure>, this: Option<Value>, args: Vec<Value>, line: usize) -> Flow<Value> {
        let n = c.def.params.iter().filter(|p| p.name != "self").count();
        let mut slots: Vec<Option<Value>> = args.into_iter().map(Some).collect();
        if slots.len() < n {
            slots.resize(n, None);
        }
        self.call_closure_slots(c, this, slots, line)
    }

    /// Call with one slot per parameter (`None`: use the default or null).
    fn call_closure_slots(&mut self, c: Rc<Closure>, this: Option<Value>, args: Vec<Option<Value>>, line: usize) -> Flow<Value> {
        if self.depth >= self.limits.depth {
            return Err(Ctrl::Fatal(format!("calls nested deeper than {} (runaway recursion?)", self.limits.depth)));
        }
        let def = &c.def;
        let Some(body) = &def.body else { return throw(format!("`{}` is only declared", def.name)) };
        let env = new_scope(Some(c.env.clone()));
        let mut args = args.into_iter();
        for p in &def.params {
            let v = if p.name == "self" {
                this.clone().unwrap_or(Value::Null)
            } else {
                match args.next().flatten() {
                    Some(v) => v,
                    None => match &p.default {
                        Some(d) => self.eval(d, &env)?,
                        None => Value::Null,
                    },
                }
            };
            let v = match &p.ty {
                Some(t) => self.coerce(t, v, line)?,
                None => v,
            };
            env.borrow_mut().vars.insert(p.name.clone(), v);
        }
        self.depth += 1;
        let r = self.exec_block(body, &env);
        self.depth -= 1;
        let v = match r {
            Ok(()) => Value::Null,
            Err(Ctrl::Return(v)) => v,
            Err(e) => return Err(e),
        };
        match &def.ret {
            Some(t) => self.coerce(t, v, line),
            None => Ok(v),
        }
    }

    pub fn call_method(&mut self, o: Value, name: &str, args: Vec<Value>, line: usize) -> Flow<Value> {
        if let Value::Struct(s) = &o {
            let ty = s.borrow().name.clone();
            if let Some(m) = self.methods.get(&ty).and_then(|m| m.get(name)).cloned() {
                let c = Rc::new(Closure { def: m, env: self.globals.clone() });
                return self.call_closure(c, Some(o.clone()), args, line);
            }
            let field = s.borrow().fields.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());
            if let Some(f) = field {
                return self.call_value(&f, args, line);
            }
        }
        if let Value::Map(m) = &o {
            let f = m.borrow().get(name).cloned();
            if let Some(f @ (Value::Func(_) | Value::Native(_))) = f {
                return self.call_value(&f, args, line);
            }
        }
        if let Value::Object(obj) = &o {
            let obj = obj.clone();
            return obj.call(self, name, args).map_err(|m| Ctrl::Throw(Value::str(&format!("line {line}: {m}"))));
        }
        // Built-in methods: `xs.map(f)` == `map(xs, f)`.
        let mut a = vec![o.clone()];
        a.extend(args);
        match crate::stdlib::method(self, &o, name) {
            Some(f) => self.call_value(&f, a, line),
            None => throw(format!("line {line}: {} has no method `{name}`", o.type_name())),
        }
    }

    pub fn field(&mut self, o: &Value, name: &str) -> Flow<Value> {
        Ok(match o {
            Value::Struct(s) => match s.borrow().fields.iter().find(|(k, _)| k == name) {
                Some((_, v)) => v.clone(),
                None => return throw(format!("{} has no field `{name}`", s.borrow().name)),
            },
            Value::Map(m) => m.borrow().get(name).cloned().unwrap_or(Value::Null),
            Value::Color(c) => {
                let (r, g, b, a) = color_parts(*c);
                match name {
                    "r" => Value::Int(r as i64),
                    "g" => Value::Int(g as i64),
                    "b" => Value::Int(b as i64),
                    "a" => Value::Int(a as i64),
                    _ => return throw(format!("a color has r, g, b and a, not `{name}`")),
                }
            }
            Value::List(l) if name == "length" || name == "len" => Value::Int(l.borrow().len() as i64),
            Value::Str(s) if name == "length" || name == "len" => Value::Int(s.chars().count() as i64),
            Value::Object(obj) => match obj.field(name) {
                Some(v) => v,
                None => return throw(format!("{} has no field `{name}`", obj.type_name())),
            },
            other => return throw(format!("{} has no field `{name}`", other.type_name())),
        })
    }

    fn index(&mut self, o: Value, i: Value) -> Flow<Value> {
        Ok(match (&o, &i) {
            (Value::List(l), _) => {
                let l = l.borrow();
                let n = l.len() as i64;
                let Some(k) = i.as_i64() else { return throw("a list index must be a number") };
                let k = if k < 0 { n + k } else { k };
                if k < 0 || k >= n {
                    return throw(format!("index {k} is outside the list (length {n})"));
                }
                l[k as usize].clone()
            }
            (Value::Map(m), _) => m.borrow().get(&i.to_string()).cloned().unwrap_or(Value::Null),
            (Value::Object(obj), _) => {
                let obj = obj.clone();
                match obj.index(self, &[Index::At(i.clone())]) {
                    Some(r) => r.map_err(|m| Ctrl::Throw(Value::str(&m)))?,
                    None => return throw(format!("can't index into {}", obj.type_name())),
                }
            }
            (Value::Str(s), Value::Range(a, b)) => {
                let chars: Vec<char> = s.chars().collect();
                let (a, b) = ((*a).clamp(0, chars.len() as i64) as usize, (*b).clamp(0, chars.len() as i64) as usize);
                Value::str(&chars[a..b.max(a)].iter().collect::<String>())
            }
            (Value::Str(s), _) => {
                let Some(k) = i.as_i64() else { return throw("a string index must be a number") };
                // Negative counts from the end, as for lists.
                let k = if k < 0 { k + s.chars().count() as i64 } else { k };
                if k < 0 {
                    return throw(format!("index {k} is outside the string"));
                }
                match s.chars().nth(k as usize) {
                    Some(c) => Value::str(&c.to_string()),
                    None => return throw(format!("index {k} is outside the string")),
                }
            }
            _ => return throw(format!("can't index into {}", o.type_name())),
        })
    }

    fn eval_subs(&mut self, subs: &[Sub], env: &Env) -> Flow<Vec<Index>> {
        let mut out = vec![];
        for s in subs {
            out.push(match s {
                Sub::At(e) => Index::At(self.eval(e, env)?),
                Sub::Slice(a, b, c) => {
                    let n = |e: &Option<Expr>, me: &mut Self| -> Flow<Option<i64>> {
                        match e {
                            None => Ok(None),
                            Some(e) => match me.eval(e, env)? {
                                Value::Null => Ok(None),
                                v => v.as_i64().map(Some).ok_or(Ctrl::Throw(Value::str("a slice needs whole numbers"))),
                            },
                        }
                    };
                    let (a, b, c) = (n(a, self)?, n(b, self)?, n(c, self)?);
                    Index::Slice(a, b, c)
                }
            });
        }
        Ok(out)
    }

    /// `o[parts]` with slices and several parts (`a[1:3]`, `grid[1, 2]`).
    fn subscript(&mut self, o: Value, parts: &[Index]) -> Flow<Value> {
        if let Value::Object(obj) = &o {
            let obj = obj.clone();
            return match obj.index(self, parts) {
                Some(r) => r.map_err(|m| Ctrl::Throw(Value::str(&m))),
                None => throw(format!("can't index into {}", obj.type_name())),
            };
        }
        let Some((first, rest)) = parts.split_first() else { return Ok(o) };
        let v = match first {
            Index::At(i) => self.index(o, i.clone())?,
            Index::Slice(a, b, c) => match &o {
                Value::List(l) => {
                    let l = l.borrow();
                    let ix = slice_indices(l.len(), *a, *b, *c).map_err(|m| Ctrl::Throw(Value::str(&m)))?;
                    Value::list(ix.into_iter().map(|i| l[i].clone()).collect())
                }
                Value::Str(s) => {
                    let ch: Vec<char> = s.chars().collect();
                    let ix = slice_indices(ch.len(), *a, *b, *c).map_err(|m| Ctrl::Throw(Value::str(&m)))?;
                    Value::str(&ix.into_iter().map(|i| ch[i]).collect::<String>())
                }
                Value::Range(x, y) => {
                    let all: Vec<i64> = (*x..*y).collect();
                    let ix = slice_indices(all.len(), *a, *b, *c).map_err(|m| Ctrl::Throw(Value::str(&m)))?;
                    Value::list(ix.into_iter().map(|i| Value::Int(all[i])).collect())
                }
                other => return throw(format!("can't slice {}", other.type_name())),
            },
        };
        if rest.is_empty() {
            return Ok(v);
        }
        // `grid[i, j]` on nested lists is `grid[i][j]`; a slice first applies the rest to each item.
        match (first, &v) {
            (Index::Slice(..), Value::List(l)) => {
                let items = l.borrow().clone();
                let mut out = vec![];
                for x in items {
                    out.push(self.subscript(x, rest)?);
                }
                Ok(Value::list(out))
            }
            _ => self.subscript(v, rest),
        }
    }

    pub fn binary(&mut self, op: &str, l: Value, r: Value, line: usize) -> Flow<Value> {
        let at = if line > 0 { format!("line {line}: ") } else { String::new() };
        // Host objects (nd arrays, 3D vectors…) can define operators.
        if let Value::Object(o) = &l {
            let o = o.clone();
            if let Some(res) = o.binary(self, op, &r, false) {
                return res.map_err(|m| Ctrl::Throw(Value::str(&format!("{at}{m}"))));
            }
        }
        if let Value::Object(o) = &r {
            let o = o.clone();
            if let Some(res) = o.binary(self, op, &l, true) {
                return res.map_err(|m| Ctrl::Throw(Value::str(&format!("{at}{m}"))));
            }
        }
        Ok(match op {
            "==" => Value::Bool(l.equals(&r)),
            "!=" => Value::Bool(!l.equals(&r)),
            "in" => Value::Bool(match &r {
                Value::List(xs) => xs.borrow().iter().any(|x| x.equals(&l)),
                Value::Map(m) => m.borrow().contains_key(&l.to_string()),
                Value::Str(s) => s.contains(&*l.to_string()),
                Value::Range(a, b) => l.as_i64().is_some_and(|x| x >= *a && x < *b),
                _ => return throw(format!("{at}can't look inside {}", r.type_name())),
            }),
            "<" | ">" | "<=" | ">=" => {
                let ord = match (&l, &r) {
                    (Value::Str(a), Value::Str(b)) => a.cmp(b),
                    _ => match (l.as_f64(), r.as_f64()) {
                        (Some(a), Some(b)) => a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal),
                        _ => return throw(format!("{at}can't compare {} with {}", l.type_name(), r.type_name())),
                    },
                };
                Value::Bool(match op {
                    "<" => ord.is_lt(),
                    ">" => ord.is_gt(),
                    "<=" => ord.is_le(),
                    _ => ord.is_ge(),
                })
            }
            "+" => match (&l, &r) {
                (Value::Int(a), Value::Int(b)) => Value::Int(a.checked_add(*b).ok_or(Ctrl::Throw(Value::str("number too large")))?),
                (Value::Str(_), _) | (_, Value::Str(_)) => {
                    let s = format!("{l}{r}");
                    self.check_len(s.len()).map_err(|m| Ctrl::Throw(Value::str(&m)))?;
                    Value::str(&s)
                }
                (Value::List(a), Value::List(b)) => {
                    let mut v = a.borrow().clone();
                    v.extend(b.borrow().iter().cloned());
                    self.check_len(v.len()).map_err(|m| Ctrl::Throw(Value::str(&m)))?;
                    Value::list(v)
                }
                (Value::Struct(_), Value::Struct(_)) => self.vec_op(&l, &r, |a, b| a + b)?,
                _ => self.num(&at, op, &l, &r)?,
            },
            "-" => match (&l, &r) {
                (Value::Int(a), Value::Int(b)) => Value::Int(a.checked_sub(*b).ok_or(Ctrl::Throw(Value::str("number too large")))?),
                (Value::Struct(_), Value::Struct(_)) => self.vec_op(&l, &r, |a, b| a - b)?,
                _ => self.num(&at, op, &l, &r)?,
            },
            "*" => match (&l, &r) {
                (Value::Int(a), Value::Int(b)) => Value::Int(a.checked_mul(*b).ok_or(Ctrl::Throw(Value::str("number too large")))?),
                (Value::Str(s), Value::Int(n)) | (Value::Int(n), Value::Str(s)) => {
                    let n = (*n).max(0) as usize;
                    self.check_len(s.len().saturating_mul(n)).map_err(|m| Ctrl::Throw(Value::str(&m)))?;
                    Value::str(&s.repeat(n))
                }
                (Value::Struct(v), k) | (k, Value::Struct(v)) if k.as_f64().is_some() => {
                    let k = k.as_f64().unwrap();
                    let v = Value::Struct(v.clone());
                    self.vec_scale(&v, k)?
                }
                _ => self.num(&at, op, &l, &r)?,
            },
            "/" => match (&l, &r) {
                (Value::Int(_), Value::Int(0)) => return throw(format!("{at}division by zero")),
                (Value::Int(a), Value::Int(b)) => Value::Int(a.div_euclid(*b)),
                _ => self.num(&at, op, &l, &r)?,
            },
            "%" => match (&l, &r) {
                (Value::Int(_), Value::Int(0)) => return throw(format!("{at}division by zero")),
                (Value::Int(a), Value::Int(b)) => Value::Int(a.rem_euclid(*b)),
                _ => self.num(&at, op, &l, &r)?,
            },
            "**" => match (&l, &r) {
                (Value::Int(a), Value::Int(b)) if *b >= 0 => Value::Int(a.checked_pow((*b).min(u32::MAX as i64) as u32).ok_or(Ctrl::Throw(Value::str("number too large")))?),
                _ => self.num(&at, op, &l, &r)?,
            },
            _ => return throw(format!("{at}unknown operator `{op}`")),
        })
    }

    fn num(&self, at: &str, op: &str, l: &Value, r: &Value) -> Flow<Value> {
        let (Some(a), Some(b)) = (l.as_f64(), r.as_f64()) else {
            return throw(format!("{at}can't use `{op}` on {} and {}", l.type_name(), r.type_name()));
        };
        Ok(Value::Float(match op {
            "+" => a + b,
            "-" => a - b,
            "*" => a * b,
            "/" => a / b,
            "%" => a.rem_euclid(b),
            _ => a.powf(b),
        }))
    }

    /// Vec2-style structs (all-number fields) add/subtract field by field.
    fn vec_op(&self, l: &Value, r: &Value, f: fn(f64, f64) -> f64) -> Flow<Value> {
        let (Value::Struct(a), Value::Struct(b)) = (l, r) else { unreachable!() };
        let (a, b) = (a.borrow(), b.borrow());
        if a.name != b.name {
            return throw(format!("can't combine {} with {}", a.name, b.name));
        }
        let mut fields = vec![];
        for ((k, x), (_, y)) in a.fields.iter().zip(b.fields.iter()) {
            let (Some(x), Some(y)) = (x.as_f64(), y.as_f64()) else { return throw(format!("{} isn't all numbers", a.name)) };
            fields.push((k.clone(), Value::Float(f(x, y))));
        }
        Ok(Value::Struct(Rc::new(RefCell::new(StructVal { name: a.name.clone(), fields }))))
    }

    fn vec_scale(&self, v: &Value, k: f64) -> Flow<Value> {
        let Value::Struct(a) = v else { unreachable!() };
        let a = a.borrow();
        let mut fields = vec![];
        for (n, x) in &a.fields {
            let Some(x) = x.as_f64() else { return throw(format!("{} isn't all numbers", a.name)) };
            fields.push((n.clone(), Value::Float(x * k)));
        }
        Ok(Value::Struct(Rc::new(RefCell::new(StructVal { name: a.name.clone(), fields }))))
    }
}

/// `*` matches any run of characters.
pub fn glob(p: &str, t: &str) -> bool {
    let (p, t) = (p.as_bytes(), t.as_bytes());
    let (mut pi, mut ti, mut star, mut mark) = (0, 0, None, 0);
    while ti < t.len() {
        if pi < p.len() && p[pi] == b'*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if pi < p.len() && p[pi] == t[ti] {
            pi += 1;
            ti += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == b'*' {
        pi += 1;
    }
    pi == p.len()
}

/// The value of an expression made only of literals (`5`, `"a"`, `[1, 2]`,
/// `{ a: -1 }`), or `None`.
fn literal(e: &Expr) -> Option<Value> {
    Some(match e {
        Expr::Null => Value::Null,
        Expr::Bool(b) => Value::Bool(*b),
        Expr::Int(i) => Value::Int(*i),
        Expr::Float(f) => Value::Float(*f),
        Expr::Color(c) => Value::Color(*c),
        Expr::Str(segs) => match segs.as_slice() {
            [] => Value::str(""),
            [StrSeg::Lit(s)] => Value::str(s),
            _ => return None,
        },
        Expr::Unary("-", x) => match literal(x)? {
            Value::Int(i) => Value::Int(i.checked_neg()?),
            Value::Float(f) => Value::Float(-f),
            _ => return None,
        },
        Expr::List(xs) => Value::list(xs.iter().map(literal).collect::<Option<_>>()?),
        Expr::Map(kvs) => {
            let mut m = BTreeMap::new();
            for (k, v) in kvs {
                let key = match k {
                    Expr::Var(n, _) => n.clone(),
                    k => literal(k)?.to_string(),
                };
                m.insert(key, literal(v)?);
            }
            Value::map(m)
        }
        _ => return None,
    })
}
