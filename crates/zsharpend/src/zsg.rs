//! `.zsg`: GPU code — compute **kernels** (general-purpose, not only
//! rendering) and **fragment** shaders, with typed helper `func`s and
//! `const`s. A module is type-checked, compiled to WGSL (validated with
//! naga, the shader compiler wgpu uses) and can also run on the CPU
//! reference executor, which is what runs here without a GPU and what the
//! tests compare against.
//!
//! ```text
//! kernel scale(buf data: [float], uniform k: float) {
//!     data[gid] = data[gid] * k
//! }
//! fragment glow(uv: vec2, uniform time: float) -> color {
//!     let d = length(uv - vec2(0.5, 0.5))
//!     return vec4(0.2, 0.5, 1.0, 1.0) * (1.0 - d)
//! }
//! ```
//!
//! Types: `float`, `int`, `bool`, `vec2`, `vec3`, `vec4`/`color`, and
//! `[float]` / `[int]` buffers (kernel parameters marked `buf`). Kernels
//! run once per element of their first buffer, with its index in `gid`.
//! Fragments get `uv` (0..1 across the area) and `resolution` (pixels).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::ast::*;
use crate::value::{HostObject, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ty {
    F32,
    I32,
    Bool,
    Vec(u8),
    /// A buffer of f32 or i32.
    Buf(bool),
    Void,
}

impl Ty {
    fn parse(t: &Type) -> Result<Ty, String> {
        if t.array {
            return match t.name.as_str() {
                "float" | "f32" => Ok(Ty::Buf(false)),
                "int" | "i32" => Ok(Ty::Buf(true)),
                other => Err(format!("buffers hold float or int, not {other}")),
            };
        }
        Ok(match t.name.as_str() {
            "float" | "f32" => Ty::F32,
            "int" | "i32" => Ty::I32,
            "bool" => Ty::Bool,
            "vec2" => Ty::Vec(2),
            "vec3" => Ty::Vec(3),
            "vec4" | "color" => Ty::Vec(4),
            other => return Err(format!("`{other}` isn't a GPU type (float, int, bool, vec2, vec3, vec4, color, [float], [int])")),
        })
    }
    fn wgsl(self) -> String {
        match self {
            Ty::F32 => "f32".into(),
            Ty::I32 => "i32".into(),
            Ty::Bool => "bool".into(),
            Ty::Vec(n) => format!("vec{n}<f32>"),
            Ty::Buf(i) => format!("array<{}>", if i { "i32" } else { "f32" }),
            Ty::Void => String::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Kernel,
    Fragment,
}

#[derive(Debug)]
pub struct Entry {
    pub kind: EntryKind,
    pub def: Rc<FuncDef>,
    /// (name, type, is a `buf`)
    pub params: Vec<(String, Ty, bool)>,
}

/// A helper `func`: definition, typed parameters, return type.
pub type Helper = (Rc<FuncDef>, Vec<(String, Ty)>, Ty);

#[derive(Debug, Default)]
pub struct Module {
    pub entries: Vec<Entry>,
    pub helpers: Vec<Helper>,
    pub consts: Vec<(String, Expr, Ty)>,
}

/// Type environment while checking/compiling one function.
#[derive(Clone)]
struct Scope {
    vars: HashMap<String, Ty>,
    /// Names that are uniforms (emitted as `u.name`).
    uniforms: HashMap<String, String>,
}

fn fl(x: f64) -> String {
    let s = format!("{}", x as f32);
    if s.contains('.') || s.contains('e') || s.contains("inf") || s.contains("NaN") {
        s
    } else {
        format!("{s}.0")
    }
}

const SWIZ: &str = "xyzwrgba";

impl Module {
    pub fn parse(src: &str) -> Result<Module, String> {
        let items = crate::parser::parse(src, Kind::Gpu)?;
        Module::from_items(&items)
    }

    pub fn from_items(items: &[Item]) -> Result<Module, String> {
        let mut m = Module::default();
        for it in items {
            match it {
                Item::Kernel(f) | Item::Fragment(f) => {
                    let kind = if matches!(it, Item::Kernel(_)) { EntryKind::Kernel } else { EntryKind::Fragment };
                    let mut params = vec![];
                    for p in &f.params {
                        let ty = Ty::parse(p.ty.as_ref().ok_or(format!("{}: parameter `{}` needs a type", f.name, p.name))?).map_err(|e| format!("{}: {e}", f.name))?;
                        let is_buf = p.kind == "buf";
                        if is_buf != matches!(ty, Ty::Buf(_)) {
                            return Err(format!("{}: `{}` — mark buffers `buf name: [float]`, other parameters `uniform`", f.name, p.name));
                        }
                        params.push((p.name.clone(), ty, is_buf));
                    }
                    match kind {
                        EntryKind::Kernel => {
                            if !params.iter().any(|p| p.2) {
                                return Err(format!("kernel {}: needs at least one `buf` parameter (it runs once per element of the first)", f.name));
                            }
                        }
                        EntryKind::Fragment => {
                            if params.first().map(|p| (p.0.as_str(), p.1)) != Some(("uv", Ty::Vec(2))) {
                                return Err(format!("fragment {}: the first parameter is `uv: vec2`", f.name));
                            }
                            if f.ret.as_ref().map(|t| t.name.as_str()) != Some("color") && f.ret.as_ref().map(|t| t.name.as_str()) != Some("vec4") {
                                return Err(format!("fragment {}: returns `color` (or vec4)", f.name));
                            }
                        }
                    }
                    m.entries.push(Entry { kind, def: f.clone(), params });
                }
                Item::Func(f) => {
                    let mut ps = vec![];
                    for p in &f.params {
                        let t = p.ty.as_ref().ok_or(format!("{}: in .zsg every parameter needs a type (`{}`)", f.name, p.name))?;
                        ps.push((p.name.clone(), Ty::parse(t).map_err(|e| format!("{}: {e}", f.name))?));
                    }
                    let ret = match &f.ret {
                        Some(t) => Ty::parse(t).map_err(|e| format!("{}: {e}", f.name))?,
                        None => Ty::Void,
                    };
                    m.helpers.push((f.clone(), ps, ret));
                }
                Item::Stmt(Stmt::Let { name, value: Some(v), constant: true, .. }) => {
                    let ty = m.infer(v, &Scope { vars: HashMap::new(), uniforms: HashMap::new() })?;
                    m.consts.push((name.clone(), v.clone(), ty));
                }
                Item::Struct { name, .. } => return Err(format!("struct {name}: structs aren't supported in .zsg yet")),
                _ => {}
            }
        }
        // Type-check every body now, so errors show at load time.
        m.to_wgsl()?;
        Ok(m)
    }

    pub fn entry_names(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.def.name.clone()).collect()
    }

    pub fn entry(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.def.name == name)
    }

    // ── Types ──────────────────────────────────────────────────────────

    fn helper(&self, name: &str) -> Option<&Helper> {
        self.helpers.iter().find(|h| h.0.name == name)
    }

    fn builtin_ty(&self, name: &str, args: &[Ty]) -> Option<Ty> {
        let first = args.first().copied().unwrap_or(Ty::F32);
        let fl = |t: Ty| if t == Ty::I32 { Ty::F32 } else { t };
        Some(match name {
            "sin" | "cos" | "tan" | "sqrt" | "abs" | "floor" | "ceil" | "fract" | "exp" | "log" | "normalize" | "sign" | "round" => fl(first),
            "min" | "max" | "clamp" | "pow" | "step" | "smoothstep" | "atan2" => {
                if args.iter().any(|t| matches!(t, Ty::Vec(_))) {
                    *args.iter().find(|t| matches!(t, Ty::Vec(_))).unwrap()
                } else if args.iter().all(|t| *t == Ty::I32) && matches!(name, "min" | "max" | "clamp") {
                    Ty::I32
                } else {
                    Ty::F32
                }
            }
            "mix" => {
                if matches!(args.first(), Some(Ty::Vec(_))) {
                    args[0]
                } else {
                    Ty::F32
                }
            }
            "length" | "distance" | "dot" => Ty::F32,
            "vec2" => Ty::Vec(2),
            "vec3" => Ty::Vec(3),
            "vec4" | "rgba" => Ty::Vec(4),
            "float" => Ty::F32,
            "int" => Ty::I32,
            "len" => Ty::I32,
            _ => return None,
        })
    }

    fn infer(&self, e: &Expr, sc: &Scope) -> Result<Ty, String> {
        Ok(match e {
            Expr::Int(_) => Ty::I32,
            Expr::Float(_) => Ty::F32,
            Expr::Bool(_) => Ty::Bool,
            Expr::Color(_) => Ty::Vec(4),
            Expr::Var(n, line) => match sc.vars.get(n) {
                Some(t) => *t,
                None => match self.consts.iter().find(|c| &c.0 == n) {
                    Some(c) => c.2,
                    None => match n.as_str() {
                        "PI" | "TAU" => Ty::F32,
                        _ => return Err(format!("line {line}: `{n}` isn't defined here")),
                    },
                },
            },
            Expr::Unary("!", _) => Ty::Bool,
            Expr::Unary(_, x) => self.infer(x, sc)?,
            Expr::Binary(op, a, b) => {
                let (ta, tb) = (self.infer(a, sc)?, self.infer(b, sc)?);
                match *op {
                    "==" | "!=" | "<" | ">" | "<=" | ">=" | "&&" | "||" => Ty::Bool,
                    _ => match (ta, tb) {
                        (Ty::Vec(n), _) | (_, Ty::Vec(n)) => Ty::Vec(n),
                        (Ty::I32, Ty::I32) => Ty::I32,
                        _ => Ty::F32,
                    },
                }
            }
            Expr::Ternary(_, a, b) => {
                let (ta, tb) = (self.infer(a, sc)?, self.infer(b, sc)?);
                if ta == Ty::I32 && tb == Ty::I32 {
                    Ty::I32
                } else if ta == Ty::I32 || ta == Ty::F32 {
                    tb.max_num(ta)
                } else {
                    ta
                }
            }
            Expr::Index(b, _) => match self.infer(b, sc)? {
                Ty::Buf(true) => Ty::I32,
                Ty::Buf(false) => Ty::F32,
                Ty::Vec(_) => Ty::F32,
                t => return Err(format!("can't index a {t:?}")),
            },
            Expr::Field(o, f, _) => match self.infer(o, sc)? {
                Ty::Vec(n) => {
                    if f.is_empty() || f.len() > 4 || !f.chars().all(|c| SWIZ.contains(c)) {
                        return Err(format!("`.{f}`: use x/y/z/w or r/g/b/a"));
                    }
                    for c in f.chars() {
                        let i = SWIZ.find(c).unwrap() % 4;
                        if i as u8 >= n {
                            return Err(format!("`.{f}`: a vec{n} has no `{c}`"));
                        }
                    }
                    if f.len() == 1 {
                        Ty::F32
                    } else {
                        Ty::Vec(f.len() as u8)
                    }
                }
                t => return Err(format!("`.{f}` on {t:?}")),
            },
            Expr::Call(f, args, line) => {
                let Expr::Var(name, _) = &**f else { return Err(format!("line {line}: only functions can be called in .zsg")) };
                let ts: Vec<Ty> = args.iter().map(|a| self.infer(a, sc)).collect::<Result<_, _>>()?;
                if let Some(h) = self.helper(name) {
                    if h.1.len() != args.len() {
                        return Err(format!("line {line}: `{name}` takes {} arguments", h.1.len()));
                    }
                    h.2
                } else {
                    self.builtin_ty(name, &ts).ok_or(format!("line {line}: `{name}` isn't a GPU function"))?
                }
            }
            other => return Err(format!("{other:?} isn't allowed in .zsg code")),
        })
    }

    // ── WGSL ───────────────────────────────────────────────────────────

    /// Emit `e`, converting to `want` where WGSL needs it.
    fn emit_as(&self, e: &Expr, want: Ty, sc: &Scope) -> Result<String, String> {
        let have = self.infer(e, sc)?;
        let s = self.emit(e, sc, want)?;
        Ok(match (have, want) {
            (Ty::I32, Ty::F32) => match e {
                Expr::Int(i) => fl(*i as f64),
                _ => format!("f32({s})"),
            },
            (Ty::F32, Ty::I32) => format!("i32({s})"),
            (Ty::I32 | Ty::F32, Ty::Vec(n)) => format!("vec{n}<f32>({})", if have == Ty::I32 { format!("f32({s})") } else { s }),
            _ => s,
        })
    }

    fn emit(&self, e: &Expr, sc: &Scope, hint: Ty) -> Result<String, String> {
        Ok(match e {
            Expr::Int(i) => {
                if hint == Ty::F32 {
                    fl(*i as f64)
                } else {
                    format!("{i}")
                }
            }
            Expr::Float(f) => fl(*f),
            Expr::Bool(b) => b.to_string(),
            Expr::Color(c) => {
                let (r, g, b, a) = crate::value::color_parts(*c);
                format!("vec4<f32>({}, {}, {}, {})", fl(r as f64 / 255.0), fl(g as f64 / 255.0), fl(b as f64 / 255.0), fl(a as f64 / 255.0))
            }
            Expr::Var(n, _) => match sc.uniforms.get(n) {
                Some(path) => path.clone(),
                None => match n.as_str() {
                    "PI" => fl(std::f64::consts::PI),
                    "TAU" => fl(std::f64::consts::TAU),
                    _ => n.clone(),
                },
            },
            Expr::Unary(op, x) => format!("({op}{})", self.emit(x, sc, hint)?),
            Expr::Binary(op, a, b) => {
                let (ta, tb) = (self.infer(a, sc)?, self.infer(b, sc)?);
                let num = |t: Ty| matches!(t, Ty::F32 | Ty::I32);
                let target = match *op {
                    "&&" | "||" => Ty::Bool,
                    _ => match (ta, tb) {
                        (Ty::I32, Ty::I32) => Ty::I32,
                        (Ty::Vec(n), Ty::Vec(_)) => Ty::Vec(n),
                        (Ty::Vec(_), t) | (t, Ty::Vec(_)) if num(t) => Ty::F32,
                        _ => Ty::F32,
                    },
                };
                let side = |x: &Expr, t: Ty| -> Result<String, String> {
                    if num(t) && target != Ty::Bool {
                        self.emit_as(x, if matches!(target, Ty::Vec(_)) { Ty::F32 } else { target }, sc)
                    } else {
                        self.emit(x, sc, target)
                    }
                };
                let (l, r) = (side(a, ta)?, side(b, tb)?);
                if *op == "**" {
                    format!("pow({l}, {r})")
                } else {
                    format!("({l} {op} {r})")
                }
            }
            Expr::Ternary(c, a, b) => {
                let t = self.infer(e, sc)?;
                format!("select({}, {}, {})", self.emit_as(b, t, sc)?, self.emit_as(a, t, sc)?, self.emit(c, sc, Ty::Bool)?)
            }
            Expr::Index(b, i) => {
                let bt = self.infer(b, sc)?;
                format!("{}[{}]", self.emit(b, sc, bt)?, self.emit_as(i, Ty::I32, sc)?)
            }
            Expr::Field(o, f, _) => {
                let s: String = f.chars().map(|c| SWIZ.as_bytes()[SWIZ.find(c).unwrap() % 4] as char).collect();
                format!("{}.{}", self.emit(o, sc, Ty::Vec(4))?, s)
            }
            Expr::Call(f, args, _) => {
                let Expr::Var(name, _) = &**f else { unreachable!() };
                if let Some(h) = self.helper(name) {
                    let a: Vec<String> = args.iter().zip(h.1.iter()).map(|(x, (_, t))| self.emit_as(x, *t, sc)).collect::<Result<_, _>>()?;
                    return Ok(format!("{name}({})", a.join(", ")));
                }
                let ts: Vec<Ty> = args.iter().map(|a| self.infer(a, sc)).collect::<Result<_, _>>()?;
                let rt = self.builtin_ty(name, &ts).unwrap();
                let arg_f = |x: &Expr| self.emit_as(x, Ty::F32, sc);
                let elem = if let Ty::Vec(_) = rt { rt } else { Ty::F32 };
                match name.as_str() {
                    "vec2" | "vec3" | "vec4" | "rgba" => {
                        let n = if name == "rgba" { 4 } else { name[3..].parse::<u8>().unwrap() };
                        let a: Vec<String> = args.iter().map(|x| match self.infer(x, sc) {
                            Ok(Ty::Vec(_)) => self.emit(x, sc, Ty::F32),
                            _ => arg_f(x),
                        }).collect::<Result<_, _>>()?;
                        format!("vec{n}<f32>({})", a.join(", "))
                    }
                    "float" => format!("f32({})", self.emit(&args[0], sc, Ty::F32)?),
                    "int" => format!("i32({})", self.emit(&args[0], sc, Ty::I32)?),
                    "len" => format!("i32(arrayLength(&{}))", self.emit(&args[0], sc, Ty::Void)?),
                    "min" | "max" | "clamp" if rt == Ty::I32 => {
                        let a: Vec<String> = args.iter().map(|x| self.emit_as(x, Ty::I32, sc)).collect::<Result<_, _>>()?;
                        format!("{name}({})", a.join(", "))
                    }
                    "mix" | "clamp" | "min" | "max" | "step" | "smoothstep" | "pow" => {
                        let a: Vec<String> = args
                            .iter()
                            .enumerate()
                            .map(|(i, x)| {
                                let t = self.infer(x, sc)?;
                                // mix's third argument may stay scalar.
                                let want = if name == "mix" && i == 2 { Ty::F32 } else if matches!(elem, Ty::Vec(_)) && !matches!(t, Ty::Vec(_)) { elem } else { Ty::F32 };
                                if matches!(t, Ty::Vec(_)) {
                                    self.emit(x, sc, t)
                                } else {
                                    self.emit_as(x, want, sc)
                                }
                            })
                            .collect::<Result<_, _>>()?;
                        format!("{name}({})", a.join(", "))
                    }
                    "atan2" => format!("atan2({}, {})", arg_f(&args[0])?, arg_f(&args[1])?),
                    _ => {
                        let a: Vec<String> = args
                            .iter()
                            .map(|x| match self.infer(x, sc)? {
                                Ty::Vec(_) => self.emit(x, sc, Ty::F32),
                                _ => arg_f(x),
                            })
                            .collect::<Result<_, _>>()?;
                        format!("{name}({})", a.join(", "))
                    }
                }
            }
            other => return Err(format!("{other:?} isn't allowed in .zsg code")),
        })
    }

    fn emit_block(&self, b: &[Stmt], sc: &mut Scope, ret: Ty, ind: usize, out: &mut String) -> Result<(), String> {
        let pad = "    ".repeat(ind);
        for s in b {
            match s {
                Stmt::Let { name, ty, value, line, .. } => {
                    let t = match (ty, value) {
                        (Some(t), _) => Ty::parse(t).map_err(|e| format!("line {line}: {e}"))?,
                        (None, Some(v)) => self.infer(v, sc)?,
                        (None, None) => return Err(format!("line {line}: `{name}` needs a type or a value")),
                    };
                    let init = match value {
                        Some(v) => self.emit_as(v, t, sc)?,
                        None => match t {
                            Ty::I32 => "0".into(),
                            Ty::Bool => "false".into(),
                            _ => format!("{}()", t.wgsl()),
                        },
                    };
                    out.push_str(&format!("{pad}var {name}: {} = {init};\n", t.wgsl()));
                    sc.vars.insert(name.clone(), t);
                }
                Stmt::Assign { target, op, value, line } => {
                    let tt = self.infer(target, sc).map_err(|e| format!("line {line}: {e}"))?;
                    let lhs = self.emit(target, sc, tt)?;
                    let rhs = self.emit_as(value, if matches!(tt, Ty::Vec(_)) && !matches!(self.infer(value, sc)?, Ty::Vec(_)) { Ty::F32 } else { tt }, sc)?;
                    out.push_str(&format!("{pad}{lhs} {op} {rhs};\n"));
                }
                Stmt::Expr(e, _) => out.push_str(&format!("{pad}_ = {};\n", self.emit(e, sc, Ty::Void)?)),
                Stmt::If { cond, then, els, .. } => {
                    out.push_str(&format!("{pad}if ({}) {{\n", self.emit(cond, sc, Ty::Bool)?));
                    self.emit_block(then, &mut sc.clone(), ret, ind + 1, out)?;
                    if let Some(e) = els {
                        out.push_str(&format!("{pad}}} else {{\n"));
                        self.emit_block(e, &mut sc.clone(), ret, ind + 1, out)?;
                    }
                    out.push_str(&format!("{pad}}}\n"));
                }
                Stmt::While { cond, body, .. } => {
                    out.push_str(&format!("{pad}while ({}) {{\n", self.emit(cond, sc, Ty::Bool)?));
                    self.emit_block(body, &mut sc.clone(), ret, ind + 1, out)?;
                    out.push_str(&format!("{pad}}}\n"));
                }
                Stmt::For { var, iter: Expr::Range(a, b, incl), body, .. } => {
                    let mut inner = sc.clone();
                    inner.vars.insert(var.clone(), Ty::I32);
                    out.push_str(&format!(
                        "{pad}for (var {var}: i32 = {}; {var} {} {}; {var} = {var} + 1) {{\n",
                        self.emit_as(a, Ty::I32, sc)?,
                        if *incl { "<=" } else { "<" },
                        self.emit_as(b, Ty::I32, sc)?
                    ));
                    self.emit_block(body, &mut inner, ret, ind + 1, out)?;
                    out.push_str(&format!("{pad}}}\n"));
                }
                Stmt::For { line, .. } => return Err(format!("line {line}: in .zsg, `for` loops over a range (`for i in 0..n`)")),
                Stmt::Return(v, line) => match v {
                    Some(v) => {
                        if ret == Ty::Void {
                            return Err(format!("line {line}: this function returns nothing"));
                        }
                        out.push_str(&format!("{pad}return {};\n", self.emit_as(v, ret, sc)?));
                    }
                    None => out.push_str(&format!("{pad}return;\n")),
                },
                Stmt::Break(_) => out.push_str(&format!("{pad}break;\n")),
                Stmt::Continue(_) => out.push_str(&format!("{pad}continue;\n")),
                Stmt::Block(b) => {
                    out.push_str(&format!("{pad}{{\n"));
                    self.emit_block(b, &mut sc.clone(), ret, ind + 1, out)?;
                    out.push_str(&format!("{pad}}}\n"));
                }
                Stmt::Throw(_, l) | Stmt::Try { line: l, .. } => return Err(format!("line {l}: throw/try aren't available on the GPU")),
                Stmt::Destructure { line, .. } | Stmt::Print { line, .. } | Stmt::Bare(_, line) | Stmt::Switch { line, .. } => {
                    return Err(format!("line {line}: that statement isn't available on the GPU"))
                }
            }
        }
        Ok(())
    }

    /// The whole module as WGSL. Each kernel/fragment gets its own bind group
    /// (group = its position), so entry points never share bindings.
    pub fn to_wgsl(&self) -> Result<String, String> {
        let mut out = String::from("// Generated by Z#++ from a .zsg module.\n\n");
        for (name, v, t) in &self.consts {
            let sc = Scope { vars: HashMap::new(), uniforms: HashMap::new() };
            out.push_str(&format!("const {name}: {} = {};\n", t.wgsl(), self.emit_as(v, *t, &sc)?));
        }
        let consts: HashMap<String, Ty> = self.consts.iter().map(|c| (c.0.clone(), c.2)).collect();
        for (f, ps, ret) in &self.helpers {
            let mut sc = Scope { vars: consts.clone(), uniforms: HashMap::new() };
            for (n, t) in ps {
                sc.vars.insert(n.clone(), *t);
            }
            let params: Vec<String> = ps.iter().map(|(n, t)| format!("{n}_in: {}", t.wgsl())).collect();
            let ret_s = if *ret == Ty::Void { String::new() } else { format!(" -> {}", ret.wgsl()) };
            out.push_str(&format!("\nfn {}({}){ret_s} {{\n", f.name, params.join(", ")));
            for (n, t) in ps {
                out.push_str(&format!("    var {n}: {} = {n}_in;\n", t.wgsl()));
            }
            self.emit_block(f.body.as_ref().unwrap(), &mut sc, *ret, 1, &mut out).map_err(|e| format!("{}: {e}", f.name))?;
            out.push_str("}\n");
        }
        for (g, e) in self.entries.iter().enumerate() {
            let name = &e.def.name;
            let mut sc = Scope { vars: consts.clone(), uniforms: HashMap::new() };
            let uniforms: Vec<&(String, Ty, bool)> = e.params.iter().filter(|p| !p.2 && !(e.kind == EntryKind::Fragment && p.0 == "uv")).collect();
            let ustruct = format!("U_{name}");
            let uvar = format!("u_{name}");
            let mut fields: Vec<String> = uniforms.iter().map(|p| format!("    {}: {},", p.0, p.1.wgsl())).collect();
            if e.kind == EntryKind::Fragment {
                fields.insert(0, "    resolution: vec2<f32>,".into());
                sc.vars.insert("resolution".into(), Ty::Vec(2));
                sc.uniforms.insert("resolution".into(), format!("{uvar}.resolution"));
            }
            if !fields.is_empty() {
                out.push_str(&format!("\nstruct {ustruct} {{\n{}\n}}\n@group({g}) @binding(0) var<uniform> {uvar}: {ustruct};\n", fields.join("\n")));
            }
            for p in &uniforms {
                sc.vars.insert(p.0.clone(), p.1);
                sc.uniforms.insert(p.0.clone(), format!("{uvar}.{}", p.0));
            }
            match e.kind {
                EntryKind::Kernel => {
                    let bufs: Vec<&(String, Ty, bool)> = e.params.iter().filter(|p| p.2).collect();
                    for (i, b) in bufs.iter().enumerate() {
                        out.push_str(&format!("@group({g}) @binding({}) var<storage, read_write> {}: {};\n", i + 1, b.0, b.1.wgsl()));
                        sc.vars.insert(b.0.clone(), b.1);
                    }
                    sc.vars.insert("gid".into(), Ty::I32);
                    out.push_str(&format!(
                        "\n@compute @workgroup_size(64)\nfn {name}(@builtin(global_invocation_id) gid3: vec3<u32>) {{\n    let gid = i32(gid3.x);\n    if (gid3.x >= arrayLength(&{})) {{ return; }}\n",
                        bufs[0].0
                    ));
                    self.emit_block(e.def.body.as_ref().unwrap(), &mut sc, Ty::Void, 1, &mut out).map_err(|x| format!("kernel {name}: {x}"))?;
                    out.push_str("}\n");
                }
                EntryKind::Fragment => {
                    sc.vars.insert("uv".into(), Ty::Vec(2));
                    out.push_str(&format!(
                        "\n@fragment\nfn {name}(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {{\n    var uv: vec2<f32> = pos.xy / {uvar}.resolution;\n"
                    ));
                    self.emit_block(e.def.body.as_ref().unwrap(), &mut sc, Ty::Vec(4), 1, &mut out).map_err(|x| format!("fragment {name}: {x}"))?;
                    out.push_str("}\n");
                }
            }
        }
        Ok(out)
    }

    /// Compile to WGSL and validate it with naga.
    pub fn validate(&self) -> Result<String, String> {
        let wgsl = self.to_wgsl()?;
        let module = naga::front::wgsl::parse_str(&wgsl).map_err(|e| format!("generated WGSL doesn't parse: {}\n{wgsl}", e.emit_to_string(&wgsl)))?;
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty())
            .validate(&module)
            .map_err(|e| format!("generated WGSL doesn't validate: {e:?}\n{wgsl}"))?;
        Ok(wgsl)
    }

    // ── CPU reference executor ─────────────────────────────────────────

    /// Run `kernel` once per element of its first buffer. `buffers` holds
    /// each `buf` parameter's data by name; `uniforms` the rest.
    pub fn run_kernel(&self, kernel: &str, buffers: &mut HashMap<String, Vec<f32>>, uniforms: &HashMap<String, f32>) -> Result<(), String> {
        let e = self.entry(kernel).filter(|e| e.kind == EntryKind::Kernel).ok_or(format!("no kernel `{kernel}`"))?;
        let first = &e.params.iter().find(|p| p.2).unwrap().0;
        let n = buffers.get(first).ok_or(format!("kernel {kernel}: buffer `{first}` not given"))?.len();
        for p in e.params.iter().filter(|p| p.2) {
            if !buffers.contains_key(&p.0) {
                return Err(format!("kernel {kernel}: buffer `{}` not given", p.0));
            }
        }
        let bufs = RefCell::new(std::mem::take(buffers));
        let mut steps = 0u64;
        let r = (|| {
            for gid in 0..n {
                let mut env = Cpu { m: self, vars: HashMap::new(), bufs: &bufs, steps: &mut steps };
                env.vars.insert("gid".into(), G::I(gid as i32));
                for p in e.params.iter().filter(|p| !p.2) {
                    let v = uniforms.get(&p.0).copied().unwrap_or(0.0);
                    env.vars.insert(p.0.clone(), if p.1 == Ty::I32 { G::I(v as i32) } else { G::F(v) });
                }
                match env.block(e.def.body.as_ref().unwrap()) {
                    Ok(_) => {}
                    Err(Esc::Return(_)) => {}
                    Err(Esc::Err(m)) => return Err(format!("kernel {kernel}: {m}")),
                    Err(_) => {}
                }
            }
            Ok(())
        })();
        *buffers = bufs.into_inner();
        r
    }

    /// Evaluate `fragment` at `uv` (0..1) for an area of `resolution` pixels.
    pub fn shade(&self, fragment: &str, uv: [f32; 2], resolution: [f32; 2], uniforms: &HashMap<String, f32>) -> Result<[f32; 4], String> {
        let e = self.entry(fragment).filter(|e| e.kind == EntryKind::Fragment).ok_or(format!("no fragment `{fragment}`"))?;
        let bufs = RefCell::new(HashMap::new());
        let mut steps = 0u64;
        let mut env = Cpu { m: self, vars: HashMap::new(), bufs: &bufs, steps: &mut steps };
        env.vars.insert("uv".into(), G::V(uv.to_vec()));
        env.vars.insert("resolution".into(), G::V(resolution.to_vec()));
        for p in e.params.iter().skip(1) {
            let v = uniforms.get(&p.0).copied().unwrap_or(0.0);
            env.vars.insert(p.0.clone(), if p.1 == Ty::I32 { G::I(v as i32) } else { G::F(v) });
        }
        match env.block(e.def.body.as_ref().unwrap()) {
            Err(Esc::Return(G::V(v))) if v.len() == 4 => Ok([v[0], v[1], v[2], v[3]]),
            Err(Esc::Return(other)) => Err(format!("fragment {fragment} returned {other:?}, not a color")),
            Err(Esc::Err(m)) => Err(format!("fragment {fragment}: {m}")),
            _ => Err(format!("fragment {fragment} ended without `return`")),
        }
    }
}

