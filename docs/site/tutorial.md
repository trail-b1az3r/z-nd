# Tutorial

This tutorial introduces Zsharpend by example. It assumes you've programmed before, in any language. Every example on these pages was run when the page was built; what's shown under it is its real output.

## Using Whetstone

Zsharpend programs are compiled and run by Whetstone. Build it from the repository:

```sh
git clone https://github.com/trail-b1az3r/z-nd
cd z-nd
cargo install --path crates/whetstone
```

A project is a folder with a `.wtf` file — the Whetstone Translation File, which says how to compile it — and its `.zxx` sources:

```sh
whetstone new hello      # creates hello/hello.wtf and hello/src/main.zxx
cd hello
whetstone run            # check, build and run
whetstone build          # writes build/hello.wsb, a bundle that runs anywhere
```

## A first program

A program starts at `Main()`. Statements outside functions also run, first, in order.

```zxx run
fn Main() {
    let name = "world"
    Printl("Hello, ${name}!")
    for i in 1..=3 {
        Printl("${i} squared is ${i ** 2}")
    }
}
```

Strings interpolate `${…}`; `Printl` writes its values and a line break.

## Numbers, strings and comments

```zxx run
fn Main() {
    Printl(7 / 2, 7 / 2.0, 7 % 3, 2 ** 10)       // ints stay ints
    Printl("ha" * 3, "Zsharpend".upper(), len("héllo"))
    let price = 4.5
    "price: %.2f, items: %03d\n", price, 7;     // a HolyC print statement
}
```

`//` starts a comment; `/* … */` spans lines. A string literal on its own line prints it — with values after it, it's a printf-style format, as in HolyC.

## Control flow

`if`, `elif` and `else` take conditions without parentheses; bodies are always in braces.

```zxx run
fn describe(n) {
    if n < 0 {
        return "negative"
    } elif n == 0 {
        return "zero"
    } else {
        return "positive"
    }
}

fn Main() {
    for n in [-3, 0, 8] { Printl(n, describe(n)) }
    var total = 0
    loop {
        total += 7
        if total > 30 { break }
    }
    Printl("total", total)
}
```

`for` walks lists, ranges (`0..5`, `0..=5`), maps' keys and strings. `while cond { }` and `loop { }` (forever, until `break`) repeat.

## Functions

`fn` (or Z#'s `func`) defines a function. Parameters can have defaults, and calls can name them:

```zxx run
fn box(width, height = 1, label = "box") {
    return "${label}: ${width}×${height}"
}

fn Main() {
    Printl(box(3))
    Printl(box(3, 2))
    Printl(box(width=5, label="wide"))
}
```

Lambdas are `|x| x * 2` (or `fn(x) { … }` for longer ones). Every built-in is also a method on its first argument, and `|>` pipes a value into a call:

```zxx run
fn Main() {
    let scores = [72, 95, 58, 88]
    Printl(scores.filter(|s| s >= 60).sort())
    Printl(scores |> map(|s| s / 10) |> sum())
}
```

Next: [Data structures](tutorial-data.md).
