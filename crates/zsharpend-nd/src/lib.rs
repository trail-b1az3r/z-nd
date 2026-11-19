//! **nd** — n-dimensional arrays of numbers for Zsharpend, in the style of
//! NumPy: creation (`array`, `zeros`, `arange`, `linspace`, `eye`,
//! `random`…), element-wise maths with broadcasting, comparisons that give
//! masks, slicing and masked selection, reductions along an axis, `@` for
//! matrix multiplication, reshaping and transposing.
//!
//! ```text
//! import nd
//! let a = nd.arange(6).reshape(2, 3)
//! let b = a * 2 + 1            // broadcasting a scalar
//! Printl(b.sum(axis=0), a @ a.T)
//! ```
//!
//! It's a script extender: hosts list [`Nd`] to make `import nd` work.

use std::any::Any;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use zsharpend::extend::{namespace, Extender};
use zsharpend::value::{slice_indices, HostObject, Index};
use zsharpend::{Interp, Value};

/// Largest array, in elements (the sandbox's list limit applies too).
const MAX_ELEMENTS: usize = 10_000_000;

/// A row-major array of `f64`.
pub struct NdArray {
    pub shape: Vec<usize>,
    pub data: RefCell<Vec<f64>>,
}

type R<T> = Result<T, String>;

fn size_of(shape: &[usize]) -> usize {
    shape.iter().product()
}

fn strides(shape: &[usize]) -> Vec<usize> {
    let mut s = vec![1; shape.len()];
    for i in (0..shape.len().saturating_sub(1)).rev() {
        s[i] = s[i + 1] * shape[i + 1];
    }
    s
}

impl NdArray {
    pub fn new(shape: Vec<usize>, data: Vec<f64>) -> R<NdArray> {
        if size_of(&shape) != data.len() {
            return Err(format!("{} values don't fill shape {}", data.len(), shape_str(&shape)));
        }
        if data.len() > MAX_ELEMENTS {
            return Err(format!("an array of {} elements is too large (limit {MAX_ELEMENTS})", data.len()));
        }
        Ok(NdArray { shape, data: RefCell::new(data) })
    }

    pub fn value(self) -> Value {
        Value::Object(Rc::new(self))
    }

    fn filled(shape: Vec<usize>, x: f64) -> R<NdArray> {
        let n = size_of(&shape);
        if n > MAX_ELEMENTS {
            return Err(format!("an array of {n} elements is too large (limit {MAX_ELEMENTS})"));
        }
        NdArray::new(shape, vec![x; n])
    }

    fn map(&self, f: impl Fn(f64) -> f64) -> NdArray {
        NdArray { shape: self.shape.clone(), data: RefCell::new(self.data.borrow().iter().map(|x| f(*x)).collect()) }
    }

    /// Reduce along `axis` (or everything when `None`).
    fn reduce(&self, axis: Option<i64>, f: &dyn Fn(&[f64]) -> f64) -> R<Value> {
        let data = self.data.borrow();
        let Some(ax) = axis else { return Ok(Value::Float(f(&data))) };
        let nd = self.shape.len() as i64;
        let ax = if ax < 0 { ax + nd } else { ax };
        if ax < 0 || ax >= nd {
            return Err(format!("axis {ax} is out of range for a {nd}-d array"));
        }
        let ax = ax as usize;
        let mut out_shape = self.shape.clone();
        let n = out_shape.remove(ax);
        let st = strides(&self.shape);
        let outer: usize = self.shape[..ax].iter().product();
        let inner: usize = self.shape[ax + 1..].iter().product();
        let mut out = Vec::with_capacity(outer * inner);
        let mut lane = vec![0.0; n];
        for o in 0..outer {
            for i in 0..inner {
                for (k, slot) in lane.iter_mut().enumerate() {
                    *slot = data[o * st[ax] * n + k * st[ax] + i];
                }
                out.push(f(&lane));
            }
        }
        if out_shape.is_empty() {
            return Ok(Value::Float(out[0]));
        }
        Ok(NdArray::new(out_shape, out)?.value())
    }

