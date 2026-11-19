//! 2D drawing for Z#++: `let c = Canvas(320, 200)`, then `c.rect(…)`,
//! `c.circle(…)`, `c.line(…)`, `c.polygon(…)`, `c.text(…)`, and
//! `c.shade(fragment, x, y, w, h)` to fill an area with a .zsg fragment
//! shader. Anti-aliased (tiny-skia). Colours are `#rrggbb` literals,
//! `rgb()`, `rgba()` or `hsl()`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};

use crate::interp::Interp;
use crate::value::{color_parts, HostObject, Value};

/// Largest canvas side and area, so scripts can't exhaust memory.
pub const MAX_SIDE: i64 = 4096;
pub const MAX_AREA: i64 = 4096 * 4096;

pub struct Canvas {
    pub pix: RefCell<Pixmap>,
}

fn paint(c: u32) -> Paint<'static> {
    let (r, g, b, a) = color_parts(c);
    let mut p = Paint::default();
    p.set_color_rgba8(r, g, b, a);
    p.anti_alias = true;
    p
}

fn n(a: &[Value], i: usize, what: &str) -> Result<f32, String> {
    a.get(i).and_then(|v| v.as_f64()).map(|x| x as f32).ok_or(format!("{what}: argument {} must be a number", i + 1))
}

fn color(a: &[Value], i: usize, what: &str) -> Result<u32, String> {
    match a.get(i) {
        Some(Value::Color(c)) => Ok(*c),
        Some(other) => Err(format!("{what}: argument {} must be a color (e.g. #2f7cf6), got {}", i + 1, other.type_name())),
        None => Err(format!("{what}: needs a color")),
    }
}

/// (x, y) from a Vec2 struct, a `[x, y]` list or a `{x:…, y:…}` map.
fn point(v: &Value) -> Option<(f32, f32)> {
    match v {
        Value::Struct(s) => {
            let s = s.borrow();
            let g = |k: &str| s.fields.iter().find(|(f, _)| f == k).and_then(|(_, v)| v.as_f64());
            Some((g("x")? as f32, g("y")? as f32))
        }
        Value::List(l) => {
            let l = l.borrow();
            Some((l.first()?.as_f64()? as f32, l.get(1)?.as_f64()? as f32))
        }
        Value::Map(m) => {
            let m = m.borrow();
            Some((m.get("x")?.as_f64()? as f32, m.get("y")?.as_f64()? as f32))
        }
        _ => None,
    }
}

impl Canvas {
    pub fn new(w: i64, h: i64) -> Result<Canvas, String> {
        if w <= 0 || h <= 0 || w > MAX_SIDE || h > MAX_SIDE || w * h > MAX_AREA {
            return Err(format!("Canvas: {w}×{h} is outside 1…{MAX_SIDE} per side"));
        }
        Ok(Canvas { pix: RefCell::new(Pixmap::new(w as u32, h as u32).unwrap()) })
    }

    pub fn new_value(_it: &mut Interp, w: i64, h: i64) -> Result<Value, String> {
        Ok(Value::Object(Rc::new(Canvas::new(w, h)?)))
    }

    pub fn width(&self) -> u32 {
        self.pix.borrow().width()
    }
    pub fn height(&self) -> u32 {
        self.pix.borrow().height()
    }

    /// Straight (not premultiplied) RGBA, row by row.
    pub fn rgba(&self) -> Vec<u8> {
        let p = self.pix.borrow();
        let mut out = Vec::with_capacity(p.data().len());
        for px in p.pixels() {
            let c = px.demultiply();
            out.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
        }
        out
    }

    pub fn png(&self) -> Result<Vec<u8>, String> {
        self.pix.borrow().encode_png().map_err(|e| e.to_string())
    }

    fn fill_path(&self, path: Option<tiny_skia::Path>, c: u32) {
        if let Some(p) = path {
            self.pix.borrow_mut().fill_path(&p, &paint(c), FillRule::Winding, Transform::identity(), None);
        }
    }

    fn stroke_path(&self, path: Option<tiny_skia::Path>, width: f32, c: u32) {
        if let Some(p) = path {
            let s = Stroke { width: width.max(0.1), line_cap: tiny_skia::LineCap::Round, line_join: tiny_skia::LineJoin::Round, ..Default::default() };
            self.pix.borrow_mut().stroke_path(&p, &paint(c), &s, Transform::identity(), None);
        }
    }

