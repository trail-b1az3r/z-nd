//! The syntax tree shared by .zsp programs, .zsx headers and .zsg kernels.

use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(Vec<StrSeg>),
    Color(u32),
    Var(String, usize),
    List(Vec<Expr>),
    Map(Vec<(Expr, Expr)>),
    StructLit(String, Vec<(String, Expr)>),
    Unary(&'static str, Box<Expr>),
    Binary(&'static str, Box<Expr>, Box<Expr>),
    /// `cond ? a : b`
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>, usize),
    Index(Box<Expr>, Box<Expr>),
    /// `a.b` (or `a?.b` when the flag is set)
    Field(Box<Expr>, String, bool),
    Lambda(Rc<FuncDef>),
    /// `a..b` / `a..=b`
    Range(Box<Expr>, Box<Expr>, bool),
    Match(Box<Expr>, Vec<Arm>),
    // ── Zsharpend (.zxx) ──
    /// `[elem for vars in iter if cond]`, or a map `{key: value for …}` when `value` is set.
    Comp { elem: Box<Expr>, value: Option<Box<Expr>>, vars: Vec<String>, iter: Box<Expr>, cond: Option<Box<Expr>> },
    /// `a[1:3]`, `a[::2]`, `a[i, j]` (Python-style subscripts).
    Subscript(Box<Expr>, Vec<Sub>),
    /// `expr?` (Rust): unwraps `Ok(v)`; returns early from the function on `Err` or `null`.
    Try(Box<Expr>, usize),
    /// `name=value` in a call (Python keyword argument).
    Named(String, Box<Expr>),
    /// `Enum::Variant` (Rust).
    Path(String, String, usize),
}

/// One part of a subscript.
#[derive(Clone, Debug, PartialEq)]
pub enum Sub {
    At(Expr),
    /// `start:stop:step`, each optional.
    Slice(Option<Expr>, Option<Expr>, Option<Expr>),
}

/// A `case` in a HolyC-style `switch`: values, or inclusive ranges `a...b`.
#[derive(Clone, Debug, PartialEq)]
pub struct Case {
    pub pats: Vec<(Expr, Option<Expr>)>,
    pub body: Vec<Stmt>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StrSeg {
    Lit(String),
    Expr(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Arm {
    /// Alternatives (`1 | 2`); empty = `_`.
    pub pats: Vec<Expr>,
    pub guard: Option<Expr>,
    pub body: Vec<Stmt>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    /// `let`/`const`/typed declaration (`int x = 1`), maybe `global`.
    Let { name: String, ty: Option<Type>, value: Option<Expr>, global: bool, constant: bool, line: usize },
    /// `target op= value` (op "=" for plain assignment).
    Assign { target: Expr, op: &'static str, value: Expr, line: usize },
    Expr(Expr, usize),
    If { cond: Expr, then: Vec<Stmt>, els: Option<Vec<Stmt>>, line: usize },
    While { cond: Expr, body: Vec<Stmt>, line: usize },
    For { var: String, index: Option<String>, iter: Expr, body: Vec<Stmt>, line: usize },
    Return(Option<Expr>, usize),
    Break(usize),
    Continue(usize),
    Throw(Expr, usize),
    Try { body: Vec<Stmt>, var: Option<String>, catch: Vec<Stmt>, line: usize },
    Block(Vec<Stmt>),
    // ── Zsharpend (.zxx) ──
    /// `let (a, b) = pair` / `a, b = b, a`.
    Destructure { names: Vec<String>, value: Expr, declare: bool, line: usize },
    /// HolyC: a string statement prints it; with arguments it's a printf format.
    Print { fmt: Expr, args: Vec<Expr>, line: usize },
    /// HolyC: a bare name as a statement calls it when it's a function.
    Bare(Expr, usize),
    /// HolyC `switch` (no fall-through; `break` leaves it).
    Switch { value: Expr, cases: Vec<Case>, default: Option<Vec<Stmt>>, line: usize },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Type {
    pub name: String,
    /// `[float]` (a .zsg buffer) / `list<int>`.
    pub array: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Option<Type>,
    pub default: Option<Expr>,
    /// .zsg: `buf` / `uniform` / "" (plain).
    pub kind: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FuncDef {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Option<Type>,
    /// None: a declaration only (.zsx header / `extern func`).
    pub body: Option<Vec<Stmt>>,
    pub line: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Item {
    Func(Rc<FuncDef>),
    Struct { name: String, fields: Vec<(String, Option<Type>)>, line: usize },
    /// `impl Type { … }`, or `impl Trait for Type { … }` (`.zxx`).
    Impl { name: String, methods: Vec<Rc<FuncDef>>, line: usize, of_trait: Option<String> },
    /// `.zxx`: `enum Shape { Circle(r), Rect(w, h), Empty }` — variant names and fields.
    Enum { name: String, variants: Vec<(String, Vec<String>)>, line: usize },
    /// `.zxx`: `trait Area { fn area(self) -> float }` — required (and default) methods.
    Trait { name: String, methods: Vec<Rc<FuncDef>>, line: usize },
    /// `.zxx`: `import nd`, `import nd as np`, `from geometry import area, Point as P`.
    ImportModule { name: String, alias: Option<String>, names: Option<Vec<(String, Option<String>)>>, line: usize },
    /// `on event(params) { … }` or `on event "filter" (params) { … }`
    On { event: String, filter: Option<Expr>, params: Vec<String>, body: Vec<Stmt>, line: usize },
    Import { path: String, line: usize },
    /// .zsg compute kernel.
    Kernel(Rc<FuncDef>),
    /// .zsg fragment shader (per pixel, returns a colour).
    Fragment(Rc<FuncDef>),
    /// A top-level statement (run in order before `Main`).
    Stmt(Stmt),
}

/// Which kind of source a file is; decides what it may contain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `.zsp` (or Z#'s `.zs`): a program.
    Program,
    /// `.zsx`: declarations only — function signatures, structs, constants.
    Header,
    /// `.zsg`: GPU code — kernels, fragment shaders and typed helpers.
    Gpu,
    /// `.zxx`: a Zsharpend program — everything a `.zsp` holds, plus the
    /// Python, Rust and HolyC features (and their keywords).
    Zxx,
}

impl Kind {
    /// A program file (`.zsp`, `.zs` or `.zxx`).
    pub fn is_program(self) -> bool {
        matches!(self, Kind::Program | Kind::Zxx)
    }

    pub fn from_path(path: &str) -> Option<Kind> {
        let ext = path.rsplit('.').next()?.to_ascii_lowercase();
        match ext.as_str() {
            "zsp" | "zs" => Some(Kind::Program),
            "zxx" => Some(Kind::Zxx),
            "zsx" => Some(Kind::Header),
            "zsg" => Some(Kind::Gpu),
            _ => None,
        }
    }
}
