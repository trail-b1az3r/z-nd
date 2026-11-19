//! gfx3d from .zxx programs: meshes are drawn with depth and lighting.

use zsharpend::canvas::Canvas;
use zsharpend::extend::Extender;
use zsharpend::program::Options;
use zsharpend::{Program, Value};
use zsharpend_3d::Gfx3d;

fn run(src: &str) -> Program {
    let s = src.to_string();
    let read = move |p: &str| if p == "main.zxx" { Some(s.clone()) } else { None };
    let ext: [&dyn Extender; 1] = [&Gfx3d];
    let mut p = Program::load_with("main.zxx", &read, |_| {}, Options { extenders: &ext, ..Default::default() }).unwrap_or_else(|e| panic!("{e}"));
    p.run_main().unwrap_or_else(|e| panic!("{e}"));
    p
}

fn pixel(p: &Program, x: u32, y: u32) -> [u8; 4] {
    let Some(Value::Object(o)) = p.interp.global("canvas") else { panic!("no canvas") };
    let c = o.as_any().downcast_ref::<Canvas>().unwrap();
    let rgba = c.rgba();
    let i = ((y * c.width() + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}

#[test]
fn a_lit_scene_with_depth() {
    let p = run(r#"
import gfx3d
fn Main() {
    global let canvas = Canvas(320, 240)
    canvas.clear(#10141c)
    let cam = gfx3d.camera([0, 0, 5], target=[0, 0, 0], fov=50)
    // The red cube is nearer than the blue sphere; both sit on the axis.
    let near = gfx3d.cube(1).color(#ff0000).translate(0, 0, 1)
    let far = gfx3d.sphere(1.5).color(#0000ff).translate(0, 0, -2)
    let n = gfx3d.render(canvas, [far, near], cam, light=[0, 0, -1], ambient=0.2)
    Printl("drew", n > 0)
    Printl(gfx3d.project([0, 0, 0], cam, canvas))
    Printl(gfx3d.torus(), gfx3d.cylinder(segments=8).triangles)
}
"#);
    assert_eq!(p.output(), "drew true\n[160.0, 120.0]\nMesh(495 vertices, 896 triangles) 32\n");
    let centre = pixel(&p, 160, 120);
    assert!(centre[0] > 200 && centre[2] < 30, "the nearer red cube covers the centre: {centre:?}");
    let side = pixel(&p, 160 - 47, 120);
    assert!(side[2] > 100 && side[0] < 30, "the sphere shows beside the cube: {side:?}");
    assert_eq!(pixel(&p, 2, 2), [0x10, 0x14, 0x1c, 255], "background untouched");
}

#[test]
fn behind_the_camera_is_skipped() {
    let p = run("import gfx3d\nfn Main() {\n    global let canvas = Canvas(64, 64)\n    canvas.clear(#000000)\n    let cam = gfx3d.camera([0, 0, 0], target=[0, 0, -1])\n    Printl(gfx3d.render(canvas, gfx3d.cube().translate(0, 0, 5), cam))\n}\n");
    assert_eq!(p.output(), "0\n");
}