    fn tolist(&self) -> Value {
        fn build(shape: &[usize], data: &[f64]) -> Value {
            if shape.is_empty() {
                return Value::Float(data[0]);
            }
            if shape.len() == 1 {
                return Value::list(data.iter().map(|x| num(*x)).collect());
            }
            let step = size_of(&shape[1..]);
            Value::list((0..shape[0]).map(|i| build(&shape[1..], &data[i * step..(i + 1) * step])).collect())
        }
        build(&self.shape, &self.data.borrow())
    }

    fn transpose(&self) -> NdArray {
        let nd = self.shape.len();
        if nd < 2 {
            return NdArray { shape: self.shape.clone(), data: RefCell::new(self.data.borrow().clone()) };
        }
        let shape: Vec<usize> = self.shape.iter().rev().cloned().collect();
        let src_st = strides(&self.shape);
        let n = size_of(&shape);
        let data = self.data.borrow();
        let mut out = Vec::with_capacity(n);
        let mut idx = vec![0usize; nd];
        for _ in 0..n {
            // idx is an index into the transposed array; reversed, it's the source index.
            let off: usize = idx.iter().rev().zip(src_st.iter()).map(|(i, s)| i * s).sum();
            out.push(data[off]);
            for d in (0..nd).rev() {
                idx[d] += 1;
                if idx[d] < shape[d] {
                    break;
                }
                idx[d] = 0;
            }
        }
        NdArray { shape, data: RefCell::new(out) }
    }
}

/// Whole numbers come back to scripts as ints.
fn num(x: f64) -> Value {
    Value::Float(x)
}

