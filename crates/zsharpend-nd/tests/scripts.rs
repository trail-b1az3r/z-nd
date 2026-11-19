//! nd from .zxx programs.

use zsharpend::extend::Extender;
use zsharpend::program::Options;
use zsharpend::Program;
use zsharpend_nd::Nd;

fn run(src: &str) -> Result<String, String> {
    let s = src.to_string();
    let read = move |p: &str| if p == "main.zxx" { Some(s.clone()) } else { None };
    let ext: [&dyn Extender; 1] = [&Nd];
    let mut p = Program::load_with("main.zxx", &read, |_| {}, Options { extenders: &ext, ..Default::default() })?;
    p.run_main()?;
    Ok(p.output())
}

#[test]
fn numpy_style() {
    let out = run(r#"
import nd
fn Main() {
    let a = nd.arange(6).reshape(2, 3)
    Printl(a)
    Printl(a.shape, a.ndim, a.size)
    Printl(a * 2 + 1)
    Printl(a + nd.array([10, 20, 30]))
    Printl(a.sum(), a.sum(axis=0), a.mean(axis=1))
    Printl(a @ a.T)
    Printl(a[1], a[:, 1], a[0, 2], a[::-1, ::2])
    Printl(a[a > 2])
    a[a > 2] = 0
    Printl(a)
    let x = nd.linspace(0, 1, 5)
    Printl(x, nd.sqrt(nd.array([4, 9])), nd.sum(nd.ones((2, 2))))
    Printl(nd.eye(2) @ nd.array([3, 4]), nd.dot([1, 2, 3], [4, 5, 6]))
    Printl(nd.concatenate([nd.zeros(2), nd.ones(2)]), nd.stack([[1, 2], [3, 4]]).shape)
    Printl(x.apply(|v| v * 100).max(), nd.array([3, 1, 2]).argmax())
    Printl(nd.allclose(nd.array([0.1 + 0.2]), nd.array([0.3])))
}
"#)
    .unwrap();
    assert_eq!(
        out,
        "array([[0., 1., 2.],\n       [3., 4., 5.]])\n[2, 3] 2 6\narray([[1., 3., 5.],\n       [7., 9., 11.]])\narray([[10., 21., 32.],\n       [13., 24., 35.]])\n15.0 array([3., 5., 7.]) array([1., 4.])\narray([[5., 14.],\n       [14., 50.]])\narray([3., 4., 5.]) array([1., 4.]) 2.0 array([[3., 5.],\n       [0., 2.]])\narray([3., 4., 5.])\narray([[0., 1., 2.],\n       [0., 0., 0.]])\narray([0., 0.25, 0.5, 0.75, 1.]) array([2., 3.]) 4.0\narray([3., 4.]) 32.0\narray([0., 0., 1., 1.]) [2, 2]\n100.0 0.0\ntrue\n"
    );
}

#[test]
fn errors() {
    assert!(run("import nd\nfn Main() { nd.ones((2, 3)) + nd.ones((2, 2)) }\n").unwrap_err().contains("can't be broadcast"));
    assert!(run("import nd\nfn Main() { nd.ones((2, 3)) @ nd.ones((2, 3)) }\n").unwrap_err().contains("inner sizes"));
    assert!(run("import nd\nfn Main() { nd.array([[1, 2], [3]]) }\n").unwrap_err().contains("rectangular"));
    assert!(run("import nd\nfn Main() { nd.arange(4)[9] }\n").unwrap_err().contains("out of range"));
}
