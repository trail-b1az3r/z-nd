# Zsharpend 0.1 documentation

Zsharpend is a small, sandboxed scripting language that grew out of Z#++ (StonedGem's scripting language) and Sam-astro's Z#. Its programs are `.zxx` files. It borrows from three languages on purpose: comprehensions, slicing and keyword arguments from Python; enums, traits and the `?` operator from Rust; and HolyC's printing statements and `switch`. It ships with an n-dimensional array module, `nd`, and a 3D renderer, `gfx3d`, and it's compiled and run by **Whetstone**, which checks a whole program before it runs.

```zxx run
import nd

enum Reading { Celsius(v), Fahrenheit(v) }

fn to_c(r) {
    return match r {
        Celsius(v) => v,
        Fahrenheit(v) => (v - 32) * 5 / 9.0,
    }
}

fn Main() {
    let readings = [Celsius(21.5), Fahrenheit(70.2), Celsius(19.0)]
    let temps = nd.array([to_c(r) for r in readings])
    "mean %.1f°C, spread %.2f\n", temps.mean(), temps.std();
}
```

## Parts of the documentation

| Part | What's in it |
|---|---|
| [What's new in Zsharpend 0.1](whatsnew.md) | What Zsharpend adds to Z#++. |
| [Tutorial](tutorial.md) | Start here: Whetstone, your first program, data, types, modules. |
| [Language Reference](reference.md) | Files, syntax, statements and expressions, exactly. |
| [Library Reference](library.md) | Built-in functions and the `nd`, `gfx3d` and canvas modules. |
| [Whetstone](whetstone.md) | The compiler: commands, checks, bundles. |
| [The .wtf file](wtf.md) | The Whetstone Translation File every project has. |
| [Extending Zsharpend](extending.md) | Modules, extensions and script extenders. |
| [Embedding](embedding.md) | Running Zsharpend from Rust, and in StonedGem. |

## Indices

- [Module Index](modindex.html) — every module
- [General Index](genindex.html) — every function, method, class and module
- [Search](search.html)