fn shape_str(s: &[usize]) -> String {
    format!("({})", s.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ") + if s.len() == 1 { "," } else { "" })
}

fn fmt_num(x: f64) -> String {
    if x.is_nan() {
        "nan".into()
    } else if x.is_infinite() {
        if x > 0.0 { "inf".into() } else { "-inf".into() }
    } else if x.fract() == 0.0 && x.abs() < 1e16 {
        format!("{}.", x as i64)
    } else {
        let s = format!("{x:.6}");
        let s = s.trim_end_matches('0');
        s.to_string()
    }
}

impl HostObject for NdArray {
    fn type_name(&self) -> &str {
        "ndarray"
    }

    fn show(&self) -> Option<String> {
        fn rec(shape: &[usize], data: &[f64], depth: usize) -> String {
            if shape.is_empty() {
                return fmt_num(data[0]);
            }
            if shape.len() == 1 {
                return format!("[{}]", data.iter().map(|x| fmt_num(*x)).collect::<Vec<_>>().join(", "));
            }
            let step = size_of(&shape[1..]);
            let rows: Vec<String> = (0..shape[0]).map(|i| rec(&shape[1..], &data[i * step..(i + 1) * step], depth + 1)).collect();
            let sep = format!(",\n{}", " ".repeat(7 + depth));
            format!("[{}]", rows.join(&sep))
        }
        Some(format!("array({})", rec(&self.shape, &self.data.borrow(), 0)))
    }

    fn field(&self, name: &str) -> Option<Value> {
        Some(match name {
            "shape" => Value::list(self.shape.iter().map(|x| Value::Int(*x as i64)).collect()),
            "ndim" => Value::Int(self.shape.len() as i64),
            "size" => Value::Int(size_of(&self.shape) as i64),
            "T" => self.transpose().value(),
            _ => return None,
        })
    }

    fn call(&self, it: &mut Interp, m: &str, args: Vec<Value>) -> R<Value> {
        let (args, kw) = split_kwargs(args);
        let axis = || -> R<Option<i64>> {
            match kw.get("axis").or(args.first()) {
                None | Some(Value::Null) => Ok(None),
                Some(v) => v.as_i64().map(Some).ok_or("axis: a whole number".into()),
            }
        };
        Ok(match m {
            "tolist" => self.tolist(),
            "copy" => NdArray { shape: self.shape.clone(), data: RefCell::new(self.data.borrow().clone()) }.value(),
            "reshape" => reshape(self, &args)?.value(),
            "flatten" | "ravel" => NdArray::new(vec![size_of(&self.shape)], self.data.borrow().clone())?.value(),
            "transpose" => self.transpose().value(),
            "sum" => self.reduce(axis()?, &|v| v.iter().sum())?,
            "prod" => self.reduce(axis()?, &|v| v.iter().product())?,
            "mean" => self.reduce(axis()?, &|v| v.iter().sum::<f64>() / v.len().max(1) as f64)?,
            "min" => self.reduce(axis()?, &|v| v.iter().cloned().fold(f64::INFINITY, f64::min))?,
            "max" => self.reduce(axis()?, &|v| v.iter().cloned().fold(f64::NEG_INFINITY, f64::max))?,
            "var" => self.reduce(axis()?, &var)?,
            "std" => self.reduce(axis()?, &|v| var(v).sqrt())?,
            "argmin" => self.reduce(axis()?, &|v| argbest(v, |a, b| a < b))?,
            "argmax" => self.reduce(axis()?, &|v| argbest(v, |a, b| a > b))?,
            "dot" => matmul(self, &arr(args.first().ok_or("dot: needs an array")?)?)?.value_or_scalar(),
            "clip" => {
                let (lo, hi) = (fnum(&args, 0, "clip")?, fnum(&args, 1, "clip")?);
                self.map(|x| x.clamp(lo, hi)).value()
            }
            "apply" => {
                // Run a script function on every element.
                let f = args.first().cloned().ok_or("apply: needs a function")?;
                let data = self.data.borrow().clone();
                let mut out = Vec::with_capacity(data.len());
                for x in data {
                    let v = it.call_value(&f, vec![Value::Float(x)], 0).map_err(|c| it.describe(c))?;
                    out.push(v.as_f64().ok_or("apply: the function must return a number")?);
                }
                NdArray::new(self.shape.clone(), out)?.value()
            }
            u if unary(u).is_some() => self.map(unary(u).unwrap()).value(),
            _ => return Err(format!("ndarray has no method `{m}`")),
        })
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn binary(&self, _it: &mut Interp, op: &str, other: &Value, swapped: bool) -> Option<R<Value>> {
        if op == "@" {
            return Some((|| {
                let o = arr(other)?;
                let r = if swapped { matmul(&o, self)? } else { matmul(self, &o)? };
                Ok(r.value_or_scalar())
            })());
        }
        let f: fn(f64, f64) -> f64 = match op {
            "+" => |a, b| a + b,
            "-" => |a, b| a - b,
            "*" => |a, b| a * b,
            "/" => |a, b| a / b,
            "%" => |a, b| a.rem_euclid(b),
            "**" => f64::powf,
            "<" => |a, b| (a < b) as u8 as f64,
            ">" => |a, b| (a > b) as u8 as f64,
            "<=" => |a, b| (a <= b) as u8 as f64,
            ">=" => |a, b| (a >= b) as u8 as f64,
            "==" => |a, b| (a == b) as u8 as f64,
            "!=" => |a, b| (a != b) as u8 as f64,
            _ => return None,
        };
        Some((|| {
            let o = arr(other)?;
            let (a, b) = if swapped { (&o, self) } else { (self, &o) };
            Ok(broadcast(a, b, f)?.value())
        })())
    }

    fn negate(&self, _it: &mut Interp) -> Option<R<Value>> {
        Some(Ok(self.map(|x| -x).value()))
    }

    fn index(&self, _it: &mut Interp, parts: &[Index]) -> Option<R<Value>> {
        Some((|| {
            // A mask (an array of 0/1 the same shape) selects elements.
            if let [Index::At(Value::Object(o))] = parts {
                if let Some(mask) = o.as_any().downcast_ref::<NdArray>() {
                    if mask.shape != self.shape {
                        return Err(format!("a mask of shape {} doesn't fit an array of shape {}", shape_str(&mask.shape), shape_str(&self.shape)));
                    }
                    let d = self.data.borrow();
                    let picked: Vec<f64> = d.iter().zip(mask.data.borrow().iter()).filter(|(_, m)| **m != 0.0).map(|(x, _)| *x).collect();
                    let n = picked.len();
                    return Ok(NdArray::new(vec![n], picked)?.value());
                }
            }
            let (shape, offsets) = select(&self.shape, parts)?;
            let d = self.data.borrow();
            let vals: Vec<f64> = offsets.iter().map(|o| d[*o]).collect();
            if shape.is_empty() {
                return Ok(Value::Float(vals[0]));
            }
            Ok(NdArray::new(shape, vals)?.value())
        })())
    }

    fn set_index(&self, _it: &mut Interp, parts: &[Index], value: Value) -> Option<R<()>> {
        Some((|| {
            let offsets: Vec<usize> = match parts {
                [Index::At(Value::Object(o))] if o.as_any().downcast_ref::<NdArray>().is_some() => {
                    let mask = o.as_any().downcast_ref::<NdArray>().unwrap();
                    if mask.shape != self.shape {
                        return Err("the mask's shape doesn't match".into());
                    }
                    mask.data.borrow().iter().enumerate().filter(|(_, m)| **m != 0.0).map(|(i, _)| i).collect()
                }
                _ => select(&self.shape, parts)?.1,
            };
            let src: Vec<f64> = match &value {
                Value::Object(_) | Value::List(_) => {
                    let a = arr(&value)?;
                    let v = a.data.borrow().clone();
                    if v.len() == 1 {
                        vec![v[0]; offsets.len()]
                    } else if v.len() == offsets.len() {
                        v
                    } else {
                        return Err(format!("can't put {} values into {} places", v.len(), offsets.len()));
                    }
                }
                v => vec![v.as_f64().ok_or(format!("can't store {} in an array", v.type_name()))?; offsets.len()],
            };
            let mut d = self.data.borrow_mut();
            for (o, x) in offsets.into_iter().zip(src) {
                d[o] = x;
            }
            Ok(())
        })())
    }
}

trait ValueOrScalar {
    fn value_or_scalar(self) -> Value;
}
impl ValueOrScalar for NdArray {
    fn value_or_scalar(self) -> Value {
        if self.shape.is_empty() {
            Value::Float(self.data.borrow()[0])
        } else {
            self.value()
        }
    }
}

fn var(v: &[f64]) -> f64 {
    let n = v.len().max(1) as f64;
    let m = v.iter().sum::<f64>() / n;
    v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / n
}

fn argbest(v: &[f64], better: fn(f64, f64) -> bool) -> f64 {
    let mut b = 0;
    for (i, x) in v.iter().enumerate() {
        if better(*x, v[b]) {
            b = i;
        }
    }
    b as f64
}

fn unary(name: &str) -> Option<fn(f64) -> f64> {
    Some(match name {
        "sqrt" => f64::sqrt,
        "exp" => f64::exp,
        "log" => f64::ln,
        "log10" => f64::log10,
        "sin" => f64::sin,
        "cos" => f64::cos,
        "tan" => f64::tan,
        "tanh" => f64::tanh,
        "abs" => f64::abs,
        "floor" => f64::floor,
        "ceil" => f64::ceil,
        "round" => f64::round,
        "sign" => |x| if x > 0.0 { 1.0 } else if x < 0.0 { -1.0 } else { 0.0 },
        "sigmoid" => |x| 1.0 / (1.0 + (-x).exp()),
        "relu" => |x| x.max(0.0),
        _ => return None,
    })
}

/// Split a trailing keyword-argument map (`f(x, axis=0)`) off the arguments.
fn split_kwargs(mut args: Vec<Value>) -> (Vec<Value>, BTreeMap<String, Value>) {
    if let Some(Value::Map(m)) = args.last() {
        let m = m.borrow().clone();
        args.pop();
        return (args, m);
    }
    (args, BTreeMap::new())
}

fn fnum(a: &[Value], i: usize, f: &str) -> R<f64> {
    a.get(i).and_then(Value::as_f64).ok_or(format!("{f}: argument {} must be a number", i + 1))
}

/// A shape from `3`, `(2, 3)`, `[2, 3]` or several numbers.
fn shape_arg(args: &[Value], what: &str) -> R<Vec<usize>> {
    let dims: Vec<Value> = match args {
        [Value::List(l)] => l.borrow().clone(),
        [Value::Object(o)] => match o.field("shape") {
            Some(Value::List(l)) => l.borrow().clone(),
            _ => return Err(format!("{what}: a shape is a number or a list of numbers")),
        },
        other => other.to_vec(),
    };
    if dims.is_empty() {
        return Err(format!("{what}: needs a shape"));
    }
    dims.iter().map(|d| d.as_i64().filter(|x| *x >= 0).map(|x| x as usize).ok_or(format!("{what}: a shape is made of whole numbers ≥ 0"))).collect()
}

/// Anything array-like: an ndarray, a number, or nested lists.
pub fn arr(v: &Value) -> R<NdArray> {
    match v {
        Value::Object(o) => match o.as_any().downcast_ref::<NdArray>() {
            Some(a) => Ok(NdArray { shape: a.shape.clone(), data: RefCell::new(a.data.borrow().clone()) }),
            None => Err(format!("expected an array, got {}", o.type_name())),
        },
        Value::List(_) => {
            let mut shape = vec![];
            let mut data = vec![];
            nested(v, 0, &mut shape, &mut data)?;
            NdArray::new(shape, data)
        }
        x => match x.as_f64() {
            Some(f) => Ok(NdArray { shape: vec![], data: RefCell::new(vec![f]) }),
            None => Err(format!("expected numbers, got {}", x.type_name())),
        },
    }
}

fn nested(v: &Value, depth: usize, shape: &mut Vec<usize>, data: &mut Vec<f64>) -> R<()> {
    match v {
        Value::List(l) => {
            let l = l.borrow();
            if shape.len() == depth {
                shape.push(l.len());
            } else if shape[depth] != l.len() {
                return Err("rows have different lengths (arrays are rectangular)".into());
            }
            for x in l.iter() {
                nested(x, depth + 1, shape, data)?;
            }
            Ok(())
        }
        x => {
            if shape.len() != depth {
                return Err("rows have different depths".into());
            }
            data.push(x.as_f64().ok_or(format!("arrays hold numbers, not {}", x.type_name()))?);
            Ok(())
        }
    }
}

/// NumPy broadcasting: shapes align from the right; a dimension of 1 (or a
/// missing one) stretches to match.
fn broadcast(a: &NdArray, b: &NdArray, f: impl Fn(f64, f64) -> f64) -> R<NdArray> {
    let nd = a.shape.len().max(b.shape.len());
    let pad = |s: &[usize]| -> Vec<usize> { std::iter::repeat_n(1, nd - s.len()).chain(s.iter().cloned()).collect() };
    let (sa, sb) = (pad(&a.shape), pad(&b.shape));
    let mut shape = vec![0; nd];
    for i in 0..nd {
        shape[i] = match (sa[i], sb[i]) {
            (x, y) if x == y => x,
            (1, y) => y,
            (x, 1) => x,
            _ => return Err(format!("shapes {} and {} can't be broadcast together", shape_str(&a.shape), shape_str(&b.shape))),
        };
    }
    let (sta, stb) = (strides(&sa), strides(&sb));
    let (da, db) = (a.data.borrow(), b.data.borrow());
    let n = size_of(&shape);
    if n > MAX_ELEMENTS {
        return Err(format!("the result would have {n} elements (limit {MAX_ELEMENTS})"));
    }
    let mut out = Vec::with_capacity(n);
    let mut idx = vec![0usize; nd];
    for _ in 0..n {
        let (mut oa, mut ob) = (0, 0);
        for d in 0..nd {
            if sa[d] != 1 {
                oa += idx[d] * sta[d];
            }
            if sb[d] != 1 {
                ob += idx[d] * stb[d];
            }
        }
        out.push(f(da[oa], db[ob]));
        for d in (0..nd).rev() {
            idx[d] += 1;
            if idx[d] < shape[d] {
                break;
            }
            idx[d] = 0;
        }
    }
    NdArray::new(shape, out)
}

fn matmul(a: &NdArray, b: &NdArray) -> R<NdArray> {
    let (da, db) = (a.data.borrow(), b.data.borrow());
    match (a.shape.as_slice(), b.shape.as_slice()) {
        ([n], [m]) if n == m => NdArray::new(vec![], vec![da.iter().zip(db.iter()).map(|(x, y)| x * y).sum()]),
        ([r, k], [k2, c]) if k == k2 => {
            let mut out = vec![0.0; r * c];
            for i in 0..*r {
                for t in 0..*k {
                    let x = da[i * k + t];
                    for j in 0..*c {
                        out[i * c + j] += x * db[t * c + j];
                    }
                }
            }
            NdArray::new(vec![*r, *c], out)
        }
        ([r, k], [k2]) if k == k2 => NdArray::new(vec![*r], (0..*r).map(|i| (0..*k).map(|t| da[i * k + t] * db[t]).sum()).collect()),
        ([k], [k2, c]) if k == k2 => NdArray::new(vec![*c], (0..*c).map(|j| (0..*k).map(|t| da[t] * db[t * c + j]).sum()).collect()),
        _ => Err(format!("can't multiply shapes {} and {} (inner sizes must match)", shape_str(&a.shape), shape_str(&b.shape))),
    }
}

fn reshape(a: &NdArray, args: &[Value]) -> R<NdArray> {
    let raw: Vec<i64> = match args {
        [Value::List(l)] => l.borrow().iter().map(|v| v.as_i64().unwrap_or(0)).collect(),
        other => other.iter().map(|v| v.as_i64().unwrap_or(0)).collect(),
    };
    let n = size_of(&a.shape);
    let known: i64 = raw.iter().filter(|x| **x != -1).product();
    if raw.iter().filter(|x| **x == -1).count() > 1 {
        return Err("reshape: only one dimension can be -1".into());
    }
    let shape: Vec<usize> = raw.iter().map(|x| if *x == -1 { if known == 0 { 0 } else { n / known as usize } } else { *x as usize }).collect();
    if size_of(&shape) != n {
        return Err(format!("can't reshape {} values into {}", n, shape_str(&shape)));
    }
    NdArray::new(shape, a.data.borrow().clone())
}

/// Resolve subscript parts against `shape`: the result's shape and the
/// flat offsets it reads, in order. An index drops its axis; a slice keeps it.
fn select(shape: &[usize], parts: &[Index]) -> R<(Vec<usize>, Vec<usize>)> {
    if parts.len() > shape.len() {
        return Err(format!("{} indices for a {}-d array", parts.len(), shape.len()));
    }
    let st = strides(shape);
    let mut per_axis: Vec<(Vec<usize>, bool)> = vec![];
    for (d, &n) in shape.iter().enumerate() {
        per_axis.push(match parts.get(d) {
            None => ((0..n).collect(), true),
            Some(Index::At(v)) => {
                let i = v.as_i64().ok_or("an index must be a whole number")?;
                let k = if i < 0 { i + n as i64 } else { i };
                if k < 0 || k >= n as i64 {
                    return Err(format!("index {i} is out of range for an axis of size {n}"));
                }
                (vec![k as usize], false)
            }
            Some(Index::Slice(a, b, c)) => (slice_indices(n, *a, *b, *c)?, true),
        });
    }
    let out_shape: Vec<usize> = per_axis.iter().filter(|(_, keep)| *keep).map(|(v, _)| v.len()).collect();
    let mut offsets = vec![0usize];
    for (d, (ix, _)) in per_axis.iter().enumerate() {
        let mut next = Vec::with_capacity(offsets.len() * ix.len());
        for o in &offsets {
            for i in ix {
                next.push(o + i * st[d]);
            }
        }
        offsets = next;
    }
    Ok((out_shape, offsets))
}

fn native(name: &'static str, f: impl Fn(&mut Interp, Vec<Value>) -> R<Value> + 'static) -> (&'static str, Value) {
    (name, Value::native(&format!("nd.{name}"), f))
}

/// The `nd` module.
pub fn module() -> Value {
    let mut items = vec![
        native("array", |_, a| Ok(arr(a.first().ok_or("array: needs a list of numbers")?)?.value_or_scalar())),
        native("zeros", |_, a| Ok(NdArray::filled(shape_arg(&split_kwargs(a).0, "zeros")?, 0.0)?.value())),
        native("ones", |_, a| Ok(NdArray::filled(shape_arg(&split_kwargs(a).0, "ones")?, 1.0)?.value())),
        native("full", |_, a| {
            let x = a.last().and_then(Value::as_f64).ok_or("full: the last argument is the value")?;
            Ok(NdArray::filled(shape_arg(&a[..a.len() - 1], "full")?, x)?.value())
        }),
        native("arange", |_, a| {
            let (start, stop, step) = match a.len() {
                1 => (0.0, fnum(&a, 0, "arange")?, 1.0),
                2 => (fnum(&a, 0, "arange")?, fnum(&a, 1, "arange")?, 1.0),
                _ => (fnum(&a, 0, "arange")?, fnum(&a, 1, "arange")?, fnum(&a, 2, "arange")?),
            };
            if step == 0.0 {
                return Err("arange: the step can't be zero".into());
            }
            let n = ((stop - start) / step).ceil().max(0.0) as usize;
            if n > MAX_ELEMENTS {
                return Err("arange: too many values".into());
            }
            Ok(NdArray::new(vec![n], (0..n).map(|i| start + i as f64 * step).collect())?.value())
        }),
        native("linspace", |_, a| {
            let (lo, hi) = (fnum(&a, 0, "linspace")?, fnum(&a, 1, "linspace")?);
            let n = a.get(2).and_then(Value::as_i64).unwrap_or(50).max(0) as usize;
            let step = if n > 1 { (hi - lo) / (n - 1) as f64 } else { 0.0 };
            Ok(NdArray::new(vec![n], (0..n).map(|i| lo + i as f64 * step).collect())?.value())
        }),
        native("eye", |_, a| {
            let n = a.first().and_then(Value::as_i64).ok_or("eye: needs a size")?.max(0) as usize;
            let mut d = vec![0.0; n * n];
            for i in 0..n {
                d[i * n + i] = 1.0;
            }
            Ok(NdArray::new(vec![n, n], d)?.value())
        }),
        native("random", |it, a| {
            let shape = shape_arg(&split_kwargs(a).0, "random")?;
            let n = size_of(&shape);
            if n > MAX_ELEMENTS {
                return Err("random: too many values".into());
            }
            Ok(NdArray::new(shape, (0..n).map(|_| it.random()).collect())?.value())
        }),
        native("seed", |it, a| {
            it.seed(a.first().and_then(Value::as_i64).unwrap_or(1) as u64);
            Ok(Value::Null)
        }),
        native("dot", |_, a| Ok(matmul(&arr(&a[0])?, &arr(a.get(1).ok_or("dot: needs two arrays")?)?)?.value_or_scalar())),
        native("matmul", |_, a| Ok(matmul(&arr(&a[0])?, &arr(a.get(1).ok_or("matmul: needs two arrays")?)?)?.value_or_scalar())),
        native("transpose", |_, a| Ok(arr(a.first().ok_or("transpose: needs an array")?)?.transpose().value())),
        native("reshape", |_, a| Ok(reshape(&arr(a.first().ok_or("reshape: needs an array")?)?, &a[1..])?.value())),
        native("where", |_, a| {
            let (c, x, y) = (arr(&a[0])?, arr(a.get(1).ok_or("where: cond, x, y")?)?, arr(a.get(2).ok_or("where: cond, x, y")?)?);
            let cx = broadcast(&c, &x, |c, x| if c != 0.0 { x } else { f64::NAN })?;
            let r = broadcast(&cx, &y, |v, y| if v.is_nan() { y } else { v })?;
            Ok(r.value())
        }),
        native("maximum", |_, a| Ok(broadcast(&arr(&a[0])?, &arr(a.get(1).ok_or("maximum: two arrays")?)?, f64::max)?.value())),
        native("minimum", |_, a| Ok(broadcast(&arr(&a[0])?, &arr(a.get(1).ok_or("minimum: two arrays")?)?, f64::min)?.value())),
        native("concatenate", |_, a| {
            let (a, kw) = split_kwargs(a);
            let Some(Value::List(l)) = a.first() else { return Err("concatenate: needs a list of arrays".into()) };
            let parts: Vec<NdArray> = l.borrow().iter().map(arr).collect::<R<_>>()?;
            let axis = kw.get("axis").or(a.get(1)).and_then(Value::as_i64).unwrap_or(0) as usize;
            concat(&parts, axis)
        }),
        native("stack", |_, a| {
            let Some(Value::List(l)) = a.first() else { return Err("stack: needs a list of arrays".into()) };
            let parts: Vec<NdArray> = l.borrow().iter().map(arr).collect::<R<_>>()?;
            let lifted: Vec<NdArray> = parts.into_iter().map(|p| NdArray { shape: std::iter::once(1).chain(p.shape.iter().cloned()).collect(), data: p.data }).collect();
            concat(&lifted, 0)
        }),
        native("allclose", |_, a| {
            let (x, y) = (arr(&a[0])?, arr(a.get(1).ok_or("allclose: two arrays")?)?);
            let tol = a.get(2).and_then(Value::as_f64).unwrap_or(1e-8);
            let d = broadcast(&x, &y, |p, q| ((p - q).abs() <= tol + 1e-5 * q.abs()) as u8 as f64)?;
            let all = d.data.borrow().iter().all(|v| *v == 1.0);
            Ok(Value::Bool(all))
        }),
        ("pi", Value::Float(std::f64::consts::PI)),
        ("e", Value::Float(std::f64::consts::E)),
    ];
    // Element-wise functions and reductions, also as module functions:
    // nd.sqrt(a), nd.sum(a, axis=0)…
    for name in [
        "sqrt", "exp", "log", "log10", "sin", "cos", "tan", "tanh", "abs", "floor", "ceil", "round", "sign", "sigmoid", "relu", "sum", "prod", "mean", "min", "max",
        "var", "std", "argmin", "argmax", "clip", "flatten",
    ] {
        items.push(native(name, move |it, a| {
            let first = a.first().ok_or(format!("nd.{name}: needs an array"))?;
            let x = arr(first)?.value();
            let Value::Object(o) = &x else { unreachable!() };
            if o.as_any().downcast_ref::<NdArray>().is_some_and(|a| a.shape.is_empty()) && unary(name).is_some() {
                return Ok(Value::Float(unary(name).unwrap()(first.as_f64().unwrap_or(0.0))));
            }
            o.call(it, name, a[1..].to_vec())
        }));
    }
    namespace(items)
}

fn concat(parts: &[NdArray], axis: usize) -> R<Value> {
    let first = parts.first().ok_or("nothing to join")?;
    if axis >= first.shape.len() {
        return Err(format!("axis {axis} is out of range"));
    }
    for p in parts {
        if p.shape.len() != first.shape.len() || p.shape.iter().enumerate().any(|(d, s)| d != axis && *s != first.shape[d]) {
            return Err("arrays to join must match except along the axis".into());
        }
    }
    let mut shape = first.shape.clone();
    shape[axis] = parts.iter().map(|p| p.shape[axis]).sum();
    let outer: usize = first.shape[..axis].iter().product();
    let mut out = vec![];
    for o in 0..outer {
        for p in parts {
            let chunk: usize = p.shape[axis..].iter().product();
            out.extend_from_slice(&p.data.borrow()[o * chunk..(o + 1) * chunk]);
        }
    }
    Ok(NdArray::new(shape, out)?.value())
}

/// The `nd` script extender.
pub struct Nd;

impl Extender for Nd {
    fn name(&self) -> &str {
        "nd"
    }
    fn about(&self) -> &str {
        "n-dimensional arrays (NumPy-style): broadcasting, slicing, reductions, @"
    }
    fn modules(&self, _it: &mut Interp) -> Vec<(String, Value)> {
        vec![("nd".into(), module())]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broadcasting_and_matmul() {
        let a = NdArray::new(vec![2, 3], (0..6).map(|x| x as f64).collect()).unwrap();
        let b = NdArray::new(vec![3], vec![10.0, 20.0, 30.0]).unwrap();
        let c = broadcast(&a, &b, |x, y| x + y).unwrap();
        assert_eq!(c.shape, vec![2, 3]);
        assert_eq!(*c.data.borrow(), vec![10.0, 21.0, 32.0, 13.0, 24.0, 35.0]);
        let m = matmul(&a, &a.transpose()).unwrap();
        assert_eq!(*m.data.borrow(), vec![5.0, 14.0, 14.0, 50.0]);
        assert!(broadcast(&a, &NdArray::new(vec![2], vec![1.0, 2.0]).unwrap(), |x, y| x + y).is_err());
    }

    #[test]
    fn select_slices() {
        let (shape, offs) = select(&[3, 4], &[Index::Slice(Some(1), None, None), Index::At(Value::Int(-1))]).unwrap();
        assert_eq!(shape, vec![2]);
        assert_eq!(offs, vec![7, 11]);
    }
}