impl Ty {
    fn max_num(self, other: Ty) -> Ty {
        if self == Ty::F32 || other == Ty::F32 {
            Ty::F32
        } else {
            self
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum G {
    F(f32),
    I(i32),
    B(bool),
    V(Vec<f32>),
}

enum Esc {
    Return(G),
    Break,
    Continue,
    Err(String),
}

struct Cpu<'a> {
    m: &'a Module,
    vars: HashMap<String, G>,
    bufs: &'a RefCell<HashMap<String, Vec<f32>>>,
    steps: &'a mut u64,
}

fn f(g: &G) -> Result<f32, Esc> {
    match g {
        G::F(x) => Ok(*x),
        G::I(i) => Ok(*i as f32),
        G::B(b) => Ok(*b as i32 as f32),
        G::V(_) => Err(Esc::Err("expected a number, got a vector".into())),
    }
}

fn vecof(g: &G, n: usize) -> Result<Vec<f32>, Esc> {
    match g {
        G::V(v) => Ok(v.clone()),
        other => Ok(vec![f(other)?; n]),
    }
}

impl Cpu<'_> {
    fn block(&mut self, b: &[Stmt]) -> Result<(), Esc> {
        for s in b {
            self.stmt(s)?;
        }
        Ok(())
    }

    fn stmt(&mut self, s: &Stmt) -> Result<(), Esc> {
        *self.steps += 1;
        if *self.steps > 50_000_000 {
            return Err(Esc::Err("ran too long".into()));
        }
        match s {
            Stmt::Let { name, ty, value, .. } => {
                let v = match value {
                    Some(v) => self.eval(v)?,
                    None => G::F(0.0),
                };
                let v = match ty.as_ref().map(|t| t.name.as_str()) {
                    Some("float") => G::F(f(&v)?),
                    Some("int") => G::I(f(&v)? as i32),
                    _ => v,
                };
                self.vars.insert(name.clone(), v);
            }
            Stmt::Assign { target, op, value, .. } => {
                let mut v = self.eval(value)?;
                if *op != "=" {
                    let cur = self.eval(target)?;
                    v = self.bin(&op[..1], cur, v)?;
                }
                match target {
                    Expr::Var(n, _) => {
                        let v = match self.vars.get(n) {
                            Some(G::I(_)) => G::I(f(&v)? as i32),
                            Some(G::F(_)) => G::F(f(&v)?),
                            _ => v,
                        };
                        self.vars.insert(n.clone(), v);
                    }
                    Expr::Index(b, i) => {
                        let Expr::Var(bn, _) = &**b else { return Err(Esc::Err("only buffers can be indexed into".into())) };
                        let i = f(&self.eval(i)?)? as i64;
                        let mut bufs = self.bufs.borrow_mut();
                        let buf = bufs.get_mut(bn).ok_or(Esc::Err(format!("no buffer `{bn}`")))?;
                        if i < 0 || i as usize >= buf.len() {
                            return Err(Esc::Err(format!("index {i} is outside `{bn}` (length {})", buf.len())));
                        }
                        buf[i as usize] = f(&v)?;
                    }
                    _ => return Err(Esc::Err("can't assign to that on the GPU".into())),
                }
            }
            Stmt::Expr(e, _) => {
                self.eval(e)?;
            }
            Stmt::If { cond, then, els, .. } => {
                if self.truthy(cond)? {
                    self.block(then)?
                } else if let Some(e) = els {
                    self.block(e)?
                }
            }
            Stmt::While { cond, body, .. } => {
                while self.truthy(cond)? {
                    match self.block(body) {
                        Err(Esc::Break) => break,
                        Err(Esc::Continue) | Ok(()) => {}
                        Err(e) => return Err(e),
                    }
                }
            }
            Stmt::For { var, iter: Expr::Range(a, b, incl), body, .. } => {
                let a = f(&self.eval(a)?)? as i32;
                let b = f(&self.eval(b)?)? as i32 + *incl as i32;
                for i in a..b {
                    self.vars.insert(var.clone(), G::I(i));
                    match self.block(body) {
                        Err(Esc::Break) => break,
                        Err(Esc::Continue) | Ok(()) => {}
                        Err(e) => return Err(e),
                    }
                }
            }
            Stmt::Return(v, _) => return Err(Esc::Return(match v {
                Some(v) => self.eval(v)?,
                None => G::F(0.0),
            })),
            Stmt::Break(_) => return Err(Esc::Break),
            Stmt::Continue(_) => return Err(Esc::Continue),
            Stmt::Block(b) => self.block(b)?,
            _ => return Err(Esc::Err("not available on the GPU".into())),
        }
        Ok(())
    }

    fn truthy(&mut self, e: &Expr) -> Result<bool, Esc> {
        Ok(match self.eval(e)? {
            G::B(b) => b,
            g => f(&g)? != 0.0,
        })
    }

    fn bin(&mut self, op: &str, a: G, b: G) -> Result<G, Esc> {
        if matches!(op, "==" | "!=" | "<" | ">" | "<=" | ">=") {
            let (x, y) = (f(&a)?, f(&b)?);
            return Ok(G::B(match op {
                "==" => x == y,
                "!=" => x != y,
                "<" => x < y,
                ">" => x > y,
                "<=" => x <= y,
                _ => x >= y,
            }));
        }
        if let (G::I(x), G::I(y)) = (&a, &b) {
            return Ok(G::I(match op {
                "+" => x.wrapping_add(*y),
                "-" => x.wrapping_sub(*y),
                "*" => x.wrapping_mul(*y),
                "/" => {
                    if *y == 0 {
                        0
                    } else {
                        x / y
                    }
                }
                "%" => {
                    if *y == 0 {
                        0
                    } else {
                        x % y
                    }
                }
                _ => x.pow((*y).max(0) as u32),
            }));
        }
        let op1 = |x: f32, y: f32| match op {
            "+" => x + y,
            "-" => x - y,
            "*" => x * y,
            "/" => x / y,
            "%" => x - y * (x / y).trunc(),
            _ => x.powf(y),
        };
        if matches!(a, G::V(_)) || matches!(b, G::V(_)) {
            let n = match (&a, &b) {
                (G::V(v), _) | (_, G::V(v)) => v.len(),
                _ => unreachable!(),
            };
            let (x, y) = (vecof(&a, n)?, vecof(&b, n)?);
            return Ok(G::V(x.iter().zip(y.iter()).map(|(p, q)| op1(*p, *q)).collect()));
        }
        Ok(G::F(op1(f(&a)?, f(&b)?)))
    }

    fn eval(&mut self, e: &Expr) -> Result<G, Esc> {
        Ok(match e {
            Expr::Int(i) => G::I(*i as i32),
            Expr::Float(x) => G::F(*x as f32),
            Expr::Bool(b) => G::B(*b),
            Expr::Color(c) => {
                let (r, g, b, a) = crate::value::color_parts(*c);
                G::V(vec![r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a as f32 / 255.0])
            }
            Expr::Var(n, _) => match self.vars.get(n) {
                Some(v) => v.clone(),
                None => match n.as_str() {
                    "PI" => G::F(std::f32::consts::PI),
                    "TAU" => G::F(std::f32::consts::TAU),
                    _ => match self.m.consts.iter().find(|c| &c.0 == n) {
                        Some(c) => {
                            let e = c.1.clone();
                            self.eval(&e)?
                        }
                        None => return Err(Esc::Err(format!("`{n}` isn't defined"))),
                    },
                },
            },
            Expr::Unary("!", x) => G::B(!self.truthy(x)?),
            Expr::Unary(_, x) => match self.eval(x)? {
                G::I(i) => G::I(-i),
                G::V(v) => G::V(v.iter().map(|x| -x).collect()),
                g => G::F(-f(&g)?),
            },
            Expr::Binary("&&", a, b) => G::B(self.truthy(a)? && self.truthy(b)?),
            Expr::Binary("||", a, b) => G::B(self.truthy(a)? || self.truthy(b)?),
            Expr::Binary(op, a, b) => {
                let (x, y) = (self.eval(a)?, self.eval(b)?);
                self.bin(op, x, y)?
            }
            Expr::Ternary(c, a, b) => {
                if self.truthy(c)? {
                    self.eval(a)?
                } else {
                    self.eval(b)?
                }
            }
            Expr::Index(b, i) => {
                let i = f(&self.eval(i)?)? as i64;
                match &**b {
                    Expr::Var(bn, _) if self.bufs.borrow().contains_key(bn) => {
                        let bufs = self.bufs.borrow();
                        let buf = &bufs[bn];
                        if i < 0 || i as usize >= buf.len() {
                            return Err(Esc::Err(format!("index {i} is outside `{bn}` (length {})", buf.len())));
                        }
                        G::F(buf[i as usize])
                    }
                    other => match self.eval(other)? {
                        G::V(v) => G::F(*v.get(i as usize).ok_or(Esc::Err("vector index out of range".into()))?),
                        _ => return Err(Esc::Err("can't index that".into())),
                    },
                }
            }
            Expr::Field(o, name, _) => {
                let v = vecof(&self.eval(o)?, 4)?;
                let comps: Result<Vec<f32>, Esc> = name.chars().map(|c| v.get(SWIZ.find(c).unwrap_or(0) % 4).copied().ok_or(Esc::Err(format!("no `.{c}`")))).collect();
                let comps = comps?;
                if comps.len() == 1 {
                    G::F(comps[0])
                } else {
                    G::V(comps)
                }
            }
            Expr::Call(fun, args, _) => {
                let Expr::Var(name, _) = &**fun else { return Err(Esc::Err("only functions can be called".into())) };
                let mut a = vec![];
                for x in args {
                    a.push(self.eval(x)?);
                }
                if let Some((def, ps, _)) = self.m.helper(name) {
                    let mut sub = Cpu { m: self.m, vars: HashMap::new(), bufs: self.bufs, steps: self.steps };
                    for ((n, t), v) in ps.iter().zip(a) {
                        sub.vars.insert(n.clone(), if *t == Ty::I32 { G::I(f(&v).unwrap_or(0.0) as i32) } else { v });
                    }
                    return match sub.block(def.body.as_ref().unwrap()) {
                        Err(Esc::Return(v)) => Ok(v),
                        Ok(()) => Ok(G::F(0.0)),
                        Err(e) => Err(e),
                    };
                }
                self.call(name, a)?
            }
            _ => return Err(Esc::Err("not available on the GPU".into())),
        })
    }

    fn call(&mut self, name: &str, a: Vec<G>) -> Result<G, Esc> {
        let map1 = |g: &G, op: fn(f32) -> f32| -> Result<G, Esc> {
            Ok(match g {
                G::V(v) => G::V(v.iter().map(|x| op(*x)).collect()),
                g => G::F(op(f(g)?)),
            })
        };
        let n = a.iter().find_map(|g| if let G::V(v) = g { Some(v.len()) } else { None });
        let zip3 = |op: &dyn Fn(f32, f32, f32) -> f32, x: &G, y: &G, z: &G| -> Result<G, Esc> {
            match n {
                Some(n) => {
                    let (x, y, z) = (vecof(x, n)?, vecof(y, n)?, vecof(z, n)?);
                    Ok(G::V((0..n).map(|i| op(x[i], y[i], z[i])).collect()))
                }
                None => Ok(G::F(op(f(x)?, f(y)?, f(z)?))),
            }
        };
        let arg = |i: usize| a.get(i).cloned().unwrap_or(G::F(0.0));
        Ok(match name {
            "sin" => map1(&arg(0), f32::sin)?,
            "cos" => map1(&arg(0), f32::cos)?,
            "tan" => map1(&arg(0), f32::tan)?,
            "sqrt" => map1(&arg(0), f32::sqrt)?,
            "abs" => match arg(0) {
                G::I(i) => G::I(i.abs()),
                g => map1(&g, f32::abs)?,
            },
            "floor" => map1(&arg(0), f32::floor)?,
            "ceil" => map1(&arg(0), f32::ceil)?,
            "round" => map1(&arg(0), f32::round)?,
            "fract" => map1(&arg(0), |x| x - x.floor())?,
            "exp" => map1(&arg(0), f32::exp)?,
            "log" => map1(&arg(0), f32::ln)?,
            "sign" => map1(&arg(0), |x| if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else { 0.0 })?,
            "min" | "max" if n.is_none() && matches!((arg(0), arg(1)), (G::I(_), G::I(_))) => {
                let (G::I(x), G::I(y)) = (arg(0), arg(1)) else { unreachable!() };
                G::I(if name == "min" { x.min(y) } else { x.max(y) })
            }
            "min" => zip3(&|x, y, _| x.min(y), &arg(0), &arg(1), &G::F(0.0))?,
            "max" => zip3(&|x, y, _| x.max(y), &arg(0), &arg(1), &G::F(0.0))?,
            "pow" => zip3(&|x, y, _| x.powf(y), &arg(0), &arg(1), &G::F(0.0))?,
            "atan2" => G::F(f(&arg(0))?.atan2(f(&arg(1))?)),
            "clamp" => zip3(&|x, lo, hi| x.clamp(lo.min(hi), hi.max(lo)), &arg(0), &arg(1), &arg(2))?,
            "mix" => match (arg(0), arg(1), arg(2)) {
                (x, y, t @ (G::F(_) | G::I(_))) if n.is_some() => {
                    let n = n.unwrap();
                    let (x, y, t) = (vecof(&x, n)?, vecof(&y, n)?, f(&t)?);
                    G::V((0..n).map(|i| x[i] + (y[i] - x[i]) * t).collect())
                }
                (x, y, t) => zip3(&|x, y, t| x + (y - x) * t, &x, &y, &t)?,
            },
            "step" => zip3(&|edge, x, _| if x < edge { 0.0 } else { 1.0 }, &arg(0), &arg(1), &G::F(0.0))?,
            "smoothstep" => zip3(
                &|e0, e1, x| {
                    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
                    t * t * (3.0 - 2.0 * t)
                },
                &arg(0),
                &arg(1),
                &arg(2),
            )?,
            "length" => G::F(vecof(&arg(0), 1)?.iter().map(|x| x * x).sum::<f32>().sqrt()),
            "distance" => {
                let (x, y) = (vecof(&arg(0), 1)?, vecof(&arg(1), 1)?);
                G::F(x.iter().zip(y.iter()).map(|(p, q)| (p - q) * (p - q)).sum::<f32>().sqrt())
            }
            "dot" => {
                let (x, y) = (vecof(&arg(0), 1)?, vecof(&arg(1), 1)?);
                G::F(x.iter().zip(y.iter()).map(|(p, q)| p * q).sum())
            }
            "normalize" => {
                let v = vecof(&arg(0), 1)?;
                let l = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-12);
                G::V(v.iter().map(|x| x / l).collect())
            }
            "vec2" | "vec3" | "vec4" | "rgba" => {
                let want = if name == "rgba" { 4 } else { name[3..].parse::<usize>().unwrap() };
                let mut v = vec![];
                for g in &a {
                    match g {
                        G::V(x) => v.extend_from_slice(x),
                        g => v.push(f(g)?),
                    }
                }
                if v.len() == 1 {
                    v = vec![v[0]; want];
                }
                if v.len() != want {
                    return Err(Esc::Err(format!("{name} needs {want} components, got {}", v.len())));
                }
                G::V(v)
            }
            "float" => G::F(f(&arg(0))?),
            "int" => G::I(f(&arg(0))? as i32),
            _ => return Err(Esc::Err(format!("`{name}` isn't a GPU function"))),
        })
    }
}

