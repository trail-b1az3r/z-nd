# Statements

## Declarations

```text
let name = value          let name: type = value
var name = value          const NAME = value
let mut name = value      // Rust spelling; the same as `let`
int name = value          // Z#: the type first
global let name = value   // inside a function: declares a global
let (a, b) = pair         // unpack a list or a struct's fields
let a, b = 1, 2
```

`const` can't be reassigned (Whetstone reports it before the program runs). In Z# style, assigning to a name that doesn't exist declares it; Whetstone warns about that in `.zxx` files.

## Assignment

`target op= value`, where `op=` is one of `= += -= *= /= %=` and the target is a variable, an element (`xs[i]`, `grid[i, j]`, `m["k"]`), a slice (`xs[1:3] = [...]`) or a field. `a, b = b, a` assigns several at once, evaluating the right side first.

## if, elif, else

```text
if cond { … } elif other { … } else { … }      // `else if` works too
```

## Loops

```text
while cond { … }
loop { … }                       // until `break`
for x in iterable { … }
for i, x in list { … }           // Z#++: the index, then the item
for (k, v) in items(map) { … }   // .zxx: unpack each item
```

`for` walks lists, ranges, a number (`for i in 5` is `0..5`), maps (their keys), strings (their characters) and array-like objects. `break` and `continue` work in every loop.

## match

`match value { pattern => result, … }` is an expression. Patterns, tried in order:

| Pattern | Matches |
|---|---|
| `0`, `"x"`, `#fff` | that value |
| `1..5`, `1..=5` | a number in the range |
| `a \| b` | either |
| `_` | anything |
| `Circle(r)`, `Shape::Rect(w, h)` | that enum variant (or struct), binding its fields to lower-case names; `_` skips one |
| `Ok(v)`, `Err(e)` | a result |
| `Some(x)` | anything but `null`, bound to `x` |
| `pattern if cond` | the pattern, when `cond` holds |

An arm is an expression or a block `{ … }`.

## switch

HolyC's `switch` compares a value with `case` labels; ranges use three dots and include both ends:

```zxx run
fn grade(n) {
    switch n {
        case 90...100: "A";
        case 75...89: "B";
        case 0, 1, 2: "barely";
        default: "C";
    }
}

fn Main() {
    for n in [95, 80, 1, 60] { grade(n); " "; }
    "\n";
}
```

Cases don't fall through to the next one; `break` leaves the `switch` early.

## Printing statements

A string literal alone prints it, without a line break. With values after it, it's a printf format:

| Format | Prints |
|---|---|
| `%d %i %u` | an integer (`%+d` with a sign, `%05d` zero-padded) |
| `%x %X %o` | hex, upper-case hex, octal |
| `%f %.2f %e %g` | a float |
| `%s %-10s %.3s` | text (left-aligned in 10 columns; first 3 characters) |
| `%c` | a character |
| `%%` | a percent sign |

Whetstone checks that the number of values matches the format.

## Bare calls

A function's name on its own line calls it with no arguments (HolyC): `Main;` or `Setup`.

## try, throw

```text
try { … } catch e { … }
throw value
```

`throw` raises any value; `try` catches it and errors from the language. Sandbox limits (time, steps, depth, size) can't be caught.

## Declarations at the top level

| Item | Form |
|---|---|
| function | `fn name(params) -> type { … }`, `func …`, or `I64 Name(I64 x) { … }` |
| struct | `struct Point { x: float, y: float }` |
| enum | `enum Shape { Circle(r), Rect(w, h), Empty }` |
| trait | `trait Area { fn area(self) -> float  fn twice(self) { … } }` |
| impl | `impl Point { … }`, `impl Area for Point { … }` |
| event handler | `on event "filter" (params) { … }` |
| import | `import "file.zxx"`, `import name`, `import name as alias`, `from name import a, b as c`, `#include "file"` |
| decorator | `@decorator` before `fn`: the function becomes `decorator(function)` |

```zxx run
fn logged(f) {
    return |x| {
        "calling with %d\n", x;
        return f(x)
    }
}

@logged
fn double(x) { return x * 2 }

fn Main() { Printl(double(21)) }
```
