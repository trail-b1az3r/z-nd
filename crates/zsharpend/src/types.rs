//! Runtime type objects: parsing type specs (`map<text, list<int>>`),
//! checking values against them, safe conversion, and the `Type` value
//! scripts see through `type_of`, `is`, `as` and `try_as`.
//!
//! Older names keep working as aliases: `string`/`str`/`String` are `text`,
//! `I64`/`i64`/`isize`… are `int`, `F64`/`f32`… are `float`, `u8` is `byte`,
//! `U0` is `void`, `nil`/`none` are `null`.

use std::any::Any;
use std::collections::BTreeMap;
use std::fmt;
use std::rc::Rc;

use crate::interp::Interp;
use crate::value::{HostObject, Value};

#[derive(Clone, Debug, PartialEq)]
pub enum TypeSpec {
    Any,
    Dynamic,
    Int,
    Float,
    Number,
    Text,
    Bool,
    Byte,
    Char,
    Null,
    Void,
    Color,
    Func,
    Range,
    List(Box<TypeSpec>),
    Array(Box<TypeSpec>),
    Map(Box<TypeSpec>, Box<TypeSpec>),
    Tuple(Vec<TypeSpec>),
    Optional(Box<TypeSpec>),
    Result(Box<TypeSpec>, Box<TypeSpec>),
    /// A struct, enum, trait or host object type, by name.
    Named(String),
}

use TypeSpec as T;

impl TypeSpec {
    /// Parses `int`, `list<text>`, `map<text, list<int>>`, `[int]`,
    /// `(int, text)`, `int?`, `optional<int>`, `result<int, text>`.
    pub fn parse(src: &str) -> Result<TypeSpec, String> {
        let toks = lex(src)?;
        let mut p = SpecParser { toks, pos: 0 };
        let t = p.spec()?;
        if p.pos < p.toks.len() {
            return Err(format!("unexpected `{}` in type `{src}`", p.toks[p.pos]));
        }
        Ok(t)
    }

    /// A bare name (no parameters) to its type. Unknown names are `Named`.
    pub fn from_name(name: &str) -> TypeSpec {
        match name {
            "any" => T::Any,
            "dynamic" => T::Dynamic,
            "int" | "integer" | "i64" | "I64" | "isize" | "u64" | "U64" | "usize" | "i32" | "I32" | "u32" | "U32" | "i16" | "I16" | "u16" | "U16"
            | "i8" | "I8" => T::Int,
            "float" | "f64" | "F64" | "f32" | "F32" => T::Float,
            "number" | "numeric" => T::Number,
            "text" | "string" | "str" | "String" => T::Text,
            "bool" | "boolean" | "Bool" => T::Bool,
            "byte" | "u8" | "U8" => T::Byte,
            "char" => T::Char,
            "null" | "nil" | "none" | "None" => T::Null,
            "void" | "U0" => T::Void,
            "color" => T::Color,
            "func" | "fn" => T::Func,
            "range" => T::Range,
            "list" => T::List(Box::new(T::Any)),
            "array" => T::Array(Box::new(T::Any)),
            "map" => T::Map(Box::new(T::Text), Box::new(T::Any)),
            "tuple" => T::Tuple(vec![]),
            "optional" => T::Optional(Box::new(T::Any)),
            "result" | "Result" => T::Result(Box::new(T::Any), Box::new(T::Any)),
            _ => T::Named(name.to_string()),
        }
    }

    /// The type a value has right now. Containers report `any` elements:
    /// a list's contents can change, its type of `list` can't.
    pub fn of(v: &Value) -> TypeSpec {
        match v {
            Value::Null => T::Null,
            Value::Bool(_) => T::Bool,
            Value::Int(_) => T::Int,
            Value::Float(_) => T::Float,
            Value::Str(_) => T::Text,
            Value::Color(_) => T::Color,
            Value::List(_) => T::List(Box::new(T::Any)),
            Value::Map(_) => T::Map(Box::new(T::Text), Box::new(T::Any)),
            Value::Func(_) | Value::Native(_) => T::Func,
            Value::Range(..) => T::Range,
            Value::Struct(s) => T::Named(s.borrow().name.clone()),
            Value::Object(o) => match o.as_any().downcast_ref::<TypeObj>() {
                Some(_) => T::Named("type".into()),
                None => T::Named(o.type_name().into()),
            },
        }
    }