/// A kernel or fragment as a script value: `blur.run(...)`, `canvas.shade(glow, …)`.
pub struct ShaderRef {
    pub name: String,
    pub module: Rc<Module>,
}

impl HostObject for ShaderRef {
    fn type_name(&self) -> &str {
        match self.module.entry(&self.name).map(|e| e.kind) {
            Some(EntryKind::Kernel) => "kernel",
            _ => "fragment",
        }
    }
    fn field(&self, name: &str) -> Option<Value> {
        match name {
            "name" => Some(Value::str(&self.name)),
            _ => None,
        }
    }
    fn call(&self, _it: &mut crate::interp::Interp, method: &str, args: Vec<Value>) -> Result<Value, String> {
        match method {
            // kernel.run({data: [1, 2, 3]}, {k: 2}) -> {data: [...]}
            "run" => {
                let mut bufs = HashMap::new();
                if let Some(Value::Map(m)) = args.first() {
                    for (k, v) in m.borrow().iter() {
                        let Value::List(l) = v else { return Err(format!("run: buffer `{k}` must be a list")) };
                        bufs.insert(k.clone(), l.borrow().iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect());
                    }
                }
                let mut unis = HashMap::new();
                if let Some(Value::Map(m)) = args.get(1) {
                    for (k, v) in m.borrow().iter() {
                        unis.insert(k.clone(), v.as_f64().unwrap_or(0.0) as f32);
                    }
                }
                self.module.run_kernel(&self.name, &mut bufs, &unis)?;
                let mut out = std::collections::BTreeMap::new();
                for (k, v) in bufs {
                    out.insert(k, Value::list(v.into_iter().map(|x| Value::Float(x as f64)).collect()));
                }
                Ok(Value::map(out))
            }
            "wgsl" => Ok(Value::str(&self.module.to_wgsl()?)),
            _ => Err(format!("{} has no method `{method}` (run, wgsl)", self.type_name())),
        }
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"
const GAIN = 2
func soften(x: float) -> float { return x * 0.5 + 0.25 }
kernel scale(buf data: [float], buf out: [float], uniform k: float) {
    out[gid] = soften(data[gid]) * k * GAIN
}
kernel prefix(buf xs: [float]) {
    var acc: float = 0.0
    for i in 0..gid + 1 { acc += xs[i] }
    xs[gid] = acc
}
fragment glow(uv: vec2, uniform time: float) -> color {
    let d = distance(uv, vec2(0.5, 0.5))
    let c = mix(#2f7cf6, #ffffff, smoothstep(0.5, 0.0, d))
    return vec4(c.rgb, 1.0)
}
"#;

    #[test]
    fn compiles_to_valid_wgsl() {
        let m = Module::parse(SRC).unwrap();
        let wgsl = m.validate().unwrap();
        assert!(wgsl.contains("@compute @workgroup_size(64)\nfn scale("), "{wgsl}");
        assert!(wgsl.contains("@fragment\nfn glow("), "{wgsl}");
    }

    #[test]
    fn cpu_kernel_and_fragment() {
        let m = Module::parse(SRC).unwrap();
        let mut b = HashMap::new();
        b.insert("data".to_string(), vec![0.0, 1.0, 2.0]);
        b.insert("out".to_string(), vec![0.0; 3]);
        let mut u = HashMap::new();
        u.insert("k".to_string(), 3.0);
        m.run_kernel("scale", &mut b, &u).unwrap();
        assert_eq!(b["out"], vec![1.5, 4.5, 7.5]);
        let c = m.shade("glow", [0.5, 0.5], [100.0, 100.0], &HashMap::new()).unwrap();
        assert_eq!(c, [1.0, 1.0, 1.0, 1.0]);
        let edge = m.shade("glow", [0.0, 0.0], [100.0, 100.0], &HashMap::new()).unwrap();
        assert!((edge[0] - 0x2f as f32 / 255.0).abs() < 1e-3);
    }

    #[test]
    fn errors_are_clear() {
        assert!(Module::parse("kernel k(uniform a: float) { }").unwrap_err().contains("`buf`"));
        assert!(Module::parse("fragment f(p: vec2) -> color { return #fff }").unwrap_err().contains("uv: vec2"));
        assert!(Module::parse("kernel k(buf a: [float]) { a[gid] = nope }").unwrap_err().contains("nope"));
        assert!(Module::parse("kernel k(buf a: [float]) { let v = vec2(1.0, 2.0); a[gid] = v.z }").unwrap_err().contains("no `z`"));
    }
}
