//! **gfx3d** — 3D for Zsharpend: meshes (cube, sphere, torus, cylinder,
//! plane, or your own vertices and faces), transforms, a perspective
//! camera, and a z-buffered software renderer that draws onto a `Canvas`
//! with flat Lambert lighting. No GPU needed, so it runs anywhere a script
//! does (and in tests).
//!
//! ```text
//! import gfx3d
//! let c = Canvas(400, 300)
//! let cube = gfx3d.cube(1.5).rotate(25, 35, 0).color(#2f7cf6)
//! let cam = gfx3d.camera([0, 1.5, 4], target=[0, 0, 0], fov=50)
//! gfx3d.render(c, [cube], cam, light=[-1, -2, -1])
//! ```

use std::any::Any;
use std::collections::BTreeMap;
use std::rc::Rc;

use zsharpend::canvas::Canvas;
use zsharpend::extend::{namespace, Extender};
use zsharpend::value::{color_parts, HostObject};
use zsharpend::{Interp, Value};

type R<T> = Result<T, String>;
type V3 = [f64; 3];

/// Most triangles one mesh may have (keeps scripts inside their budget).
const MAX_TRIS: usize = 500_000;

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn norm(a: V3) -> V3 {
    let l = dot(a, a).sqrt();
    if l == 0.0 {
        a
    } else {
        [a[0] / l, a[1] / l, a[2] / l]
    }
}

/// A triangle mesh with one colour.
#[derive(Clone)]
pub struct Mesh {
    pub verts: Vec<V3>,
    pub tris: Vec<[usize; 3]>,
    pub color: u32,
}

impl Mesh {
    fn value(self) -> Value {
        Value::Object(Rc::new(self))
    }
    fn map(&self, f: impl Fn(V3) -> V3) -> Mesh {
        Mesh { verts: self.verts.iter().map(|v| f(*v)).collect(), tris: self.tris.clone(), color: self.color }
    }
    fn center(&self) -> V3 {
        let n = self.verts.len().max(1) as f64;
        let s = self.verts.iter().fold([0.0; 3], |a, v| [a[0] + v[0], a[1] + v[1], a[2] + v[2]]);
        [s[0] / n, s[1] / n, s[2] / n]
    }
}

/// Rotation by `x`, `y`, `z` degrees (applied in that order) about the origin.
fn rotate(v: V3, deg: V3) -> V3 {
    let (mut x, mut y, mut z) = (v[0], v[1], v[2]);
    let [a, b, c] = deg.map(f64::to_radians);
    (y, z) = (y * a.cos() - z * a.sin(), y * a.sin() + z * a.cos());
    (x, z) = (x * b.cos() + z * b.sin(), -x * b.sin() + z * b.cos());
    (x, y) = (x * c.cos() - y * c.sin(), x * c.sin() + y * c.cos());
    [x, y, z]
}

fn v3(v: &Value, what: &str) -> R<V3> {
    match v {
        Value::List(l) => {
            let l = l.borrow();
            let g = |i: usize| l.get(i).and_then(Value::as_f64).ok_or(format!("{what}: a point is [x, y, z]"));
            Ok([g(0)?, g(1)?, g(2)?])
        }
        Value::Object(o) => match o.call_list() {
            Some(l) => v3(&l, what),
            None => Err(format!("{what}: a point is [x, y, z]")),
        },
        _ => Err(format!("{what}: a point is [x, y, z], got {}", v.type_name())),
    }
}

/// nd arrays and similar objects turn into lists through `tolist()`.
trait CallList {
    fn call_list(&self) -> Option<Value>;
}
impl CallList for Rc<dyn HostObject> {
    fn call_list(&self) -> Option<Value> {
        // `tolist` needs an interpreter in general; nd's doesn't use it.
        let mut it = Interp::new();
        self.call(&mut it, "tolist", vec![]).ok()
    }
}