    /// The type's parameters (`map<text, int>` → text, int).
    pub fn params(&self) -> Vec<TypeSpec> {
        match self {
            T::List(t) | T::Array(t) | T::Optional(t) => vec![(**t).clone()],
            T::Map(k, v) | T::Result(k, v) => vec![(**k).clone(), (**v).clone()],
            T::Tuple(ts) => ts.clone(),
            _ => vec![],
        }
    }

    /// The name without parameters: `list`, `map`, `optional`, `Point`.
    pub fn base(&self) -> String {
        match self {
            T::List(_) => "list".into(),
            T::Array(_) => "array".into(),
            T::Map(..) => "map".into(),
            T::Tuple(_) => "tuple".into(),
            T::Optional(_) => "optional".into(),
            T::Result(..) => "result".into(),
            T::Named(n) => n.clone(),
            other => other.to_string(),
        }
    }

    /// Does `v` have this type? `it` resolves enum variants to their enum.
    pub fn check(&self, v: &Value, it: Option<&Interp>) -> bool {
        self.check_in(v, it, false)
    }

    /// What an annotation accepts: [`check`](Self::check), except an int is
    /// also a float (`let xs: list<float> = [1, 2.5]`).
    pub fn accepts(&self, v: &Value, it: Option<&Interp>) -> bool {
        self.check_in(v, it, true)
    }

    fn check_in(&self, v: &Value, it: Option<&Interp>, widen: bool) -> bool {
        let check = |t: &TypeSpec, x: &Value| t.check_in(x, it, widen);
        match (self, v) {
            (T::Any | T::Dynamic, _) => true,
            (T::Float, Value::Int(_)) => widen,
            (T::Int, Value::Int(_)) | (T::Float, Value::Float(_)) | (T::Number, Value::Int(_) | Value::Float(_)) => true,
            (T::Text, Value::Str(_)) | (T::Bool, Value::Bool(_)) | (T::Color, Value::Color(_)) => true,
            (T::Byte, Value::Int(i)) => (0..=255).contains(i),
            (T::Char, Value::Str(s)) => s.chars().count() == 1,
            (T::Null | T::Void, Value::Null) => true,
            (T::Func, Value::Func(_) | Value::Native(_)) | (T::Range, Value::Range(..)) => true,
            (T::List(t) | T::Array(t), Value::List(l)) => l.borrow().iter().all(|x| check(t, x)),
            (T::List(t) | T::Array(t), Value::Range(..)) => matches!(**t, T::Any | T::Dynamic | T::Int | T::Number),
            (T::Map(k, val), Value::Map(m)) => m.borrow().iter().all(|(key, x)| check(k, &Value::str(key)) && check(val, x)),
            (T::Tuple(ts), Value::List(l)) => {
                let l = l.borrow();
                ts.is_empty() || (l.len() == ts.len() && l.iter().zip(ts).all(|(x, t)| check(t, x)))
            }
            (T::Optional(_), Value::Null) => true,
            (T::Optional(t), x) => check(t, x),
            (T::Result(ok, err), Value::Struct(s)) => {
                let s = s.borrow();
                let inner = s.fields.first().map(|f| f.1.clone()).unwrap_or(Value::Null);
                match s.name.as_str() {
                    "Ok" => check(ok, &inner),
                    "Err" => check(err, &inner),
                    _ => false,
                }
            }
            (T::Named(n), Value::Struct(s)) => {
                let s = s.borrow();
                &s.name == n || it.is_some_and(|it| it.variant_of.get(&s.name) == Some(n) || it.implements(&s.name, n))
            }
            (T::Named(n), Value::Object(o)) => o.type_name() == n || (n == "type" && o.as_any().is::<TypeObj>()),
            _ => false,
        }
    }

    /// An int stored where a float is expected becomes a float.
    pub fn widen(&self, v: Value) -> Value {
        match (self, &v) {
            (T::Float, Value::Int(i)) => Value::Float(*i as f64),
            (T::Optional(t), Value::Int(_)) => t.widen(v),
            _ => v,
        }
    }

