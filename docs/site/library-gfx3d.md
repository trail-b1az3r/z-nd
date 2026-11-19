# gfx3d — 3D rendering

::: module gfx3d — 3D meshes, a camera and a z-buffered renderer

`gfx3d` draws 3D scenes onto a [Canvas](library-canvas.md): meshes made from shapes or your own vertices, moved and turned with transforms, seen through a perspective camera, and drawn with a depth buffer and flat Lambert lighting. It runs on the CPU, so it works anywhere Zsharpend does.

```zxx run
import gfx3d

fn Main() {
    global let canvas = Canvas(480, 300)
    canvas.clear(#10141c)
    let cam = gfx3d.camera([3, 2.4, 5], target=[0, 0.2, 0], fov=45)
    let floor = gfx3d.plane(8, 8).color(#2a2e34).translate(0, -1, 0)
    let cube = gfx3d.cube(1.4).rotate(0, 30, 0).color(#e8b45c).translate(-1.3, -0.3, 0)
    let ball = gfx3d.sphere(0.9, 20, 36).color(#2f7cf6).translate(1.1, -0.1, -0.4)
    let ring = gfx3d.torus(0.7, 0.22).rotate(70, 0, 20).color(#a3322a).translate(0.1, 0.9, 1.0)
    let n = gfx3d.render(canvas, [floor, cube, ball, ring], cam, light=[-1, -2, -1.5])
    Printl(n, "triangles drawn")
}
```

## Shapes

Each returns a [Mesh](#Mesh) centred on the origin, coloured light grey.

::: function gfx3d.cube(size=1)
A cube with sides of `size`.
:::

::: function gfx3d.sphere(radius=1, rings=12, segments=24)
A sphere; more rings and segments make it smoother.
:::

::: function gfx3d.torus(radius=1, tube=0.35, rings=32, sides=14)
A ring: `radius` to the middle of the tube, `tube` the tube's radius.
:::

::: function gfx3d.cylinder(radius=0.5, height=1, segments=24)
A closed cylinder standing on the y axis.
:::

::: function gfx3d.plane(width=1, depth=1)
A flat rectangle in the x–z plane.
:::

::: function gfx3d.mesh(vertices, faces)
A mesh of your own: `vertices` is a list of `[x, y, z]`, `faces` a list of index lists (triangles, quads or larger polygons, split into triangles).
:::

```zxx run
import gfx3d

fn Main() {
    let pyramid = gfx3d.mesh([[0, 1, 0], [-1, 0, -1], [1, 0, -1], [1, 0, 1], [-1, 0, 1]], [[0, 1, 2], [0, 2, 3], [0, 3, 4], [0, 4, 1], [1, 2, 3, 4]])
    Printl(pyramid, pyramid.center)
    global let canvas = Canvas(200, 140)
    canvas.clear(#fbfaf7)
    gfx3d.render(canvas, pyramid.color(#2c6e3a).rotate(0, 25, 0), gfx3d.camera([0, 1.6, 3.4], target=[0, 0.4, 0]), wireframe=#101418)
}
```

## Meshes

::: class Mesh
A triangle mesh with one colour. Its transforms return a new mesh; the original doesn't change.
:::

::: method Mesh.rotate(x=0, y=0, z=0)
Turned by `x`, `y` and `z` degrees about the origin, in that order.
:::

::: method Mesh.translate(x=0, y=0, z=0)
Moved.
:::

::: method Mesh.scale(s)
Scaled by `s`, or by `x`, `y`, `z` separately: `scale(1, 2, 1)`.
:::

::: method Mesh.color(colour)
The same mesh in another colour.
:::

::: method Mesh.merge(other)
One mesh with both meshes' triangles (it keeps this one's colour).
:::

::: data Mesh.triangles
How many triangles it has. Also `Mesh.vertices` (a list of points) and `Mesh.center`.
:::

## Cameras and rendering

::: function gfx3d.camera(position, target=[0, 0, 0], fov=60)
A perspective camera at `position` looking at `target`, with a vertical field of view of `fov` degrees.
:::

::: method Camera.orbit(degrees)
The camera moved around its target, about the vertical axis.
:::

::: function gfx3d.render(canvas, meshes, camera, light=[-0.6, -1, -0.8], ambient=0.22, wireframe=None)
Draws one mesh or a list of them onto `canvas`, nearest in front. `light` is the direction light travels; `ambient` the share of light every face gets; `wireframe` a colour (or `true`) to outline triangles. Faces are drawn from both sides. Returns how many triangles were drawn. Anything behind the camera is skipped.
:::

::: function gfx3d.project(point, camera, canvas)
Where a 3D point lands on the canvas, as `[x, y]`, or `null` behind the camera — for labelling things in a scene.
:::

```zxx run
import gfx3d

fn Main() {
    global let canvas = Canvas(480, 220)
    canvas.clear(#10141c)
    let knot = gfx3d.torus(0.8, 0.3, 40, 16).rotate(60, 0, 0).color(#e8b45c)
    var cam = gfx3d.camera([0, 1, 4], fov=40)
    // Walk the camera round the scene; where does a point on the ring land?
    for step in 0..3 {
        Printl("after", step * 60, "degrees:", gfx3d.project([0.8, 0, 0], cam, canvas))
        cam = cam.orbit(60)
    }
    gfx3d.render(canvas, knot, cam, light=[-1, -1, -1])
}
```
