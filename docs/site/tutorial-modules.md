# Modules

As a program grows, split it into modules. A module is a `.zxx` file (or a folder with a `module.wtf`) whose top-level names other files can import.

## Importing

```text
import geometry               // the module, as `geometry`
import geometry as geo        // under another name
from geometry import area, Point as P
#include "helpers.zxx"        // HolyC-style: include a file's code here
```

Whetstone looks for `import name` in this order:

1. built-in modules, from [script extenders](extending.md#script-extenders) like `nd` and `gfx3d`;
2. `name.zxx` next to the importing file;
3. `modules/name/` in the project (its `module.wtf` names the file to load);
4. modules installed with `whetstone install`.

A module's names stay inside it: importing `geometry` doesn't make its helper functions globals.

```zxx run include=examples/shapes/src/main.zxx
```

The module it imports:

```zxx include=examples/shapes/modules/geometry/geometry.zxx
```

## Installing modules

A module you want in several projects can be installed once:

```sh
whetstone install path/to/geometry     # copies it to ~/.zsharpend/modules/geometry
whetstone list
whetstone uninstall geometry
```

See [Extending Zsharpend](extending.md) for writing modules, extensions and script extenders.