    /// Converts `v` to this type, or says why it can't. Never loses data
    /// silently: `"12"` → int works, `"twelve"` → int is an error, and
    /// `300` → byte is an error rather than a wrap.
    pub fn convert(&self, v: &Value, it: Option<&Interp>) -> Result<Value, String> {
        if self.check(v, it) && !matches!(self, T::List(_) | T::Array(_)) || matches!(self, T::Any | T::Dynamic) {
            return Ok(v.clone());
        }
        let fail = || Err(format!("can't convert {} ({}) to {self}", v.type_name(), show(v)));
        Ok(match (self, v) {
            (T::Int, Value::Float(f)) if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.2e18 => Value::Int(*f as i64),
            (T::Int, Value::Bool(b)) => Value::Int(*b as i64),
            (T::Int, Value::Str(s)) => match s.trim().parse::<i64>() {
                Ok(i) => Value::Int(i),
                Err(_) => return fail(),
            },
            (T::Float, Value::Int(i)) => Value::Float(*i as f64),
            (T::Float, Value::Bool(b)) => Value::Float(*b as i64 as f64),
            (T::Float | T::Number, Value::Str(s)) => {
                let s = s.trim();
                match (s.parse::<i64>(), s.parse::<f64>()) {
                    (Ok(i), _) if *self == T::Number => Value::Int(i),
                    (_, Ok(f)) => Value::Float(f),
                    _ => return fail(),
                }
            }
            (T::Number, Value::Bool(b)) => Value::Int(*b as i64),
            (T::Text, x) => Value::str(&x.to_string()),
            (T::Bool, Value::Str(s)) => match s.trim().to_ascii_lowercase().as_str() {
                "true" | "yes" | "1" => Value::Bool(true),
                "false" | "no" | "0" | "" => Value::Bool(false),
                _ => return fail(),
            },
            (T::Bool, Value::Int(_) | Value::Float(_) | Value::Null | Value::List(_) | Value::Map(_)) => Value::Bool(v.truthy()),
            (T::Byte, x) => match T::Int.convert(x, it)? {
                Value::Int(i) if (0..=255).contains(&i) => Value::Int(i),
                _ => return fail(),
            },
            (T::Char, Value::Int(i)) => match u32::try_from(*i).ok().and_then(char::from_u32) {
                Some(c) => Value::str(&c.to_string()),
                None => return fail(),
            },
            (T::Color, Value::Str(s)) => match parse_hex_color(s) {
                Some(c) => Value::Color(c),
                None => return fail(),
            },
            (T::Color, Value::Int(i)) if (0..=u32::MAX as i64).contains(i) => Value::Color(*i as u32),
            (T::List(t) | T::Array(t), Value::List(l)) => {
                let items = l.borrow().clone();
                if items.iter().all(|x| t.check(x, it)) {
                    return Ok(v.clone()); // already right: keep the same list
                }
                Value::list(items.iter().map(|x| t.convert(x, it)).collect::<Result<_, _>>().map_err(|e| format!("{e} (in {})", show(v)))?)
            }
            (T::List(t) | T::Array(t), Value::Range(a, b)) => Value::list((*a..*b).map(|i| t.convert(&Value::Int(i), it)).collect::<Result<_, _>>()?),
            (T::List(t) | T::Array(t), Value::Str(s)) => Value::list(s.chars().map(|c| t.convert(&Value::str(&c.to_string()), it)).collect::<Result<_, _>>()?),
            (T::Map(k, val), Value::Map(m)) => {
                let mut out = BTreeMap::new();
                for (key, x) in m.borrow().iter() {
                    k.convert(&Value::str(key), it)?;
                    out.insert(key.clone(), val.convert(x, it)?);
                }
                Value::map(out)
            }
            (T::Tuple(ts), Value::List(l)) if l.borrow().len() == ts.len() => {
                Value::list(l.borrow().iter().zip(ts).map(|(x, t)| t.convert(x, it)).collect::<Result<_, _>>()?)
            }
            (T::Optional(t), x) => t.convert(x, it)?,
            _ => return fail(),
        })
    }
}

