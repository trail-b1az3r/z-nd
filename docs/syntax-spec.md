# Zsharpend syntax specification

Stage 1 of the [language roadmap](native-language-roadmap.md). This document
describes the syntax the implementation accepts **today**, for every dialect,
so later stages can change the language deliberately instead of by accident.
Each rule here is pinned by a test: the fixture suite in
`crates/zsharpend/tests/compat/` (one program per file with its recorded
output, see [Compatibility suite](#compatibility-suite)) plus the unit tests in
`crates/zsharpend/tests/`.

The [Still proposed](#still-proposed) section lists what is specified but not implemented yet.

## Dialects

| Extension | Dialect | Notes |
| --- | --- | --- |
| `.zsp` | Z#++ | Sam-astro's Z# plus structs, lambdas, pipes, `match`, `try`. Original Z# programs run unchanged. |
| `.zsx` | header | Declarations only (`struct`, `func f(...) -> T` without body). A program that imports a header must define everything it declares. |
| `.zsg` | GPU | `kernel` and `fragment` functions, compiled to WGSL. A typed subset of the expression language. |
| `.zxx` | Zsharpend | The modern dialect: everything in `.zsp`, plus the words in [`.zxx`-only keywords](#keywords) and the Python, Rust and HolyC forms below. |

A `.zsp` program never changes meaning because `.zxx` gained a feature:
words reserved in `.zxx` stay ordinary names in `.zsp`
(fixture `zsharppp_names.zsp`).

## Lexical structure

* **Source** is UTF-8. Identifiers are letters, digits and `_`, not starting
  with a digit (Unicode letters allowed).
* **Statements end at a line end.** `;` may also end a statement. A newline is
  ignored where an expression obviously continues: after a binary operator, a
  comma or an opening bracket, and before a line starting with `.`, `?.` or `|>`.
* **Comments:** `// to end of line` and `/* block */`.
* **Numbers:** `42`, `1_000`, `0xff`, `2.5`, `1e3`. A literal with `.` or an
  exponent is a `float`, otherwise an `int` (64-bit).
* **Strings:** `"…"` or `'…'`. Escapes: `\n \t \r \0 \\ \" \' \e`, `\xHH`,
  `\u{H…}`. `${expr}` interpolates any expression.
* **Colours:** `#rrggbb` and `#rrggbbaa` are `color` literals.
* **`#include "file"`** (HolyC) is `import "file"`.

### Keywords

Reserved in every dialect:

```
func let const var global if else while for in return break continue struct
impl on import match try catch throw true false null and or not extern kernel
fragment
```

Reserved only in `.zxx`:

```
fn enum trait loop elif switch case mut from None True False
```

`as`, `is`, `default` and type names (`int`, `text`, …) are not reserved;
they are recognised by position.

## Declarations

```
func Name(params) [-> Type] { body }      // .zsp and .zxx
fn   Name(params) [-> Type] { body }      // .zxx
TYPE Name(TYPE p, …) { body }             // .zxx, HolyC: I64 Twice(I64 x)
struct Name { field[: Type], … }
enum Name { Variant, Variant(field, …), … }               // .zxx
trait Name { fn method(self) [-> T]  |  fn method(self) { default } }   // .zxx
impl Name { methods }  /  impl Trait for Name { methods }
const NAME = expr
import "file.zsx" | "file.zxx" | module [as alias] | from module import a, b
on event(params) { body }
@decorator                                 // .zxx, before fn
```

Parameters: `name`, `name: Type`, `name = default`. Calls may pass keyword
arguments (`box(width=5)`) in `.zxx`. `Main()` is the entry point.

### Variables

```
TYPE name = expr            // original Z#: int i = 0, string s = "r"
global TYPE name = expr     // original Z#: creates a global from inside a function
let name[: Type] [= expr]
let mut name …              // .zxx (mut is accepted and has no further effect)
var name …   const name = expr
let (a, b) = expr           // destructuring
a, b = b, a                 // .zxx: parallel assignment
```

A declaration with a type and no value gets that type's zero: `0`, `0.0`,
`""`, `false`, `[]`, `{}`; others start as `null`.

## Statements

`if c { } [elif|else if c { }] [else { }]`, `while c { }`, `loop { }` (.zxx),
`for x in iterable { }`, `for i, v in list { }`, `for (k, v) in items(m) { }`,
`match v { pattern => expr|block, … }`, `switch v { case 1: …; case 2...5: …;
case 6, 7: …; default: … }` (.zxx), `try { } catch e { }`, `throw expr`,
`return [expr]`, `break`, `continue`, and expression statements. Assignment
operators: `= += -= *= /= %=`, also to `a[i]`, `a[i:j]` and `a.field`.

In `.zxx`, a statement that is only a string (`"Hello\n";`) prints it, and
`"fmt", args…;` prints with `printf` formatting (HolyC). A bare function name
as a statement calls it (`Hello;`).

## Expressions

Precedence, loosest first. Binary operators are left-associative unless noted.

| Level | Operators |
| --- | --- |
| 1 | `x \|> f(args)` — pipe: `f(x, args)` |
| 2 | `c ? a : b` |
| 3 | `??` |
| 4 | `\|\|`, `or` |
| 5 | `&&`, `and` |
| 6 | `==`, `!=` |
| 7 | `<`, `<=`, `>`, `>=`, `in`, `not in` (.zxx) |
| 8 | `a..b` (half-open), `a..=b` (not associative) |
| 9 | `+`, `-` |
| 10 | `*`, `/`, `%`, `@` (.zxx, matrices) |
| 11 | `**` (right-associative; binds *looser* than unary minus: `-2 ** 2` is `4`) |
| 12 | unary `-`, `!`, `not` |
| 13 | postfix: call `f(…)`, index/slice `a[i]`, `a[i:j:k]`, `a[i, j]`, field `.x`, method `.m(…)`, optional chaining `?.`, error propagation `expr?` (.zxx), `Enum::Variant` |

`/` on two ints is floor division (`-7 / 2` is `-4`); `%` follows it.

Primary expressions: literals, names, `[list]`, `{ key: value }` maps,
`(a, b)` tuples, `Name { field: value }` struct literals, `|args| expr` and
`func(args) { body }` lambdas, `match` as an expression, list and map
comprehensions (`[x * x for x in xs if c]`, `{ k: v for k in xs }`, .zxx).

Patterns in `match`: literals, ranges `1..5`, alternatives `a | b`, `_`,
`Some(x)`, `Ok(x)` / `Err(e)`, and enum variants with bindings `Rect(w, h)`.

## Types

A type annotation follows `:` (variables, parameters, fields) or `->`
(return types). Grammar:

```
Type     := Atom "?"*
Atom     := Name
          | Name "<" Type ("," Type)* ">"      // list<int>, map<text, int>
          | "[" Type "]"                         // [int]: a list of int
          | "(" Type ("," Type)* ")"             // (int, text): a tuple
```

`>=` directly after a type list is split (`map<text, list<int>>= {}`).

| Names | Meaning |
| --- | --- |
| `int`, `float`, `bool`, `string`, `color`, `list`, `map` | **Original**: the value is converted (`let n: int = 2.9` is `2`, `let s: string = 5` is `"5"`). |
| `u8`…`i32`, `I64`, `F64`, `U0`, `Bool`, `str`, `String` | **Original** Rust/HolyC names, also converting (`u8` wraps). |
| `text`, `number`, `byte`, `char`, `any`, `dynamic`, `void`, `null` | **Checked**: the wrong type is a run-time error (`expected text, got int (5)`). |
| `list<T>`, `array<T>`, `[T]`, `map<K, V>`, `(A, B)`, `tuple<…>`, `T?`, `optional<T>`, `result<T, E>` | **Checked**, element by element. |
| a struct, enum or trait name | **Checked**: a value of the struct, a variant of the enum, or a type that implements the trait. |
| any other name | **Not checked** (host types, and annotations in older programs). |

An `int` is accepted where a `float` is expected. `null` is accepted by every
annotation (as before). Types are also values at run time; see
[the library reference](site/library-builtins.md#types) for `type_of`, `is`,
`as`, `try_as` and `Type`.

## Native declarations (stage 3)

Only in `.zxx`, as statements:

```
name: Type [= expr]      // every type name is checked, none converts
name := expr             // the type is the first value's (none for null)
```

A statement starting `name :` is a declaration only when what follows
parses as a type and is followed by `=` or the end of the line; otherwise
it's parsed as before (so `default: …` in a `switch` is unchanged).
`default` and reserved words are never declared this way.

A variable declared either way, or with `let name: T` where `T` is a
checked type, **keeps its type**: assigning a value that doesn't match is a
run-time error (`` `n` is int, can't assign string (x) ``); an int assigned
to a float variable is stored as a float. Assigning into an element
(`xs[0] = …`) is not checked. Re-declaring the name (`let n = …`) replaces
the variable and its type.

`func name(args) -> Type` is the preferred function form in `.zxx`; `fn` is
still accepted.

### Types in expressions

In `.zxx`, a generic type name (`list`, `array`, `map`, `optional`,
`Option`, `result`, `Result`, `tuple`) followed by `<` is read as a type
when the whole `<…>` parses as one and is followed by `)`, `,`, `]`, `}` or
the end of the line; likewise a built-in type name followed by `?` and one
of those (`int?`). It evaluates to `Type("…")`. Anything else is parsed as
before, so `a < b` and `f(x < y, z > w)` are comparisons. A tuple of types
`(int, text)` is a tuple type wherever a type is expected.

### Checks before running

When a program loads, every declaration whose value is made only of
literals (numbers, strings, `null`, colours, `-`, and lists and maps of
those) is checked against its checked type, in all functions, methods,
handlers and top-level statements, whether or not they run. All failures
are reported together. Declarations using the original converting names,
and values that aren't literals, are checked when they run.

## Still proposed

* Checking non-literal initialisers and call arguments before running
  (type inference across expressions).
* Checking assignments into elements of typed lists and maps.

Rust- and Python-style forms (`fn`, `let mut`, `trait`, `?`, comprehensions)
remain accepted in `.zxx`; none becomes mandatory.

## Compatibility suite

`crates/zsharpend/tests/compat/` holds one program per file:

* `NAME.zsp` / `NAME.zxx` with `NAME.out`: the program's exact output.
* `NAME.zxx` with `NAME.err`: loading or running must fail with an error
  containing that text.
* Files starting with `_` are helpers (headers, `#include`d code).

`cargo test -p zsharpend --test compat` runs them all. Every roadmap stage
keeps them passing; a change that alters a recorded output is a language
change and must say so. To record a new fixture, add the program and run
`UPDATE_COMPAT=1 cargo test -p zsharpend --test compat`, then read the
`.out` it wrote before committing.
