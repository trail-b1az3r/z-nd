//! Runtime values.

use std::any::Any;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::rc::Rc;

use crate::ast::FuncDef;
use crate::interp::Interp;

pub type NativeFn = dyn Fn(&mut Interp, Vec<Value>) -> Result<Value, String>;

/// Something the host gives scripts (a canvas, a page, a shader…). Scripts
/// see its fields and call its methods; nothing else.
pub trait HostObject {
    fn type_name(&self) -> &str;
    fn field(&self, _name: &str) -> Option<Value> {
        None
    }
    fn call(&self, it: &mut Interp, method: &str, args: Vec<Value>) -> Result<Value, String>;
    fn as_any(&self) -> &dyn Any;
    /// `self op other` (or `other op self` when `swapped`). `None`: not supported.
    fn binary(&self, _it: &mut Interp, _op: &str, _other: &Value, _swapped: bool) -> Option<Result<Value, String>> {
        None
    }
    /// `-self`.
    fn negate(&self, _it: &mut Interp) -> Option<Result<Value, String>> {
        None
    }
    /// `self[parts]` — one part per comma (`a[1, 2:4]`).
    fn index(&self, _it: &mut Interp, _parts: &[Index]) -> Option<Result<Value, String>> {
        None
    }
    /// `self[parts] = value`.
    fn set_index(&self, _it: &mut Interp, _parts: &[Index], _value: Value) -> Option<Result<(), String>> {
        None
    }
    /// How `Printl` shows it (default: its type name).
    fn show(&self) -> Option<String> {
        None
    }
}

/// One part of a subscript, evaluated: an index or a `start:stop:step` slice.
#[derive(Clone)]
pub enum Index {
    At(Value),
    Slice(Option<i64>, Option<i64>, Option<i64>),
}

/// Python's slice rules: the indices `start:stop:step` selects from `len`
/// items (negative counts from the end; a negative step walks backwards).
pub fn slice_indices(len: usize, start: Option<i64>, stop: Option<i64>, step: Option<i64>) -> Result<Vec<usize>, String> {
    let n = len as i64;
    let step = step.unwrap_or(1);
    if step == 0 {
        return Err("a slice step can't be zero".into());
    }
    let norm = |x: i64| if x < 0 { x + n } else { x };
    let mut out = vec![];
    if step > 0 {
        let a = start.map(norm).unwrap_or(0).clamp(0, n);
        let b = stop.map(norm).unwrap_or(n).clamp(0, n);
        let mut i = a;
        while i < b {
            out.push(i as usize);
            i += step;
        }
    } else {
        let a = start.map(norm).unwrap_or(n - 1).clamp(-1, n - 1);
        let b = stop.map(norm).unwrap_or(-1).clamp(-1, n - 1);
        let mut i = a;
        while i > b {
            out.push(i as usize);
            i += step;
        }
    }
    Ok(out)
}

#[derive(Clone)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(Rc<str>),
    /// RGBA, 8 bits each (`0xRRGGBBAA`).
    Color(u32),
    List(Rc<RefCell<Vec<Value>>>),
    Map(Rc<RefCell<BTreeMap<String, Value>>>),
    Struct(Rc<RefCell<StructVal>>),
    Func(Rc<Closure>),
    Native(Rc<Native>),
    /// Half-open `start..end`.
    Range(i64, i64),
    Object(Rc<dyn HostObject>),
}

pub struct StructVal {
    pub name: String,
    pub fields: Vec<(String, Value)>,
}

pub struct Closure {
    pub def: Rc<FuncDef>,
    pub env: Env,
}

pub struct Native {
    pub name: String,
    pub f: Box<NativeFn>,
}

/// A scope: variables plus the scope it sits in.
pub struct Scope {
    pub vars: HashMap<String, Value>,
    pub consts: Vec<String>,
    pub parent: Option<Env>,
}

pub type Env = Rc<RefCell<Scope>>;

pub fn new_scope(parent: Option<Env>) -> Env {
    Rc::new(RefCell::new(Scope { vars: HashMap::new(), consts: vec![], parent }))
}

impl Value {
    pub fn str(s: &str) -> Value {
        Value::Str(Rc::from(s))
    }
    pub fn list(v: Vec<Value>) -> Value {
        Value::List(Rc::new(RefCell::new(v)))
    }
    pub fn map(m: BTreeMap<String, Value>) -> Value {
        Value::Map(Rc::new(RefCell::new(m)))
    }
    pub fn native(name: &str, f: impl Fn(&mut Interp, Vec<Value>) -> Result<Value, String> + 'static) -> Value {
        Value::Native(Rc::new(Native { name: name.into(), f: Box::new(f) }))
    }

