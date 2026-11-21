# Library Reference

The Language Reference describes Zsharpend's syntax; this part describes what comes with it.

- [Built-in functions](library-builtins.md) — always available, also as methods on their first argument.
- [nd — n-dimensional arrays](library-nd.md) — NumPy-style arrays; `import nd`.
- [gfx3d — 3D rendering](library-gfx3d.md) — meshes, cameras, a z-buffered renderer; `import gfx3d`.
- [os — the operating system](library-os.md) — the terminal, processes, files and environment, for programs that leave the sandbox; `import os`.
- [Canvas — 2D drawing](library-canvas.md) — `Canvas(w, h)`, and GPU shaders from `.zsg` files.

`nd`, `gfx3d` and `os` are [script extenders](extending.md#script-extenders) built into Whetstone; a project lists the ones it uses in its [.wtf](wtf.md):

```toml
[extenders]
use = ["nd", "gfx3d"]
```

The [Module Index](modindex.html) lists every module.