fn show(v: &Value) -> String {
    let s = match v {
        Value::Str(s) => format!("\"{s}\""),
        v => v.to_string(),
    };
    if s.chars().count() > 40 {
        format!("{}…", s.chars().take(40).collect::<String>())
    } else {
        s
    }
}

fn parse_hex_color(s: &str) -> Option<u32> {
    let h = s.trim().strip_prefix('#')?;
    let n = u32::from_str_radix(h, 16).ok()?;
    match h.len() {
        6 => Some((n << 8) | 0xff),
        8 => Some(n),
        _ => None,
    }
}

impl fmt::Display for TypeSpec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let simple = match self {
            T::Any => "any",
            T::Dynamic => "dynamic",
            T::Int => "int",
            T::Float => "float",
            T::Number => "number",
            T::Text => "text",
            T::Bool => "bool",
            T::Byte => "byte",
            T::Char => "char",
            T::Null => "null",
            T::Void => "void",
            T::Color => "color",
            T::Func => "func",
            T::Range => "range",
            _ => "",
        };
        if !simple.is_empty() {
            return write!(f, "{simple}");
        }
        match self {
            T::List(t) if **t == T::Any => write!(f, "list"),
            T::List(t) => write!(f, "list<{t}>"),
            T::Array(t) if **t == T::Any => write!(f, "array"),
            T::Array(t) => write!(f, "array<{t}>"),
            T::Map(k, v) if **k == T::Text && **v == T::Any => write!(f, "map"),
            T::Map(k, v) => write!(f, "map<{k}, {v}>"),
            T::Tuple(ts) if ts.is_empty() => write!(f, "tuple"),
            T::Tuple(ts) => write!(f, "({})", ts.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(", ")),
            T::Optional(t) => write!(f, "optional<{t}>"),
            T::Result(a, b) if **a == T::Any && **b == T::Any => write!(f, "result"),
            T::Result(a, b) => write!(f, "result<{a}, {b}>"),
            T::Named(n) => write!(f, "{n}"),
            _ => unreachable!(),
        }
    }
}

// ── Parsing ──

fn lex(src: &str) -> Result<Vec<String>, String> {
    let mut out = vec![];
    let mut chars = src.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c.is_alphanumeric() || c == '_' {
            let mut w = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_alphanumeric() || c == '_' {
                    w.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            out.push(w);
        } else if "<>,[]()?".contains(c) {
            out.push(c.to_string());
            chars.next();
        } else {
            return Err(format!("unexpected `{c}` in type `{src}`"));
        }
    }
    if out.is_empty() {
        return Err("empty type".into());
    }
    Ok(out)
}

struct SpecParser {
    toks: Vec<String>,
    pos: usize,
}

impl SpecParser {
    fn peek(&self) -> Option<&str> {
        self.toks.get(self.pos).map(|s| s.as_str())
    }
    fn eat(&mut self, t: &str) -> bool {
        if self.peek() == Some(t) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, t: &str) -> Result<(), String> {
        if self.eat(t) {
            Ok(())
        } else {
            Err(format!("expected `{t}` in type, found {}", self.peek().map(|s| format!("`{s}`")).unwrap_or("the end".into())))
        }
    }
    fn spec(&mut self) -> Result<TypeSpec, String> {
        let mut t = self.atom()?;
        while self.eat("?") {
            t = T::Optional(Box::new(t));
        }
        Ok(t)
    }
    fn list(&mut self, close: &str) -> Result<Vec<TypeSpec>, String> {
        let mut ts = vec![self.spec()?];
        while self.eat(",") {
            ts.push(self.spec()?);
        }
        self.expect(close)?;
        Ok(ts)
    }
    fn atom(&mut self) -> Result<TypeSpec, String> {
        if self.eat("[") {
            let t = self.spec()?;
            self.expect("]")?;
            return Ok(T::List(Box::new(t)));
        }
        if self.eat("(") {
            return Ok(T::Tuple(self.list(")")?));
        }
        let Some(name) = self.peek().filter(|s| s.chars().next().is_some_and(|c| c.is_alphanumeric() || c == '_')) else {
            return Err(format!("expected a type name, found {}", self.peek().map(|s| format!("`{s}`")).unwrap_or("the end".into())));
        };
        let name = name.to_string();
        self.pos += 1;
        if !self.eat("<") {
            return Ok(TypeSpec::from_name(&name));
        }
        let ps = self.list(">")?;
        let n = ps.len();
        let one = |ps: Vec<TypeSpec>| Box::new(ps.into_iter().next().unwrap());
        let arity = |want: usize| if n == want { Ok(()) } else { Err(format!("`{name}` takes {want} type parameter(s), got {n}")) };
        Ok(match name.as_str() {
            "list" => {
                arity(1)?;
                T::List(one(ps))
            }
            "array" => {
                arity(1)?;
                T::Array(one(ps))
            }
            "optional" | "Option" => {
                arity(1)?;
                T::Optional(one(ps))
            }
            "map" => {
                arity(2)?;
                let mut it = ps.into_iter();
                T::Map(Box::new(it.next().unwrap()), Box::new(it.next().unwrap()))
            }
            "result" | "Result" => {
                arity(2)?;
                let mut it = ps.into_iter();
                T::Result(Box::new(it.next().unwrap()), Box::new(it.next().unwrap()))
            }
            "tuple" => T::Tuple(ps),
            _ => return Err(format!("`{name}` doesn't take type parameters")),
        })
    }
}