    pub fn type_name(&self) -> String {
        match self {
            Value::Null => "null".into(),
            Value::Bool(_) => "bool".into(),
            Value::Int(_) => "int".into(),
            Value::Float(_) => "float".into(),
            Value::Str(_) => "string".into(),
            Value::Color(_) => "color".into(),
            Value::List(_) => "list".into(),
            Value::Map(_) => "map".into(),
            Value::Struct(s) => s.borrow().name.clone(),
            Value::Func(_) | Value::Native(_) => "func".into(),
            Value::Range(..) => "range".into(),
            Value::Object(o) => o.type_name().into(),
        }
    }

    pub fn truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::List(l) => !l.borrow().is_empty(),
            Value::Map(m) => !m.borrow().is_empty(),
            _ => true,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Float(f) => Some(*f),
            Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Float(f) if f.is_finite() => Some(*f as i64),
            Value::Bool(b) => Some(*b as i64),
            _ => None,
        }
    }

    /// Structural equality (numbers compare by value).
    pub fn equals(&self, o: &Value) -> bool {
        match (self, o) {
            (Value::Null, Value::Null) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Color(a), Value::Color(b)) => a == b,
            (Value::Range(a, b), Value::Range(c, d)) => a == c && b == d,
            (Value::List(a), Value::List(b)) => {
                let (a, b) = (a.borrow(), b.borrow());
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.equals(y))
            }
            (Value::Map(a), Value::Map(b)) => {
                let (a, b) = (a.borrow(), b.borrow());
                a.len() == b.len() && a.iter().all(|(k, v)| b.get(k).is_some_and(|w| v.equals(w)))
            }
            (Value::Struct(a), Value::Struct(b)) => {
                let (a, b) = (a.borrow(), b.borrow());
                a.name == b.name && a.fields.len() == b.fields.len() && a.fields.iter().zip(b.fields.iter()).all(|(x, y)| x.0 == y.0 && x.1.equals(&y.1))
            }
            (Value::Func(a), Value::Func(b)) => Rc::ptr_eq(a, b),
            (Value::Native(a), Value::Native(b)) => Rc::ptr_eq(a, b),
            _ => match (self.as_f64(), o.as_f64()) {
                (Some(a), Some(b)) if !matches!(self, Value::Bool(_)) && !matches!(o, Value::Bool(_)) => a == b,
                _ => false,
            },
        }
    }
}

pub fn color_parts(c: u32) -> (u8, u8, u8, u8) {
    ((c >> 24) as u8, (c >> 16) as u8, (c >> 8) as u8, c as u8)
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Null => write!(f, "null"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Int(i) => write!(f, "{i}"),
            Value::Float(x) => {
                if x.fract() == 0.0 && x.abs() < 1e15 {
                    write!(f, "{x:.1}")
                } else {
                    write!(f, "{x}")
                }
            }
            Value::Str(s) => write!(f, "{s}"),
            Value::Color(c) => {
                let (r, g, b, a) = color_parts(*c);
                if a == 255 {
                    write!(f, "#{r:02x}{g:02x}{b:02x}")
                } else {
                    write!(f, "#{r:02x}{g:02x}{b:02x}{a:02x}")
                }
            }
            Value::List(l) => {
                write!(f, "[")?;
                for (i, v) in l.borrow().iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    if let Value::Str(s) = v {
                        write!(f, "\"{s}\"")?;
                    } else {
                        write!(f, "{v}")?;
                    }
                }
                write!(f, "]")
            }
            Value::Map(m) => {
                write!(f, "{{")?;
                for (i, (k, v)) in m.borrow().iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
            Value::Struct(s) => {
                let s = s.borrow();
                write!(f, "{} {{ ", s.name)?;
                for (i, (k, v)) in s.fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, " }}")
            }
            Value::Func(c) => write!(f, "<func {}>", c.def.name),
            Value::Native(n) => write!(f, "<func {}>", n.name),
            Value::Range(a, b) => write!(f, "{a}..{b}"),
            Value::Object(o) => match o.show() {
                Some(t) => write!(f, "{t}"),
                None => write!(f, "<{}>", o.type_name()),
            },
        }
    }
}
