# nd — n-dimensional arrays

::: module nd — n-dimensional arrays (NumPy-style)

`nd` gives Zsharpend arrays of numbers with any number of dimensions, in the style of NumPy: element-wise arithmetic with broadcasting, comparisons that make masks, slicing and masked selection, reductions along an axis, and `@` for matrix multiplication. Arrays hold 64-bit floats in row-major order, up to ten million elements.

```zxx run
import nd

fn Main() {
    let a = nd.arange(6).reshape(2, 3)
    Printl(a)
    Printl(a * 2 + 1)
    Printl(a.sum(axis=0), a @ a.T)
}
```

`nd` is a [script extender](extending.md#script-extenders): a project lists it in its `.wtf` (`[extenders] use = ["nd"]`) and imports it with `import nd`.

## Creating arrays

::: function nd.array(values)
An array from nested lists of numbers (they must be rectangular), or from a number or another array.
:::

::: function nd.zeros(shape)
An array of zeros. A shape is a number, a list or a tuple: `nd.zeros(3)`, `nd.zeros((2, 3))`.
:::

::: function nd.ones(shape)
An array of ones.
:::

::: function nd.full(shape..., value)
An array filled with `value`: `nd.full(2, 3, 7)`.
:::

::: function nd.arange(start=0, stop, step=1)
Evenly spaced values from `start` up to (not including) `stop`.
:::

::: function nd.linspace(start, stop, num=50)
`num` evenly spaced values from `start` to `stop` inclusive.
:::

::: function nd.eye(n)
The `n`×`n` identity matrix.
:::

::: function nd.random(shape)
Random values in `0..1` (from the interpreter's generator; seed it with `nd.seed`).
:::

::: function nd.seed(n)
Seed the random generator, so runs repeat.
:::

```zxx run
import nd

fn Main() {
    Printl(nd.array([[1, 2], [3, 4]]))
    Printl(nd.linspace(0, 1, 5), nd.full(2, 2, 7))
    nd.seed(7)
    Printl(nd.random(3).shape, nd.eye(2))
}
```

## Arithmetic and broadcasting

`+ - * / % **` work element by element between arrays, or between an array and a number. Shapes are **broadcast** as NumPy does: they're aligned from the right, and a dimension of 1 (or a missing one) stretches to match. Comparisons (`< <= > >= == !=`) give arrays of 1 and 0 — masks.

```zxx run
import nd

fn Main() {
    let m = nd.arange(6).reshape(2, 3)
    let row = nd.array([10, 20, 30])
    let col = nd.array([[100], [200]])
    Printl(m + row)
    Printl(m + col)
    Printl(m > 2)
}
```

Shapes that can't broadcast are an error:

```zxx error
import nd
fn Main() { nd.ones((2, 3)) + nd.ones((3, 2)) }
```

`a @ b` multiplies matrices (and vectors): a 2-D array by a 2-D or 1-D one, or two vectors (their dot product).

::: function nd.dot(a, b)
The same as `a @ b`.
:::

::: function nd.matmul(a, b)
The same as `a @ b`.
:::

## Indexing and slicing

`a[i]` takes along the first axis; `a[i, j]` along two; slices keep an axis, indexes drop it. A mask (an array of 1s and 0s of the same shape) selects elements, and works on the left of `=` too.

```zxx run
import nd

fn Main() {
    let a = nd.arange(12).reshape(3, 4)
    Printl(a[1], a[1, 2], a[:, 0], a[::2, 1:3])
    Printl(a[a % 5 == 0])
    a[a > 8] = -1
    a[0] = 0
    Printl(a)
}
```

## The ndarray type

::: class ndarray
What `nd` functions return. `Printl` shows it as `array([...])`.
:::

::: data ndarray.shape
The size of each dimension, as a list.
:::

::: data ndarray.ndim
The number of dimensions.
:::

::: data ndarray.size
The number of elements.
:::

::: data ndarray.T
The transpose (dimensions reversed).
:::

::: method ndarray.reshape(shape...)
The same values in a new shape; one dimension may be `-1` (worked out).
:::

::: method ndarray.transpose()
The same as `.T`.
:::

::: method ndarray.tolist()
Nested lists of floats.
:::

::: method ndarray.copy()
A copy (arrays are shared when assigned).
:::

::: method ndarray.apply(f)
A new array with the script function `f` applied to each element.
:::

## Reductions

Each takes an optional `axis`: without it, the whole array reduces to one number; with it, that axis disappears. Each is also a method: `a.sum(axis=0)`.

::: function nd.sum(a, axis=None)
The total.
:::

::: function nd.prod(a, axis=None)
The product.
:::

::: function nd.mean(a, axis=None)
The mean.
:::

::: function nd.min(a, axis=None)
The smallest value.
:::

::: function nd.max(a, axis=None)
The largest value.
:::

::: function nd.var(a, axis=None)
The variance (population).
:::

::: function nd.std(a, axis=None)
The standard deviation (population).
:::

::: function nd.argmin(a, axis=None)
Where the smallest value is (in the flattened array, or along the axis).
:::

::: function nd.argmax(a, axis=None)
Where the largest value is.
:::

```zxx run
import nd

fn Main() {
    let scores = nd.array([[70, 85, 90], [60, 75, 95]])
    Printl(scores.mean(), scores.mean(axis=1), scores.max(axis=0))
    Printl(scores.argmax(), nd.std(scores))
}
```

## Element-wise functions

Each returns a new array (and works on plain numbers too). Each is also a method: `a.sqrt()`.

::: function nd.sqrt(a)
Square roots.
:::

::: function nd.exp(a)
`e` to the power of each element.
:::

::: function nd.log(a)
Natural logarithms. Also `nd.log10`.
:::

::: function nd.log10(a)
Base-10 logarithms.
:::

::: function nd.sin(a)
Sines (radians). Also `nd.cos`, `nd.tan`, `nd.tanh`.
:::

::: function nd.cos(a)
Cosines.
:::

::: function nd.tan(a)
Tangents.
:::

::: function nd.tanh(a)
Hyperbolic tangents.
:::

::: function nd.abs(a)
Absolute values.
:::

::: function nd.floor(a)
Rounded down. Also `nd.ceil`, `nd.round`.
:::

::: function nd.ceil(a)
Rounded up.
:::

::: function nd.round(a)
Rounded to the nearest.
:::

::: function nd.sign(a)
-1, 0 or 1.
:::

::: function nd.sigmoid(a)
`1 / (1 + exp(-x))`.
:::

::: function nd.relu(a)
`max(x, 0)`.
:::

::: function nd.clip(a, lo, hi)
Each value limited to `lo..hi`.
:::

## Joining and reshaping

::: function nd.reshape(a, shape...)
The same as `a.reshape(...)`.
:::

::: function nd.transpose(a)
The same as `a.T`.
:::

::: function nd.flatten(a)
A 1-D copy.
:::

::: function nd.concatenate(arrays, axis=0)
Arrays joined along an existing axis (they must match on the others).
:::

::: function nd.stack(arrays)
Arrays of the same shape stacked along a new first axis.
:::

::: function nd.where(cond, x, y)
`x` where `cond` is non-zero, `y` elsewhere (broadcast).
:::

::: function nd.maximum(a, b)
The larger of each pair (broadcast). Also `nd.minimum`.
:::

::: function nd.minimum(a, b)
The smaller of each pair.
:::

::: function nd.allclose(a, b, tolerance=1e-8)
Whether two arrays are equal within a small tolerance.
:::

::: data nd.pi
π.
:::

::: data nd.e
Euler's number.
:::

```zxx run
import nd

fn Main() {
    let a = nd.array([1, -2, 3])
    Printl(nd.where(a > 0, a, 0), nd.maximum(a, 0), nd.concatenate([a, nd.zeros(2)]))
    Printl(nd.stack([a, a * 10]), nd.allclose(nd.array([0.1 + 0.2]), nd.array([0.3])))
}
```

::: note
`nd` arrays are host objects: they live in the interpreter, not in Zsharpend's lists. Use `a.tolist()` to get lists back, and `nd.array(list)` to make an array from one.
:::