fn split_kwargs(mut args: Vec<Value>) -> (Vec<Value>, BTreeMap<String, Value>) {
    if let Some(Value::Map(m)) = args.last() {
        let m = m.borrow().clone();
        args.pop();
        return (args, m);
    }
    (args, BTreeMap::new())
}

/// Argument `i`, or keyword `name`, or `default`.
fn num(args: &[Value], kw: &BTreeMap<String, Value>, i: usize, name: &str, default: f64) -> R<f64> {
    match kw.get(name).or(args.get(i)) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => v.as_f64().ok_or(format!("{name} must be a number")),
    }
}

impl HostObject for Mesh {
    fn type_name(&self) -> &str {
        "Mesh"
    }
    fn show(&self) -> Option<String> {
        Some(format!("Mesh({} vertices, {} triangles)", self.verts.len(), self.tris.len()))
    }
    fn field(&self, name: &str) -> Option<Value> {
        Some(match name {
            "vertices" => Value::list(self.verts.iter().map(|v| Value::list(v.iter().map(|x| Value::Float(*x)).collect())).collect()),
            "triangles" => Value::Int(self.tris.len() as i64),
            "colour" | "color_value" => Value::Color(self.color),
            "center" => Value::list(self.center().iter().map(|x| Value::Float(*x)).collect()),
            _ => return None,
        })
    }
    fn call(&self, _it: &mut Interp, m: &str, args: Vec<Value>) -> R<Value> {
        let (a, kw) = split_kwargs(args);
        Ok(match m {
            "rotate" => {
                let d = [num(&a, &kw, 0, "x", 0.0)?, num(&a, &kw, 1, "y", 0.0)?, num(&a, &kw, 2, "z", 0.0)?];
                self.map(|v| rotate(v, d)).value()
            }
            "translate" | "move" => {
                let d = [num(&a, &kw, 0, "x", 0.0)?, num(&a, &kw, 1, "y", 0.0)?, num(&a, &kw, 2, "z", 0.0)?];
                self.map(|v| [v[0] + d[0], v[1] + d[1], v[2] + d[2]]).value()
            }
            "scale" => {
                let sx = num(&a, &kw, 0, "x", 1.0)?;
                let (sy, sz) = if a.len() >= 3 { (num(&a, &kw, 1, "y", 1.0)?, num(&a, &kw, 2, "z", 1.0)?) } else { (sx, sx) };
                self.map(|v| [v[0] * sx, v[1] * sy, v[2] * sz]).value()
            }
            "color" | "colour" => match a.first() {
                Some(Value::Color(c)) => Mesh { color: *c, ..self.clone() }.value(),
                _ => return Err("color: needs a colour like #2f7cf6".into()),
            },
            "merge" => {
                let Some(Value::Object(o)) = a.first() else { return Err("merge: needs another mesh".into()) };
                let other = o.as_any().downcast_ref::<Mesh>().ok_or("merge: needs another mesh")?;
                let off = self.verts.len();
                let mut m = self.clone();
                m.verts.extend(other.verts.iter().cloned());
                m.tris.extend(other.tris.iter().map(|t| [t[0] + off, t[1] + off, t[2] + off]));
                m.value()
            }
            _ => return Err(format!("Mesh has no method `{m}` (rotate, translate, scale, color, merge)")),
        })
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// A perspective camera.
#[derive(Clone)]
pub struct Camera {
    pub position: V3,
    pub target: V3,
    pub fov: f64,
}

impl HostObject for Camera {
    fn type_name(&self) -> &str {
        "Camera"
    }
    fn field(&self, name: &str) -> Option<Value> {
        let l = |v: V3| Value::list(v.iter().map(|x| Value::Float(*x)).collect());
        Some(match name {
            "position" => l(self.position),
            "target" => l(self.target),
            "fov" => Value::Float(self.fov),
            _ => return None,
        })
    }
    fn call(&self, _it: &mut Interp, m: &str, args: Vec<Value>) -> R<Value> {
        match m {
            // Orbit around the target by `deg` degrees (about the vertical axis).
            "orbit" => {
                let deg = args.first().and_then(Value::as_f64).ok_or("orbit: needs degrees")?;
                let rel = rotate(sub(self.position, self.target), [0.0, deg, 0.0]);
                Ok(Value::Object(Rc::new(Camera { position: [self.target[0] + rel[0], self.target[1] + rel[1], self.target[2] + rel[2]], ..self.clone() })))
            }
            _ => Err(format!("Camera has no method `{m}` (orbit)")),
        }
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

struct View {
    eye: V3,
    r: V3,
    u: V3,
    f: V3,
    focal: f64,
    cx: f64,
    cy: f64,
}

const NEAR: f64 = 0.05;

impl View {
    fn new(cam: &Camera, w: u32, h: u32) -> View {
        let f = norm(sub(cam.target, cam.position));
        let mut r = norm(cross(f, [0.0, 1.0, 0.0]));
        if dot(r, r) < 1e-9 {
            r = [1.0, 0.0, 0.0]; // looking straight up or down
        }
        let u = cross(r, f);
        let focal = (h as f64 / 2.0) / (cam.fov.to_radians() / 2.0).tan();
        View { eye: cam.position, r, u, f, focal, cx: w as f64 / 2.0, cy: h as f64 / 2.0 }
    }
    /// Screen x, y and depth (distance along the view direction).
    fn project(&self, p: V3) -> Option<(f64, f64, f64)> {
        let d = sub(p, self.eye);
        let z = dot(d, self.f);
        if z <= NEAR {
            return None;
        }
        Some((self.cx + dot(d, self.r) / z * self.focal, self.cy - dot(d, self.u) / z * self.focal, z))
    }
}

/// Draw meshes onto the canvas. Returns how many triangles were drawn.
fn render(canvas: &Canvas, meshes: &[Mesh], cam: &Camera, light: V3, ambient: f64, wireframe: Option<u32>) -> R<i64> {
    let mut pix = canvas.pix.borrow_mut();
    let (w, h) = (pix.width(), pix.height());
    let view = View::new(cam, w, h);
    let to_light = norm([-light[0], -light[1], -light[2]]);
    let mut depth = vec![f64::INFINITY; (w * h) as usize];
    let data = pix.data_mut();
    let mut drawn = 0;
    for m in meshes {
        let (br, bg, bb, _) = color_parts(m.color);
        for t in &m.tris {
            let (a, b, c) = (m.verts[t[0]], m.verts[t[1]], m.verts[t[2]]);
            let (Some(pa), Some(pb), Some(pc)) = (view.project(a), view.project(b), view.project(c)) else { continue };
            // Two-sided: the normal is turned to face the camera.
            let mut n = norm(cross(sub(b, a), sub(c, a)));
            let mid = [(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0, (a[2] + b[2] + c[2]) / 3.0];
            if dot(n, sub(view.eye, mid)) < 0.0 {
                n = [-n[0], -n[1], -n[2]];
            }
            let lit = ambient + (1.0 - ambient) * dot(n, to_light).max(0.0);
            let shade = |ch: u8| (ch as f64 * lit).round().clamp(0.0, 255.0) as u8;
            let col = [shade(br), shade(bg), shade(bb)];
            let area = (pb.0 - pa.0) * (pc.1 - pa.1) - (pb.1 - pa.1) * (pc.0 - pa.0);
            if area.abs() < 1e-12 {
                continue;
            }
            let (x0, x1) = (pa.0.min(pb.0).min(pc.0).floor().max(0.0) as i64, pa.0.max(pb.0).max(pc.0).ceil().min(w as f64 - 1.0) as i64);
            let (y0, y1) = (pa.1.min(pb.1).min(pc.1).floor().max(0.0) as i64, pa.1.max(pb.1).max(pc.1).ceil().min(h as f64 - 1.0) as i64);
            let mut any = false;
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
                    let w0 = ((pb.0 - px) * (pc.1 - py) - (pb.1 - py) * (pc.0 - px)) / area;
                    let w1 = ((pc.0 - px) * (pa.1 - py) - (pc.1 - py) * (pa.0 - px)) / area;
                    let w2 = 1.0 - w0 - w1;
                    if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                        continue;
                    }
                    // Depth interpolates as 1/z across the screen.
                    let inv = w0 / pa.2 + w1 / pb.2 + w2 / pc.2;
                    let z = 1.0 / inv;
                    let i = (y as u32 * w + x as u32) as usize;
                    if z < depth[i] {
                        depth[i] = z;
                        let o = i * 4;
                        data[o] = col[0];
                        data[o + 1] = col[1];
                        data[o + 2] = col[2];
                        data[o + 3] = 255;
                        any = true;
                    }
                }
            }
            if any {
                drawn += 1;
            }
            if let Some(wc) = wireframe {
                let (r, g, b, _) = color_parts(wc);
                for (p, q) in [(pa, pb), (pb, pc), (pc, pa)] {
                    line(data, w, h, (p.0, p.1), (q.0, q.1), [r, g, b]);
                }
            }
        }
    }
    Ok(drawn)
}

fn line(data: &mut [u8], w: u32, h: u32, a: (f64, f64), b: (f64, f64), c: [u8; 3]) {
    let steps = (b.0 - a.0).abs().max((b.1 - a.1).abs()).ceil().max(1.0) as usize;
    for s in 0..=steps {
        let t = s as f64 / steps as f64;
        let (x, y) = ((a.0 + (b.0 - a.0) * t) as i64, (a.1 + (b.1 - a.1) * t) as i64);
        if x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h {
            let o = ((y as u32 * w + x as u32) * 4) as usize;
            data[o..o + 3].copy_from_slice(&c);
            data[o + 3] = 255;
        }
    }
}

// ── Shapes ─────────────────────────────────────────────────────────────

const DEFAULT: u32 = 0xb8c4d6ff;

fn cube(s: f64) -> Mesh {
    let h = s / 2.0;
    let mut verts = vec![];
    for &x in &[-h, h] {
        for &y in &[-h, h] {
            for &z in &[-h, h] {
                verts.push([x, y, z]);
            }
        }
    }
    let quads = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
    let tris = quads.iter().flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]]).collect();
    Mesh { verts, tris, color: DEFAULT }
}