    fn round_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
        let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
        let mut pb = PathBuilder::new();
        pb.move_to(x + r, y);
        pb.line_to(x + w - r, y);
        pb.quad_to(x + w, y, x + w, y + r);
        pb.line_to(x + w, y + h - r);
        pb.quad_to(x + w, y + h, x + w - r, y + h);
        pb.line_to(x + r, y + h);
        pb.quad_to(x, y + h, x, y + h - r);
        pb.line_to(x, y + r);
        pb.quad_to(x, y, x + r, y);
        pb.close();
        pb.finish()
    }

    fn text(&self, x: f32, y: f32, s: &str, size: f32, c: u32) {
        // Bitmap glyphs: whole-pixel cells, no anti-aliasing (crisp, and
        // tiny-skia's AA rect fill rejects sub-pixel slivers).
        let scale = (size / 7.0).round().max(1.0);
        let (x, y) = (x.round(), y.round());
        let mut p = paint(c);
        p.anti_alias = false;
        let mut cx = x;
        for ch in s.chars() {
            if ch == '\n' {
                continue;
            }
            if let Some(rows) = glyph(ch) {
                for (ry, row) in rows.iter().enumerate() {
                    for col in 0..5 {
                        if row & (0b10000 >> col) != 0 {
                            if let Some(r) = Rect::from_xywh(cx + col as f32 * scale, y + ry as f32 * scale, scale, scale) {
                                self.pix.borrow_mut().fill_rect(r, &p, Transform::identity(), None);
                            }
                        }
                    }
                }
            }
            cx += 6.0 * scale;
        }
    }

    /// Fill the rectangle `area` (x, y, w, h) with a .zsg fragment shader (CPU reference).
    pub fn shade(&self, module: &crate::zsg::Module, frag: &str, area: (i32, i32, i32, i32), uniforms: &HashMap<String, f32>) -> Result<(), String> {
        let (x, y, w, h) = area;
        let mut p = self.pix.borrow_mut();
        let (pw, ph) = (p.width() as i32, p.height() as i32);
        let (x0, y0, x1, y1) = (x.max(0), y.max(0), (x + w).min(pw), (y + h).min(ph));
        let res = [w as f32, h as f32];
        let data = p.pixels_mut();
        for py in y0..y1 {
            for px in x0..x1 {
                let uv = [(px - x) as f32 / w.max(1) as f32 + 0.5 / w.max(1) as f32, (py - y) as f32 / h.max(1) as f32 + 0.5 / h.max(1) as f32];
                let [r, g, b, a] = module.shade(frag, uv, res, uniforms)?;
                let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                let c = tiny_skia::ColorU8::from_rgba(q(r), q(g), q(b), q(a)).premultiply();
                data[(py * pw + px) as usize] = c;
            }
        }
        Ok(())
    }
}

