# Canvas — 2D drawing

::: module canvas — 2D drawing (the Canvas type)

`Canvas(width, height)` makes an image, up to 4096 × 4096, to draw on with anti-aliased shapes. A program that keeps it in a global named `canvas` or `screen` can save it: `whetstone run --png out.png`.

```zxx run
fn Main() {
    global let canvas = Canvas(420, 150)
    canvas.clear(#fbfaf7)
    canvas.round_rect(16, 16, 120, 118, 18, #0b5fa5)
    canvas.circle(210, 75, 50, with_alpha(#a3322a, 0.85))
    canvas.stroke_circle(240, 75, 50, 5, #2c6e3a)
    canvas.polygon([[320, 130], [400, 130], [360, 30]], #e8b45c)
    canvas.text(30, 60, "Z#END", 14, #ffffff)
}
```

::: class Canvas(width, height)
An image. Positions are pixels from the top-left; colours are colour values.
:::

::: method Canvas.clear(colour)
Fills everything.
:::

::: method Canvas.rect(x, y, w, h, colour)
A filled rectangle. `stroke_rect(x, y, w, h, width, colour)` draws its outline.
:::

::: method Canvas.round_rect(x, y, w, h, radius, colour)
A filled rectangle with rounded corners.
:::

::: method Canvas.circle(cx, cy, r, colour)
A filled circle. `stroke_circle(cx, cy, r, width, colour)` draws its outline.
:::

::: method Canvas.line(x1, y1, x2, y2, width, colour)
A line.
:::

::: method Canvas.polygon(points, colour)
A filled shape through `points` (`[x, y]` lists or `Vec2`s). `polyline(points, width, colour)` draws an open line.
:::

::: method Canvas.text(x, y, text, size, colour)
Text in the built-in 5×7 pixel font (capitals, digits, punctuation), `size` pixels tall, rounded to a multiple of 7.
:::

::: method Canvas.pixel(x, y)
The colour of one pixel.
:::

::: method Canvas.shade(fragment, x, y, w, h, uniforms)
Fills an area with a fragment shader from a `.zsg` file (see below).
:::

## GPU code

`.zsg` files hold Z#++ GPU code — compute `kernel`s and `fragment` shaders — that compiles to WGSL and runs on the CPU here. Import one and its entry points become globals:

```zsg include=examples/gpu/glow.zsg
```

```zxx run include=examples/gpu/main.zxx
```

The full GPU language is described in the Z#++ documentation in the StonedGem repository; it's unchanged in Zsharpend.
