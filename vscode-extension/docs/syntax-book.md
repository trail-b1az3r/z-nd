# Zsharpend & Z#++ Syntax Book

This is a practical quick-reference for the VS Code extension. Syntax is based on the project README and its documented language features; where syntax may differ between runtime versions, check the interpreter and examples in this repository.

## File types

| Extension | Language | Use |
|---|---|---|
| `.zxx` | Zsharpend | Current Zsharpend scripts |
| `.zsp` | Z#++ | Z#++ scripts |
| `.zsx` | Z#++ | Z#++ script variant |
| `.zsg` | Z#++ GPU / shader | GPU-oriented code; exact grammar varies |

## Comments and literals

```
 // line comment
 /* block comment */

let name = "text";
let count = 42;
let ratio = 3.14;
let enabled = true;
let nothing = None;
```

## Variables and mutability (Zsharpend)

```
let value = 10;
let mut counter = 0;
counter = counter + 1;
const LIMIT = 100;
```

## Functions

```
fn greet(name) {
    print(name);
}

fn add(a, b) {
    return a + b;
}
```

## Conditionals

Zsharpend documents `elif`, `not`, and `not in`:

```
if value > 10 {
    print("large");
} elif value == 10 {
    print("exact");
} else {
    print("small");
}
```

## Loops

```
for item in items {
    print(item);
}

while count < 5 {
    count = count + 1;
}

loop {
    // break or return to leave
    break;
}
```

## Python-style features (Zsharpend)

Comprehensions, slicing, unpacking, keyword arguments, decorators, `elif`, `not in`, `None/True/False`, and matrix multiplication `@` are listed in the README.

```
squares = [n * n for n in numbers];
part = values[1:4];
```

## Rust-inspired features

```
enum ResultState {
    Ready,
    Failed(String)
}

match state {
    Ready => print("ready"),
    Failed(message) => print(message),
}
```

Also documented: traits (checked), `Ok`/`Err`/`Some` with `?`, `loop`, `let mut`, and sized integers.

## HolyC-inspired syntax

```
printf("%d\\n", value);
switch (value) {
    case 1...5:
        print("small");
        break;
}

I64 add(I64 a, I64 b) {
    return a + b;
}
```

README also lists `U0`…`F64` type names and `#include`.

## Arrays and nd extender

```
import nd;
a = [1, 2, 3];
b = a[0:2];
```

The `nd` extender provides NumPy-style n-dimensional arrays, broadcasting, slicing, masks, reductions, and `@` matrix multiplication.

## Modules

```
import math;
import nd;
import gfx3d;
```

Available project modules, installed modules, and Rust-host modules depend on the environment.

## Graphics

`gfx3d` is documented as a mesh/camera and z-buffered software-renderer extender targeting a 2D canvas. Host integration and API calls depend on the version installed.

## Extensions and embedding

Zsharpend supports `.zxx` extensions with event hooks. Rust host applications can implement the `Extender` trait to add modules or rewrite source. Script extenders are compiled into the host; runtime native plugin loading is not currently supported.

## Whetstone

Whetstone is the compiler/bundler and runner. Projects use `.wtf` (Whetstone Translation File) to define compilation. Whetstone parses reachable code and performs whole-program checks (undefined names, call arity, constants, enum variants, printf format strings, trait checks, unused/unreachable code), writes a self-contained bundle, and runs it. It does not generate machine code.

## Z#++ compatibility

Zsharpend is described as retaining compatibility for `.zsp`, `.zsx`, and `.zsg` files. The VS Code extension treats those as distinct file types and provides basic highlighting, completion, snippets, and this book. Exact legacy syntax is version-dependent.

## Tips

- Completion and hover are editor assistance, not a compiler or semantic validator.
- This extension intentionally avoids claiming every runtime API exists in every build.
- Run scripts using the toolchain installed for the project (e.g. `whetstone`).