/// A surface of revolution / parametric grid: `f(u, v)` for u, v in 0..=1.
fn grid(nu: usize, nv: usize, wrap_u: bool, f: impl Fn(f64, f64) -> V3) -> Mesh {
    let mut verts = vec![];
    for i in 0..=nv {
        for j in 0..=nu {
            verts.push(f(j as f64 / nu as f64, i as f64 / nv as f64));
        }
    }
    let row = nu + 1;
    let mut tris = vec![];
    for i in 0..nv {
        for j in 0..nu {
            let (a, b, c, d) = (i * row + j, i * row + j + 1, (i + 1) * row + j + 1, (i + 1) * row + j);
            tris.push([a, b, c]);
            tris.push([a, c, d]);
        }
    }
    let _ = wrap_u;
    Mesh { verts, tris, color: DEFAULT }
}

fn count(args: &[Value], kw: &BTreeMap<String, Value>, i: usize, name: &str, default: f64, lo: usize) -> R<usize> {
    let n = num(args, kw, i, name, default)?.round() as usize;
    Ok(n.clamp(lo, 512))
}

fn native(name: &'static str, f: impl Fn(&mut Interp, Vec<Value>) -> R<Value> + 'static) -> (&'static str, Value) {
    (name, Value::native(&format!("gfx3d.{name}"), f))
}

