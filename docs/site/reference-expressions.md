# Expressions

## Operator precedence

From loosest to tightest:

| Operators | Meaning |
|---|---|
| `\|>` | pipe: `x \|> f(a)` is `f(x, a)` |
| `c ? a : b` | conditional |
| `??` | `a ?? b`: `a` unless it's null |
| `or`, `\|\|` | either |
| `and`, `&&` | both |
| `==`, `!=` | equal, not equal (by value) |
| `<`, `<=`, `>`, `>=`, `in`, `not in` | compare; membership |
| `..`, `..=` | ranges |
| `+`, `-` | add (joins strings and lists), subtract |
| `*`, `/`, `%`, `@` | multiply (repeats strings), divide, remainder, matrix multiply (arrays) |
| `**` | power (right to left) |
| `-x`, `not x`, `!x` | negate |
| `f(…)`, `a[…]`, `a.b`, `a?.b`, `x?`, `E::V` | call, subscript, field, optional field, try, enum variant |

Dividing two integers gives an integer, rounded down. Host objects such as `nd` arrays define their own operators.

## Calls

Arguments are positional, then keyword: `f(1, 2, scale=3)`. A keyword names a parameter of a script function; built-in and extender functions receive keywords as a final map. A missing argument with no default is `null` (Whetstone warns).

## Subscripts and slices

`a[i]` indexes (negative counts from the end). `a[start:stop:step]` slices lists, strings and ranges with Python's rules; any part can be left out. Several parts separated by commas index nested lists or arrays: `grid[1, 2]`, `m[:, 0]`.

```zxx run
fn Main() {
    let s = "Zsharpend"
    Printl(s[0], s[-1], s[1:6], s[::-1], s[::3])
    let grid = [[1, 2, 3], [4, 5, 6]]
    Printl(grid[1, 2], grid[:, 0])
}
```

## Comprehensions

```text
[expr for name in iterable]
[expr for name in iterable if condition]
[expr for (a, b) in pairs]
{key: value for name in iterable if condition}
```

## The try operator

`expr?` evaluates `expr`; if it's `Err(e)` or `null`, the enclosing function returns that at once; if it's `Ok(v)`, the result is `v`; anything else passes through.

## Lambdas

`|a, b| expr`, `|a| { … }`, `|| expr`, `fn(a) { … }`, `func(a) { … }`. They capture the variables around them.

## Strings

`"…${expr}…"` interpolates any expression, including comprehensions: `"${[x * 2 for x in xs]}"`.
