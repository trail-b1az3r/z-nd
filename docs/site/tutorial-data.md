# Data structures

## Lists

Lists hold any values. Index from the front with `0`, from the back with `-1`; slices take runs, as in Python.

```zxx run
fn Main() {
    let xs = [10, 20, 30, 40, 50]
    Printl(xs[0], xs[-1], xs[1:3], xs[::2], xs[::-1])
    xs[1:3] = [21, 22, 23]
    xs.push(60)
    Printl(xs, len(xs))
}
```

## Comprehensions

A comprehension builds a list (or a map) from another sequence:

```zxx run
fn Main() {
    let words = ["tea", "toast", "jam", "tart"]
    Printl([w.upper() for w in words if w.starts_with("t")])
    Printl({ w: len(w) for w in words })
    Printl([x * y for (x, y) in zip([1, 2, 3], [4, 5, 6])])
}
```

## Unpacking

```zxx run
fn min_max(xs) { return (min(xs), max(xs)) }

fn Main() {
    let (lo, hi) = min_max([4, 9, 1, 7])
    Printl(lo, hi)
    lo, hi = hi, lo
    Printl(lo, hi)
    for (i, name) in enumerate(["Ada", "Alan"]) { Printl(i, name) }
}
```

A tuple `(a, b)` is a list of two items.

## Maps

Maps have string keys and are kept sorted by key. `items(m)` gives `[key, value]` pairs.

```zxx run
fn Main() {
    let stock = { tea: 4, jam: 0 }
    stock["toast"] = 12
    for (k, v) in items(stock) { Print("${k}=${v} ") }
    Printl("")
    Printl("jam" in stock, "milk" not in stock, get(stock, "milk", 0))
}
```

## Arrays of numbers

For numeric work, the [nd](library-nd.md) module gives n-dimensional arrays with NumPy-style broadcasting:

```zxx run
import nd

fn Main() {
    let m = nd.arange(1, 7).reshape(2, 3)
    Printl(m)
    Printl(m * 10 - 1)
    Printl(m.sum(axis=0), m.mean())
    Printl(m[m > 3])
}
```

Next: [Structs, enums and traits](tutorial-types.md).