fn mesh_list(v: &Value) -> R<Vec<Mesh>> {
    let one = |v: &Value| -> R<Mesh> {
        match v {
            Value::Object(o) => o.as_any().downcast_ref::<Mesh>().cloned().ok_or(format!("expected a Mesh, got {}", o.type_name())),
            other => Err(format!("expected a Mesh, got {}", other.type_name())),
        }
    };
    match v {
        Value::List(l) => l.borrow().iter().map(one).collect(),
        v => Ok(vec![one(v)?]),
    }
}

/// The `gfx3d` module.
pub fn module() -> Value {
    use std::f64::consts::{PI, TAU};
    namespace(vec![
        native("cube", |_, a| {
            let (a, kw) = split_kwargs(a);
            Ok(cube(num(&a, &kw, 0, "size", 1.0)?).value())
        }),
        native("sphere", |_, a| {
            let (a, kw) = split_kwargs(a);
            let r = num(&a, &kw, 0, "radius", 1.0)?;
            let (rings, segs) = (count(&a, &kw, 1, "rings", 12.0, 3)?, count(&a, &kw, 2, "segments", 24.0, 3)?);
            Ok(grid(segs, rings, true, |u, v| {
                let (th, ph) = (u * TAU, v * PI);
                [r * ph.sin() * th.cos(), r * ph.cos(), r * ph.sin() * th.sin()]
            })
            .value())
        }),
        native("torus", |_, a| {
            let (a, kw) = split_kwargs(a);
            let (big, small) = (num(&a, &kw, 0, "radius", 1.0)?, num(&a, &kw, 1, "tube", 0.35)?);
            let (rings, sides) = (count(&a, &kw, 2, "rings", 32.0, 3)?, count(&a, &kw, 3, "sides", 14.0, 3)?);
            Ok(grid(rings, sides, true, |u, v| {
                let (th, ph) = (u * TAU, v * TAU);
                let d = big + small * ph.cos();
                [d * th.cos(), small * ph.sin(), d * th.sin()]
            })
            .value())
        }),
        native("cylinder", |_, a| {
            let (a, kw) = split_kwargs(a);
            let (r, hgt) = (num(&a, &kw, 0, "radius", 0.5)?, num(&a, &kw, 1, "height", 1.0)?);
            let segs = count(&a, &kw, 2, "segments", 24.0, 3)?;
            let mut m = grid(segs, 1, true, |u, v| [r * (u * TAU).cos(), hgt * (v - 0.5), r * (u * TAU).sin()]);
            // Caps: a fan from each centre.
            for (y, flip) in [(-hgt / 2.0, false), (hgt / 2.0, true)] {
                let c = m.verts.len();
                m.verts.push([0.0, y, 0.0]);
                for j in 0..segs {
                    let (p, q) = (j as f64 / segs as f64 * TAU, (j + 1) as f64 / segs as f64 * TAU);
                    let i = m.verts.len();
                    m.verts.push([r * p.cos(), y, r * p.sin()]);
                    m.verts.push([r * q.cos(), y, r * q.sin()]);
                    m.tris.push(if flip { [c, i + 1, i] } else { [c, i, i + 1] });
                }
            }
            Ok(m.value())
        }),
        native("plane", |_, a| {
            let (a, kw) = split_kwargs(a);
            let (w, d) = (num(&a, &kw, 0, "width", 1.0)?, num(&a, &kw, 1, "depth", 1.0)?);
            Ok(grid(1, 1, false, |u, v| [(u - 0.5) * w, 0.0, (v - 0.5) * d]).value())
        }),
        native("mesh", |_, a| {
            // gfx3d.mesh(vertices, faces): faces are triangles or quads of indices.
            let (a, _) = split_kwargs(a);
            let Some(Value::List(vs)) = a.first() else { return Err("mesh: vertices are a list of [x, y, z]".into()) };
            let verts: Vec<V3> = vs.borrow().iter().map(|v| v3(v, "mesh")).collect::<R<_>>()?;
            let Some(Value::List(fs)) = a.get(1) else { return Err("mesh: faces are a list of index lists".into()) };
            let mut tris = vec![];
            for f in fs.borrow().iter() {
                let Value::List(ix) = f else { return Err("mesh: each face is a list of indices".into()) };
                let ix: Vec<usize> = ix.borrow().iter().map(|i| i.as_i64().filter(|i| *i >= 0 && (*i as usize) < verts.len()).map(|i| i as usize)).collect::<Option<_>>().ok_or("mesh: a face index is outside the vertices")?;
                if ix.len() < 3 {
                    return Err("mesh: a face needs at least 3 vertices".into());
                }
                for k in 1..ix.len() - 1 {
                    tris.push([ix[0], ix[k], ix[k + 1]]);
                }
            }
            if tris.len() > MAX_TRIS {
                return Err(format!("mesh: more than {MAX_TRIS} triangles"));
            }
            Ok(Mesh { verts, tris, color: DEFAULT }.value())
        }),
        native("camera", |_, a| {
            let (a, kw) = split_kwargs(a);
            let position = v3(kw.get("position").or(a.first()).ok_or("camera: needs a position [x, y, z]")?, "camera")?;
            let target = match kw.get("target").or(a.get(1)) {
                Some(t) => v3(t, "camera target")?,
                None => [0.0; 3],
            };
            let fov = num(&a, &kw, 2, "fov", 60.0)?.clamp(5.0, 170.0);
            Ok(Value::Object(Rc::new(Camera { position, target, fov })))
        }),
        native("render", |_, a| {
            let (a, kw) = split_kwargs(a);
            let Some(Value::Object(co)) = a.first() else { return Err("render: the first argument is a Canvas".into()) };
            let canvas = co.as_any().downcast_ref::<Canvas>().ok_or("render: the first argument is a Canvas")?;
            let meshes = mesh_list(a.get(1).ok_or("render: needs meshes")?)?;
            let Some(Value::Object(cam)) = kw.get("camera").or(a.get(2)) else { return Err("render: needs a camera (gfx3d.camera(…))".into()) };
            let cam = cam.as_any().downcast_ref::<Camera>().ok_or("render: needs a camera")?;
            let light = match kw.get("light") {
                Some(l) => v3(l, "light")?,
                None => [-0.6, -1.0, -0.8],
            };
            let ambient = num(&[], &kw, 9, "ambient", 0.22)?.clamp(0.0, 1.0);
            let wire = match kw.get("wireframe") {
                Some(Value::Color(c)) => Some(*c),
                Some(Value::Bool(true)) => Some(0x101418ff),
                _ => None,
            };
            Ok(Value::Int(render(canvas, &meshes, cam, light, ambient, wire)?))
        }),
        native("project", |_, a| {
            // Where a 3D point lands on a canvas: [x, y], or null if behind the camera.
            let (a, _) = split_kwargs(a);
            let p = v3(a.first().ok_or("project: needs a point")?, "project")?;
            let Some(Value::Object(cam)) = a.get(1) else { return Err("project: needs a camera".into()) };
            let cam = cam.as_any().downcast_ref::<Camera>().ok_or("project: needs a camera")?;
            let Some(Value::Object(co)) = a.get(2) else { return Err("project: needs a Canvas".into()) };
            let c = co.as_any().downcast_ref::<Canvas>().ok_or("project: needs a Canvas")?;
            let v = View::new(cam, c.width(), c.height());
            Ok(match v.project(p) {
                Some((x, y, _)) => Value::list(vec![Value::Float(x), Value::Float(y)]),
                None => Value::Null,
            })
        }),
    ])
}

/// The `gfx3d` script extender.
pub struct Gfx3d;

impl Extender for Gfx3d {
    fn name(&self) -> &str {
        "gfx3d"
    }
    fn about(&self) -> &str {
        "3D meshes, a camera and a z-buffered renderer onto Canvas"
    }
    fn modules(&self, _it: &mut Interp) -> Vec<(String, Value)> {
        vec![("gfx3d".into(), module())]
    }
}