// ── The script-visible type object ──

/// A type as a value: what `type_of(x)`, `Type("list<int>")` and the
/// global type names (`text`, `number`, …) evaluate to.
pub struct TypeObj(pub TypeSpec);

pub fn type_value(t: TypeSpec) -> Value {
    Value::Object(Rc::new(TypeObj(t)))
}

impl HostObject for TypeObj {
    fn type_name(&self) -> &str {
        "type"
    }
    fn field(&self, name: &str) -> Option<Value> {
        match name {
            "name" => Some(Value::str(&self.0.to_string())),
            "base" => Some(Value::str(&self.0.base())),
            "params" => Some(Value::list(self.0.params().into_iter().map(type_value).collect())),
            _ => None,
        }
    }
    fn call(&self, it: &mut Interp, method: &str, args: Vec<Value>) -> Result<Value, String> {
        let v = args.first().cloned().unwrap_or(Value::Null);
        match method {
            "accepts" | "check" => Ok(Value::Bool(self.0.check(&v, Some(it)))),
            "convert" => self.0.convert(&v, Some(it)),
            "try_convert" => Ok(result(self.0.convert(&v, Some(it)))),
            "to_string" | "str" => Ok(Value::str(&self.0.to_string())),
            _ => Err(format!("type has no method `{method}`")),
        }
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn binary(&self, it: &mut Interp, op: &str, other: &Value, _swapped: bool) -> Option<Result<Value, String>> {
        let same = match other {
            Value::Object(o) => o.as_any().downcast_ref::<TypeObj>().map(|t| t.0 == self.0),
            // `type_of(x) == "string"`: compare with a type written as text.
            Value::Str(s) => Some(TypeSpec::parse(s).is_ok_and(|t| t == self.0)),
            Value::Native(_) | Value::Null => to_spec(it, other).ok().map(|t| t == self.0),
            _ => Some(false),
        }?;
        match op {
            "==" => Some(Ok(Value::Bool(same))),
            "!=" => Some(Ok(Value::Bool(!same))),
            _ => None,
        }
    }
    fn show(&self) -> Option<String> {
        Some(self.0.to_string())
    }
}

/// `Ok(v)` / `Err(message)` as the language's Result values.
pub fn result(r: Result<Value, String>) -> Value {
    let (name, field, v) = match r {
        Ok(v) => ("Ok", "value", v),
        Err(e) => ("Err", "error", Value::str(&e)),
    };
    Value::Struct(Rc::new(std::cell::RefCell::new(crate::value::StructVal { name: name.into(), fields: vec![(field.into(), v)] })))
}

/// Reads the type argument of `is`/`as`/`try_as`: a type object, a type
/// written as text, a conversion function (`int`, `str`, `list`…), a struct
/// name, or `null`.
pub fn to_spec(it: &Interp, v: &Value) -> Result<TypeSpec, String> {
    match v {
        Value::Object(o) => match o.as_any().downcast_ref::<TypeObj>() {
            Some(t) => Ok(t.0.clone()),
            None => Err(format!("expected a type, got {}", o.type_name())),
        },
        Value::Str(s) => TypeSpec::parse(s),
        Value::Null => Ok(T::Null),
        Value::Native(n) => Ok(match n.name.as_str() {
            "int" | "to_int" => T::Int,
            "float" | "to_float" => T::Float,
            "str" | "to_string" => T::Text,
            "bool" => T::Bool,
            "list" => T::List(Box::new(T::Any)),
            "map" => T::Map(Box::new(T::Text), Box::new(T::Any)),
            "any" => T::Any,
            "Some" => T::Optional(Box::new(T::Any)),
            "Ok" | "Err" => T::Result(Box::new(T::Any), Box::new(T::Any)),
            "rgb" | "rgba" => T::Color,
            "range" => T::Range,
            n if it.structs.contains_key(n) || it.enums.contains_key(n) => T::Named(n.into()),
            n => return Err(format!("`{n}` isn't a type")),
        }),
        // `(int, text)` in an expression is a tuple of types.
        Value::List(items) => Ok(T::Tuple(items.borrow().iter().map(|x| to_spec(it, x)).collect::<Result<_, _>>()?)),
        // An enum's name evaluates to the namespace of its variants.
        Value::Map(m) => it
            .enums
            .keys()
            .find(|e| matches!(it.global(e), Some(Value::Map(g)) if Rc::ptr_eq(&g, m)))
            .map(|e| T::Named(e.clone()))
            .ok_or_else(|| "expected a type, got map".to_string()),
        other => Err(format!("expected a type, got {}", other.type_name())),
    }
}

/// Type names that are values in every program: `is(x, text)`.
pub const TYPE_GLOBALS: &[&str] = &["text", "number", "byte", "char", "dynamic", "void"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs_parse_and_print_canonically() {
        for (src, want) in [
            ("int", "int"),
            ("string", "text"),
            ("I64", "int"),
            ("u8", "byte"),
            ("U0", "void"),
            ("list", "list"),
            ("list<any>", "list"),
            ("[int]", "list<int>"),
            ("list<list<text>>", "list<list<text>>"),
            ("map<text,list<int>>", "map<text, list<int>>"),
            ("map<string, any>", "map"),
            ("(int, str)", "(int, text)"),
            ("tuple<int, int>", "(int, int)"),
            ("int?", "optional<int>"),
            ("Option<float>", "optional<float>"),
            ("result<int, text>", "result<int, text>"),
            ("Point", "Point"),
        ] {
            assert_eq!(TypeSpec::parse(src).map(|t| t.to_string()), Ok(want.to_string()), "{src}");
        }
        for bad in ["", "list<", "map<int>", "int<text>", "list<int>>", "a b", "list<int,>", "$"] {
            assert!(TypeSpec::parse(bad).is_err(), "{bad} should not parse");
        }
    }

    #[test]
    fn checks_and_conversions() {
        let p = |s: &str| TypeSpec::parse(s).unwrap();
        let ints = Value::list(vec![Value::Int(1), Value::Int(2)]);
        assert!(p("list<int>").check(&ints, None));
        assert!(p("list<number>").check(&ints, None));
        assert!(!p("list<float>").check(&ints, None));
        assert!(p("list<float>").accepts(&ints, None));
        assert!(p("int?").check(&Value::Null, None));
        assert!(!p("byte").check(&Value::Int(256), None));
        assert!(p("char").check(&Value::str("é"), None));
        assert_eq!(p("int").convert(&Value::str(" 12 "), None).unwrap().to_string(), "12");
        assert!(p("int").convert(&Value::Float(2.5), None).is_err());
        assert_eq!(p("int").convert(&Value::Float(2.0), None).unwrap().to_string(), "2");
        assert!(p("byte").convert(&Value::Int(300), None).is_err());
        assert_eq!(p("list<float>").convert(&ints, None).unwrap().to_string(), "[1.0, 2.0]");
        assert_eq!(p("color").convert(&Value::str("#ff0000"), None).unwrap().to_string(), "#ff0000");
        assert_eq!(p("bool").convert(&Value::str("no"), None).unwrap().to_string(), "false");
        assert!(p("bool").convert(&Value::str("maybe"), None).is_err());
    }
}
