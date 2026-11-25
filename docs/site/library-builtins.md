# Built-in functions

These are always available. Every lower-case built-in is also a method on its first argument — `upper(s)` is `s.upper()` — and works with the pipe, `s |> upper()`.

## Output

::: function Printl(values...)
Writes the values separated by spaces, then a line break.
:::

::: function println(values...)
The same as `Printl`.
:::

::: function Print(values...)
Writes the values separated by spaces, without a line break.
:::

::: function print(values...)
The same as `Print`.
:::

::: function printf(format, values...)
Writes `values` with a printf-style `format` (see [printing statements](reference-statements.md#printing-statements)). The statement form `"%d\n", n;` does the same.
:::

::: function format(format, values...)
Returns the formatted text instead of writing it.
:::

## Conversion and inspection

::: function str(x)
`x` as text, as `Printl` writes it.
:::

::: function to_string(x)
The same as `str`.
:::

::: function int(x)
A whole number from a number (rounded down) or numeric text. Fails on other text.
:::

::: function to_int(x)
The same as `int`.
:::

::: function float(x)
A float from a number or numeric text.
:::

::: function to_float(x)
The same as `float`.
:::

::: function bool(x)
Whether `x` counts as true: everything except `null`, `false`, `0`, `0.0`, `""`, `[]` and `{}`.
:::

::: function list(x)
A list from a range, a string (its characters), a map (its keys) or a list (a copy).
:::

::: function type(x)
The name of `x`'s type as text, as it has always been: `"int"`, `"string"`, `"list"`, a struct or variant's name… `type_of(x)` gives a type object instead.
:::

::: function len(x)
The length of a list, map, string (characters) or range.
:::

::: function range(stop)
`0..stop`; `range(start, stop)` is `start..stop`.
:::

::: function assert(condition, message=None)
Fails with `message` when `condition` is false.
:::

## Types

Types are values too. `int`, `float`, `bool`, `str` and `list` (the conversion functions), `text`, `number`, `byte`, `char`, `dynamic`, `void`, `null`, struct and enum names, and types written as text (`"map<text, list<int>>"`) can all be passed where a type is expected. See [Types and annotations](tutorial-types.md#type-annotations) for the names and what annotations check.

::: data text
The type of strings. `string`, `str` and `String` are other names for it.
:::

::: data number
An `int` or a `float`.
:::

::: data byte
An `int` from 0 to 255 (`u8`).
:::

::: data char
A string of exactly one character.
:::

::: data dynamic
Any value, checked when it's used rather than when it's stored (`any` is the same type).
:::

::: data void
The type of `null`, for functions that return nothing (`U0`).
:::

::: function Type(spec)
A type object from `spec`: `Type("list<int>")`, `Type(int)`. It prints as its canonical name and has `.name`, `.base` (`"map"` for `map<text, int>`), `.params` (its type parameters, as types), and methods `.accepts(x)`, `.convert(x)` and `.try_convert(x)`.
:::

::: function type_of(x)
`x`'s type as a type object: `type_of("hi") == text`. It also compares equal to the type's name, old or new: `type_of("hi") == "string"`.
:::

::: function is(x, T)
Whether `x` has type `T`. Lists and maps are checked element by element (`[1, 2].is("list<int>")`), enum variants are their enum, and a struct is each trait it implements. Also a method: `x.is(number)`.
:::

::: function as(x, T)
`x` converted to `T`, or an error saying why it can't be: `"42".as(int)` is `42`, `"forty".as(int)` fails, `300.as(byte)` fails rather than wrapping, `3.5.as(int)` fails rather than rounding. Containers convert each element: `["1", "2"].as("list<int>")`.
:::

::: function try_as(x, T)
Like `as`, but returns `Ok(value)` or `Err(message)` instead of failing.
:::

::: function is_type(x, name)
Whether `x` has the type `name`, ignoring case and accepting the older names (`"String"`, `"Integer"`, `"numeric"`, `"none"`).
:::

::: function is_number(x)
`x.is(number)`.
:::

::: function is_int(x)
`x.is(int)`.
:::

::: function is_float(x)
`x.is(float)`.
:::

::: function is_string(x)
`x.is(text)`.
:::

::: function is_bool(x)
`x.is(bool)`.
:::

::: function is_list(x)
`x.is(list)`.
:::

::: function is_map(x)
`x.is(map)`.
:::

::: function is_null(x)
`x == null`.
:::

## Numbers

```zxx run
fn Main() {
    Printl(abs(-3), floor(2.7), ceil(2.1), round(2.5), sqrt(16))
    Printl(min(4, 2, 8), max([4, 2, 8]), clamp(15, 0, 10), lerp(0, 10, 0.25))
    Printl(pow(2, 0.5), atan2(1, 1) * 4, PI, TAU)
}
```

::: function abs(x)
The absolute value; an int stays an int.
:::

::: function floor(x)
Rounded down, as an int.
:::

::: function ceil(x)
Rounded up, as an int.
:::

::: function round(x)
Rounded to the nearest, as an int.
:::

::: function sqrt(x)
The square root, a float.
:::

::: function pow(x, y)
`x` to the power `y`, a float (`x ** y` keeps ints).
:::

::: function sin(x)
Sine of `x` radians. Also `cos`, `tan`.
:::

::: function cos(x)
Cosine of `x` radians.
:::

::: function tan(x)
Tangent of `x` radians.
:::

::: function atan2(y, x)
The angle of the point `(x, y)`, in radians.
:::

::: function min(values...)
The smallest value; with one list argument, the smallest item.
:::

::: function max(values...)
The largest value; with one list argument, the largest item.
:::

::: function clamp(x, lo, hi)
`x` limited to `lo..hi`.
:::

::: function lerp(a, b, t)
`a + (b - a) * t`.
:::

::: function random(lo=None, hi=None)
A float in `0..1`, or an int from `lo` to `hi` inclusive. The host can seed it (`whetstone run --seed`).
:::

::: data PI
π, 3.14159…
:::

::: data TAU
2π.
:::

## Colours

::: function rgb(r, g, b)
A colour from channels 0–255.
:::

::: function rgba(r, g, b, a)
With alpha: 0–1 as a float, or 0–255 as an int.
:::

::: function hsl(h, s, l)
From hue (degrees), saturation and lightness (0–1).
:::

::: function mix(a, b, t)
Two colours (or numbers) blended: `t` = 0 gives `a`, 1 gives `b`.
:::

::: function with_alpha(colour, a)
The colour with alpha `a` (0–1). A colour's channels are fields: `.r .g .b .a`.
:::

## Lists and sequences

```zxx run
fn Main() {
    let xs = [5, 3, 8, 1]
    Printl(xs.sort(), xs.reverse(), xs.map(|x| x * 2), xs.filter(|x| x > 2))
    Printl(xs.reduce(|a, b| a + b, 0), sum(xs), first(xs), last(xs), slice(xs, 1, 3))
    Printl(zip([1, 2], ["a", "b"]), enumerate(["x", "y"]), any([0, 1]), all([0, 1]))
}
```

::: function push(xs, x)
Adds `x` to the end of `xs` (changing it) and returns `xs`.
:::

::: function pop(xs)
Removes and returns the last item (or `null`).
:::

::: function map(xs, f)
`f` applied to each item.
:::

::: function filter(xs, f)
The items for which `f` returns true.
:::

::: function reduce(xs, f, start)
`f(f(start, x0), x1)…`.
:::

::: function sum(xs)
The total; an int if every item is.
:::

::: function sort(xs, key=None)
A sorted copy; `key(x)` sorts by what it returns.
:::

::: function reverse(xs)
A reversed copy (strings too).
:::

::: function join(xs, sep="")
The items as text, joined by `sep`.
:::

::: function first(xs)
The first item, or `null`.
:::

::: function last(xs)
The last item, or `null`.
:::

::: function slice(xs, start, stop=None)
Items from `start` up to `stop` (the same as `xs[start:stop]`).
:::

::: function zip(a, b)
Pairs `[a0, b0], [a1, b1]…`, as long as the shorter list.
:::

::: function enumerate(xs)
Pairs `[0, x0], [1, x1]…`.
:::

::: function any(xs)
Whether any item is true.
:::

::: function all(xs)
Whether every item is true.
:::

::: function contains(xs, x)
Whether `x` is in a list, a key of a map, or text in a string (also `x in xs`).
:::

::: function index_of(xs, x)
Where `x` is in a list or string, or `-1`.
:::

::: function remove(xs, i)
Removes and returns item `i` of a list, or key `i` of a map.
:::

## Strings

```zxx run
fn Main() {
    let s = "  Zsharpend, Whetstone  "
    Printl(s.trim().split(", "), s.upper(), s.trim().lower().replace("e", "3"))
    Printl("tea".starts_with("te"), "tea".ends_with("x"), "ab".repeat(3), "abc".chars())
}
```

::: function upper(s)
Upper case.
:::

::: function lower(s)
Lower case.
:::

::: function trim(s)
Without spaces at either end.
:::

::: function split(s, sep=" ")
The parts between each `sep`.
:::

::: function starts_with(s, prefix)
Whether `s` begins with `prefix`.
:::

::: function ends_with(s, suffix)
Whether `s` ends with `suffix`.
:::

::: function replace(s, old, new)
Every `old` replaced by `new`.
:::

::: function chars(s)
A list of its characters.
:::

::: function repeat(s, n)
`s` repeated `n` times (also `s * n`).
:::

## Maps

::: function keys(m)
The keys, in order (also a struct's field names).
:::

::: function values(m)
The values, in key order.
:::

::: function items(m)
`[key, value]` pairs, in key order.
:::

::: function has(m, key)
Whether `key` is in `m` (also `key in m`).
:::

::: function get(m, key, default=None)
The value, or `default`.
:::

## Results and options

Rust-style values for things that can fail or be missing. See [Errors as values](tutorial-types.md#errors-as-values).

::: function Ok(value)
A successful result; `.value` holds it.
:::

::: function Err(error)
A failed result; `.error` holds the reason.
:::

::: function Some(x)
`x` itself: present values need no wrapping (`None` is `null`). Matches non-null values in `match`.
:::

::: function is_ok(r)
Whether `r` is `Ok`.
:::

::: function is_err(r)
Whether `r` is `Err`.
:::

::: function is_some(x)
Whether `x` isn't `null`.
:::

::: function is_none(x)
Whether `x` is `null`.
:::

::: function unwrap(r)
The value of `Ok(v)`, or `x` when it isn't null; fails on `Err` or `None`.
:::

::: function unwrap_or(r, default)
Like `unwrap`, but `default` instead of failing.
:::

## Z# and drawing

::: function NVec2(x, y)
A `Vec2` struct (Z#). Vec2s add, subtract and scale.
:::

::: function vec2(x, y)
The same as `NVec2`.
:::

::: class Canvas(width, height)
An image to draw on. See [Canvas — 2D drawing](library-canvas.md).
:::
