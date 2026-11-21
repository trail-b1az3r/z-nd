# What's new in Zsharpend 0.1

Zsharpend is the next version of Z#++. Everything Z#++ does still works — its `.zsp` programs, `.zsx` headers and `.zsg` GPU code load unchanged — and Zsharpend adds a file type, language features from three languages, two modules, a module system and a compiler.

## .zxx files

Zsharpend programs are `.zxx` files. The new syntax and its keywords (`fn`, `enum`, `trait`, `loop`, `elif`, `switch`, `case`, `mut`, `from`, `None`, `True`, `False`) exist only in `.zxx` files, so a Z#++ program that uses `loop` or `case` as a variable name keeps working.

## From Python

- List and map comprehensions: `[x * x for x in xs if x > 0]`, `{k: len(v) for (k, v) in items(m)}`
- Slices with steps and negative indices: `xs[1:-1]`, `s[::-1]`, and slice assignment
- Unpacking and swapping: `let (a, b) = pair`, `a, b = b, a`, `for (i, x) in enumerate(xs)`
- Keyword arguments: `area(width=3, height=4)`
- Decorators: `@cached` before a function
- `elif`, `not in`, `None`, `True`, `False`, `enumerate`, `items`, `any`, `all`, and `@` for matrix multiplication

## From Rust

- `enum` with data, and `match` that destructures it: `Circle(r) => PI * r * r`
- `trait` with required and default methods, `impl Trait for Type` (checked)
- `Ok`/`Err` results and `Some`/`None` options with the `?` operator
- `fn`, `loop`, `let mut`, `Enum::Variant` paths, sized integer types (`u8`, `i32`, `f64`…)

## From HolyC

- A string on its own line prints it; with values after it, it's a printf format: `"%d items\n", n;`
- A function's name on its own line calls it: `Main;`
- `switch` with `case 1...5:` ranges
- C-style declarations: `I64 Square(I64 x) { … }`, and the types `U0`, `U8`…`U64`, `I8`…`I64`, `F64`
- `#include "file.zxx"`

## New modules

- [nd](library-nd.md): n-dimensional arrays with broadcasting, slicing, masks, reductions and `@`.
- [gfx3d](library-gfx3d.md): meshes, a camera and a z-buffered renderer onto a canvas.

## Modules, extensions and script extenders

`import name` loads a module — one built into Whetstone, one in the project's `modules/` folder, or one installed with `whetstone install`. Extensions hook into every program a project builds; script extenders are Rust plugins that add modules and rewrite code. See [Extending Zsharpend](extending.md).

## Whetstone and .wtf

Whetstone compiles a project described by its [.wtf file](wtf.md): it parses every file the program reaches, checks the whole program (undefined names, wrong argument counts, unknown enum variants, printf formats and more), writes a self-contained bundle, and runs it. See [Whetstone](whetstone.md).

## Leaving the sandbox

- The [os](library-os.md) script extender gives programs the terminal (raw keys, direct output, size), processes (pipelines, redirections, job control), files and the environment. Programs that use it have no time or step budget.
- `examples/spire` is an interactive shell written in Zsharpend with it: fish and bash syntax (here-documents, `[[ ]]`, `$(( ))`, arrays, traps), concurrent pipelines, syntax highlighting, autosuggestions, programmable completion, and your fish config.
- `os` also has `fork`, `pipe`, `dup2`, signal traps (`os.trap`, `os.pending_signals`), POSIX regular expressions (`os.regex`) and a `timeout_ms` option for captured commands.
- Strings understand `\e`, `\xHH` and `\u{…}`, for terminal escape sequences.
- Whetstone runs programs with a larger stack, so deep recursion doesn't overflow.