impl HostObject for Canvas {
    fn type_name(&self) -> &str {
        "Canvas"
    }
    fn field(&self, name: &str) -> Option<Value> {
        match name {
            "width" => Some(Value::Int(self.width() as i64)),
            "height" => Some(Value::Int(self.height() as i64)),
            _ => None,
        }
    }
    fn call(&self, _it: &mut Interp, m: &str, a: Vec<Value>) -> Result<Value, String> {
        match m {
            "clear" => {
                let (r, g, b, al) = color_parts(color(&a, 0, "clear")?);
                self.pix.borrow_mut().fill(tiny_skia::Color::from_rgba8(r, g, b, al));
            }
            "rect" => self.fill_path(Rect::from_xywh(n(&a, 0, "rect")?, n(&a, 1, "rect")?, n(&a, 2, "rect")?, n(&a, 3, "rect")?).map(PathBuilder::from_rect), color(&a, 4, "rect")?),
            "stroke_rect" => self.stroke_path(
                Rect::from_xywh(n(&a, 0, "stroke_rect")?, n(&a, 1, "stroke_rect")?, n(&a, 2, "stroke_rect")?, n(&a, 3, "stroke_rect")?).map(PathBuilder::from_rect),
                n(&a, 4, "stroke_rect")?,
                color(&a, 5, "stroke_rect")?,
            ),
            "round_rect" => self.fill_path(
                Self::round_rect(n(&a, 0, "round_rect")?, n(&a, 1, "round_rect")?, n(&a, 2, "round_rect")?, n(&a, 3, "round_rect")?, n(&a, 4, "round_rect")?),
                color(&a, 5, "round_rect")?,
            ),
            "circle" => self.fill_path(PathBuilder::from_circle(n(&a, 0, "circle")?, n(&a, 1, "circle")?, n(&a, 2, "circle")?), color(&a, 3, "circle")?),
            "stroke_circle" => self.stroke_path(
                PathBuilder::from_circle(n(&a, 0, "stroke_circle")?, n(&a, 1, "stroke_circle")?, n(&a, 2, "stroke_circle")?),
                n(&a, 3, "stroke_circle")?,
                color(&a, 4, "stroke_circle")?,
            ),
            "line" => {
                let mut pb = PathBuilder::new();
                pb.move_to(n(&a, 0, "line")?, n(&a, 1, "line")?);
                pb.line_to(n(&a, 2, "line")?, n(&a, 3, "line")?);
                self.stroke_path(pb.finish(), n(&a, 4, "line")?, color(&a, 5, "line")?);
            }
            "polygon" | "polyline" => {
                let Some(Value::List(pts)) = a.first() else { return Err(format!("{m}: the first argument is a list of points")) };
                let pts: Vec<(f32, f32)> = pts.borrow().iter().filter_map(point).collect();
                if pts.len() < 2 {
                    return Err(format!("{m}: needs at least 2 points"));
                }
                let mut pb = PathBuilder::new();
                pb.move_to(pts[0].0, pts[0].1);
                for p in &pts[1..] {
                    pb.line_to(p.0, p.1);
                }
                if m == "polygon" {
                    pb.close();
                    self.fill_path(pb.finish(), color(&a, 1, m)?);
                } else {
                    self.stroke_path(pb.finish(), n(&a, 1, m)?, color(&a, 2, m)?);
                }
            }
            "text" => {
                let s = a.get(2).map(|v| v.to_string()).unwrap_or_default();
                self.text(n(&a, 0, "text")?, n(&a, 1, "text")?, &s, n(&a, 3, "text")?, color(&a, 4, "text")?);
            }
            "pixel" => {
                let (x, y) = (n(&a, 0, "pixel")? as u32, n(&a, 1, "pixel")? as u32);
                let p = self.pix.borrow();
                let px = p.pixel(x, y).ok_or("pixel: outside the canvas")?.demultiply();
                return Ok(Value::Color(((px.red() as u32) << 24) | ((px.green() as u32) << 16) | ((px.blue() as u32) << 8) | px.alpha() as u32));
            }
            "shade" => {
                let Some(Value::Object(o)) = a.first() else { return Err("shade: the first argument is a fragment from a .zsg file".into()) };
                let Some(sr) = o.as_any().downcast_ref::<crate::zsg::ShaderRef>() else { return Err("shade: the first argument is a fragment from a .zsg file".into()) };
                let (x, y, w, h) = (n(&a, 1, "shade")? as i32, n(&a, 2, "shade")? as i32, n(&a, 3, "shade")? as i32, n(&a, 4, "shade")? as i32);
                let mut unis = HashMap::new();
                if let Some(Value::Map(mp)) = a.get(5) {
                    for (k, v) in mp.borrow().iter() {
                        unis.insert(k.clone(), v.as_f64().unwrap_or(0.0) as f32);
                    }
                }
                self.shade(&sr.module, &sr.name, (x, y, w, h), &unis)?;
            }
            _ => return Err(format!("Canvas has no method `{m}`")),
        }
        Ok(Value::Null)
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// A 5×7 bitmap font: digits, letters (lower case drawn as capitals) and
/// common punctuation.
fn glyph(c: char) -> Option<[u8; 7]> {
    Some(match c.to_ascii_uppercase() {
        ' ' => [0; 7],
        '0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1F],
        '3' => [0x1F, 0x02, 0x04, 0x02, 0x01, 0x11, 0x0E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        '6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        'A' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'B' => [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
        'C' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
        'D' => [0x1C, 0x12, 0x11, 0x11, 0x11, 0x12, 0x1C],
        'E' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        'F' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
        'G' => [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F],
        'H' => [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'I' => [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
        'J' => [0x07, 0x02, 0x02, 0x02, 0x02, 0x12, 0x0C],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
        'M' => [0x11, 0x1B, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x11, 0x19, 0x15, 0x13, 0x11, 0x11],
        'O' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'P' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
        'Q' => [0x0E, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0D],
        'R' => [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
        'S' => [0x0F, 0x10, 0x10, 0x0E, 0x01, 0x01, 0x1E],
        'T' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0A, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0A],
        'X' => [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11],
        'Y' => [0x11, 0x11, 0x11, 0x0A, 0x04, 0x04, 0x04],
        'Z' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1F],
        '.' => [0, 0, 0, 0, 0, 0x0C, 0x0C],
        ',' => [0, 0, 0, 0, 0x0C, 0x04, 0x08],
        ':' => [0, 0x0C, 0x0C, 0, 0x0C, 0x0C, 0],
        '!' => [0x04, 0x04, 0x04, 0x04, 0x04, 0, 0x04],
        '?' => [0x0E, 0x11, 0x01, 0x02, 0x04, 0, 0x04],
        '-' => [0, 0, 0, 0x1F, 0, 0, 0],
        '+' => [0, 0x04, 0x04, 0x1F, 0x04, 0x04, 0],
        '#' => [0x0A, 0x0A, 0x1F, 0x0A, 0x1F, 0x0A, 0x0A],
        '/' => [0x01, 0x01, 0x02, 0x04, 0x08, 0x10, 0x10],
        '(' => [0x02, 0x04, 0x08, 0x08, 0x08, 0x04, 0x02],
        ')' => [0x08, 0x04, 0x02, 0x02, 0x02, 0x04, 0x08],
        '%' => [0x18, 0x19, 0x02, 0x04, 0x08, 0x13, 0x03],
        '=' => [0, 0, 0x1F, 0, 0x1F, 0, 0],
        '\'' => [0x04, 0x04, 0x08, 0, 0, 0, 0],
        '_' => [0, 0, 0, 0, 0, 0, 0x1F],
        _ => return None,
    })
}
