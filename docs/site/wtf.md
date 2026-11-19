# The .wtf file

A `.wtf` — **Whetstone Translation File** — is what Whetstone needs to compile a project's `.zxx` files. Every project folder has exactly one (named after the project, `orbit/orbit.wtf`). It's TOML.

```toml include=examples/orbit/orbit.wtf
```

and the program it compiles:

```zxx run include=examples/orbit/src/main.zxx
```

## [project]

| Key | Default | Meaning |
|---|---|---|
| `name` | (required) | Letters, digits, `-` and `_`. Names the bundle. |
| `version` | `"0.1.0"` | |
| `entry` | `"src/main.zxx"` | The file the program starts from (`.zxx` or `.zsp`). |
| `host` | `"standalone"` | `"standalone"`, or `"stonedgem"` for StonedGem extension scripts (see [Embedding](embedding.md#stonedgem)). |
| `description` | `""` | |

## [build]

| Key | Default | Meaning |
|---|---|---|
| `out` | `"build"` | Where `whetstone build` writes the bundle. |
| `strict` | `false` | Warnings fail the build too. |

## [extenders]

`use = ["nd", "gfx3d"]` — the [script extenders](extending.md#script-extenders) the program uses. Their modules become importable and their names known to the checks.

## [extensions]

`use = ["timing"]` — [extensions](extending.md#extensions) to hook into the program: from the project's `extensions/<name>/`, or installed.

## [modules]

`name = "path"` — module folders outside `modules/`, relative to the project: `shared = "../shared/geometry"` makes `import shared` load that folder.

Unknown keys are errors, so a typo doesn't go unnoticed.

## module.wtf

An installable module or extension has a `module.wtf`:

```toml include=examples/shapes/modules/geometry/module.wtf
```

| Key | Default | Meaning |
|---|---|---|
| `name` | (required) | What `import name` and `[extensions] use` refer to. |
| `version` | `"0.1.0"` | |
| `kind` | `"module"` | `"module"` (imported) or `"extension"` (hooked in). |
| `entry` | `"mod.zxx"` | The file to load. |
| `description` | `""` | Shown by `whetstone list`. |
