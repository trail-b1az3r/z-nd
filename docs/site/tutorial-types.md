# Structs, enums and traits

## Structs

A struct is a record with named fields; `impl` gives it methods, whose first parameter is `self`.

```zxx run
struct Point { x: float, y: float }

impl Point {
    fn length(self) { return sqrt(self.x ** 2 + self.y ** 2) }
    fn scaled(self, k) { return Point(self.x * k, self.y * k) }
}

fn Main() {
    let p = Point { x: 3, y: 4 }
    Printl(p.length(), p.scaled(2))
}
```

## Enums

An enum is a value that is one of several variants, each with its own fields — Rust's enums. `match` takes them apart:

```zxx run
enum Shape {
    Circle(r),
    Rect(w, h),
    Empty,
}

fn area(s) {
    return match s {
        Circle(r) => PI * r * r,
        Rect(w, h) => w * h,
        Empty => 0,
    }
}

fn Main() {
    for s in [Shape::Circle(1), Rect(2, 3), Empty] {
        "%-22s %6.2f\n", str(s), area(s);
    }
}
```

`Shape::Circle(1)` and `Circle(1)` are the same. Whetstone warns when a `match` on an enum leaves variants out and has no `_` arm.

## Traits

A trait names methods a type promises to have. Methods with a body are defaults:

```zxx run
trait Describe {
    fn name(self) -> string
    fn describe(self) { return "this is ${self.name()}" }
}

struct Cat { called: string }

impl Describe for Cat {
    fn name(self) { return "a cat called ${self.called}" }
}

fn Main() { Printl(Cat("Tom").describe()) }
```

Leaving out a required method is an error when the program loads:

```zxx error
trait Area { fn area(self) -> float }
struct Square { s: float }
impl Area for Square { }
fn Main() { }
```

## Errors as values

A function that can fail returns `Ok(value)` or `Err(reason)`. The `?` operator unwraps an `Ok` — or returns the `Err` from the function straight away:

```zxx run
fn parse_age(s) {
    if s == "" { return Err("empty") }
    let n = int(s)
    if n < 0 { return Err("negative") }
    return Ok(n)
}

fn total_age(a, b) {
    let x = parse_age(a)?
    let y = parse_age(b)?
    return Ok(x + y)
}

fn Main() {
    Printl(total_age("30", "12"), total_age("30", ""))
    Printl(unwrap_or(total_age("1", "-5"), 0))
    let r = match total_age("2", "2") { Ok(v) => "total ${v}", Err(e) => "failed: ${e}" }
    Printl(r)
}
```

`None` (the same as `null`) and `Some(x)` work the same way for values that may be missing. Exceptions — `throw` and `try … catch` — are there too, for failures nobody should have to check for.

Next: [Modules](tutorial-modules.md).

## Type annotations

A `:` after a variable or parameter, or `->` after a function's parameters, gives a type. The original names convert the value as they always have — `let n: int = 2.9` stores `2`, `let s: string = 5` stores `"5"` — while the newer names check it, and a value of the wrong type is an error:

| Type | Holds |
| --- | --- |
| `int`, `float`, `bool`, `string`, `color`, `list`, `map` | as before (converting) |
| `text` | a string — `let s: text = 5` is an error |
| `number` | an `int` or a `float` |
| `byte`, `char` | an int from 0 to 255; a one-character string |
| `list<T>`, `[T]`, `array<T>` | a list whose elements are all `T` |
| `map<K, V>` | a map whose keys are `K` (always text) and values `V` |
| `(A, B)` | a tuple: a list of exactly those types |
| `T?`, `optional<T>` | `null` or a `T` |
| `result<T, E>` | `Ok(T)` or `Err(E)` |
| `any`, `dynamic` | anything |
| a struct, enum or trait name | a value of it, one of its variants, or a type implementing it |

An `int` is accepted where a `float` is expected. Names that aren't types the program knows are not checked, so annotations in older programs keep working.

```zxx run
struct Point { x: float, y: float }

fn centre(ps: list<Point>) -> Point {
    let n = len(ps)
    return Point { x: sum(ps.map(|p| p.x)) / n, y: sum(ps.map(|p| p.y)) / n }
}

fn Main() {
    let scores: map<text, list<int>> = { ada: [9, 7], alan: [8] }
    let nickname: text? = null
    Printl(centre([Point { x: 0, y: 0 }, Point { x: 4, y: 2 }]), scores, nickname ?? "none")
    let word = "two"
    try { let bad: list<int> = [1, word] } catch e { Printl(e) }
}
```

At run time a type is a value: `type_of` reflects, `is` checks, and `as` / `try_as` convert safely — failing, or returning `Err`, instead of guessing.

```zxx run
fn Main() {
    Printl(type_of(3), type_of("hi") == text, type_of([1]) == "list")
    Printl(3.is(number), "a".is(char), [1, 2].is("list<int>"))
    Printl("42".as(int) + 1, "4.5".as(number), ["1", "2"].as("list<int>"))
    Printl("forty".try_as(int), 300.try_as(byte))
    let t = Type("map<text, list<int>>")
    Printl(t.base, t.params, t.accepts({ a: [1] }))
}
```

## Native declarations

In `.zxx`, a variable can be declared without `let`, Zsharpend's own way:

* `name: Type = value` declares a variable of that type. Every type name is
  checked here, the original ones too: `n: int = 2.5` is an error, where
  `let n: int = 2.5` converts to `2`. A declaration without a value gets the
  type's zero.
* `name := value` declares a variable whose type is its first value's.

Either way the variable keeps its type: assigning a value of another type
is an error (an int assigned to a float variable becomes a float). A `let`
without a type, and the original converting names, don't fix the type.

Generic types can be written directly in expressions: `x.is(list<int>)`,
`["1"].as(list<int>)`, `5.is(int?)`, `(1, "a").is((int, text))`.

Declarations whose value is a literal are checked before the program
starts, so `count: int = "three"` in a function that hasn't run yet is
still reported.

```zxx run
struct Item { name: text, price: float }

func Main() {
    cart: list<Item> = [Item { name: "tea", price: 2 }]
    total := 0.0
    for item in cart { total += item.price }
    label: text = "total"
    Printl(label, total, cart.is(list<Item>))
    try { total = "free" } catch e { Printl(e) }
}
```
