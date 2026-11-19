# Whetstone

Whetstone is the Zsharpend compiler. It reads a project's [.wtf file](wtf.md), finds every file the program uses, checks the whole program, writes a self-contained bundle, and runs it.

## Commands

| Command | Does |
|---|---|
| `whetstone new <dir> [--host stonedgem]` | Start a project: `<dir>/<dir>.wtf`, `src/main.zxx`, `modules/`. |
| `whetstone check [project]` | Parse and check; print errors and warnings. Exit status 1 on errors. |
| `whetstone build [project]` | Check, then write `build/<name>.wsb`. |
| `whetstone run [project \| file.wsb]` | Build (unless given a bundle) and run. |
| `whetstone install <folder>` | Install a module or extension (a folder with a `module.wtf`). |
| `whetstone uninstall <name>`, `whetstone list` | Manage installed modules. |
| `whetstone extenders` | The script extenders built into this Whetstone. |

`run` takes `--png out.png` (save the canvas the program leaves in `canvas` or `screen`), `--seed n` (repeatable `random()`), and for StonedGem projects `--page <url>` and `--title <text>`.

## What compiling does

1. **Read the .wtf** and resolve what it names: script extenders, extensions, module folders, the host.
2. **Parse** the entry and every file it reaches — `import "file"`, `#include`, `import module` — in any of the four file kinds. Each script extender's pass runs over each program file.
3. **Check** the program as a whole (below).
4. **Bundle** every source with the project's settings into `build/<name>.wsb`: JSON that runs without the project folder or installed modules.
5. **Run**: extensions' `on start`, `Main()`, host events, `on exit`.

## The checks

Zsharpend runs dynamically, so these catch before a run what would otherwise fail during one — or never fail, and quietly do the wrong thing.

**Errors** (the build stops):

- a name that isn't defined anywhere (locals, globals, imports, built-ins, extenders, host);
- assigning to a constant;
- calling one of the program's functions with too many arguments, or a keyword argument it doesn't have;
- `Enum::Variant` for a variant that doesn't exist, or a struct literal with an unknown field;
- a HolyC print statement whose `%` count doesn't match its values;
- a function defined twice;
- `impl Trait for Type` missing one of the trait's methods.

**Warnings**:

- code after `return`, `break`, `continue` or `throw`;
- a local variable that's never used (names starting with `_` are exempt);
- a `match` on an enum that leaves variants out and has no `_` arm;
- a call that leaves a required argument out;
- assigning to an undeclared name in a `.zxx` file (Z#'s implicit declaration).

With `strict = true` in the `.wtf`, warnings fail the build too.

```zxx whetstone
const LIMIT = 10
enum Shape { Circle(r), Square(s), Empty }

fn area(w, h = 1) { return w * h }

fn Main() {
    let unused = 3
    LIMIT = 11
    Printl(totl)
    area(1, 2, 3)
    area(w=1, depth=2)
    let s = Shape::Triangle(3)
    "%d and %d\n", 1;
    let n = match s { Circle(r) => r, Square(x) => x }
    return n
    Printl("never")
}
```

## Bundles

A `.wsb` file is JSON: the bundle format, the project's name, version and host, the extenders and extensions it uses, and every source file by the path the program knows it by. `whetstone run app.wsb` runs it anywhere the same Whetstone (with the same extenders) is installed.

::: note
Whetstone checks and bundles programs and runs them on the Zsharpend engine, an interpreter; it doesn't produce machine code.
:::
