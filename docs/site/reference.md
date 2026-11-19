# Language Reference

This reference describes Zsharpend's syntax and meaning exactly; the [tutorial](tutorial.md) is the gentler introduction. It has three parts: this page (files and lexical structure), [statements](reference-statements.md), and [expressions](reference-expressions.md).

## Files

| Extension | Holds | Parsed as |
|---|---|---|
| `.zxx` | a Zsharpend program or module | everything on these pages |
| `.zsp`, `.zs` | a Z#++ (or Z#) program | Z#++ syntax: no `.zxx` keywords |
| `.zsx` | a header: signatures, structs, constants | declarations only |
| `.zsg` | GPU code: kernels and fragment shaders | typed GPU subset |

A program's files can mix kinds: a `.zxx` program can `import "legacy.zsp"`, a header, or GPU code. Each file is parsed by its own rules. How a project is compiled is described by its [.wtf file](wtf.md).

## Lines and statements

A statement ends at the end of its line, or at `;`. A line continues onto the next when it clearly isn't finished: after an operator, a comma, an opening bracket, or before a line that starts with `.`, `?.` or `|>`.

## Comments

`//` to the end of the line; `/* … */` across lines.

## Keywords

These words are reserved in every program file:

```text
func let const var global if else while for in return break continue struct impl on
import match try catch throw true false null and or not extern
```

and these only in `.zxx` files (so Z#++ programs can still use them as names):

```text
fn enum trait loop elif switch case mut from None True False
```

`kernel`, `fragment`, `buf` and `uniform` belong to `.zsg` files.

## Literals

| Kind | Examples |
|---|---|
| Integers (64-bit) | `42`, `-7`, `0xFF`, `1_000_000` |
| Floats (64-bit) | `3.14`, `2.0`, `1e-3`, `2f` |
| Strings | `"text"`, `'text'`, `"hi ${name}"` — escapes `\n \t \r \0 \$ \\ \"` |
| Colours | `#f80`, `#2f7cf6`, `#2f7cf680` (RGBA) |
| Booleans and null | `true`, `false`, `null`; in `.zxx` also `True`, `False`, `None` |
| Lists, tuples | `[1, 2, 3]`, `(a, b)` (a tuple is a list), `()` |
| Maps | `{ name: "Ada", "two words": 2 }` |
| Ranges | `0..10` (end excluded), `0..=10` (included) |

## Types

Types are optional and checked when a value is stored or returned. The names come from Z#, Rust and HolyC:

| Type | Also written | A value is |
|---|---|---|
| `int` | `i64`, `I64`, `u64`, `U64`, `usize`, `isize` | a 64-bit integer |
| `i8`, `i16`, `i32`, `u8`, `u16`, `u32` | `I8`…`U32` | an integer **wrapped** to that width (`let b: u8 = 300` is 44) |
| `float` | `f64`, `F64`, `f32`, `F32` | a 64-bit float |
| `bool` | `Bool` | `true` or `false` |
| `string` | `str`, `String` | text |
| `list`, `map`, `color` | | those values |
| `U0` | | nothing (HolyC's void) |

## HolyC syntax

`.zxx` files accept HolyC's C-style declarations, with the type first:

```zxx run
I64 Square(I64 x) { return x * x }
U0 Greet(string who) { "hello, %s\n", who; }

fn Main() {
    Greet("Terry")
    Printl(Square(12))
}
```

`#include "file.zxx"` is the same as `import "file.zxx"`.
