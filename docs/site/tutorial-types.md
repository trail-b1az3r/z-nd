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
