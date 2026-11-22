# Zsharpend & Z#++ VS Code Extension

Provides syntax highlighting, word/keyword completion, hover notes, language configuration, snippets, and a bundled syntax book for the **Zsharpend** and **Z#++** languages in VS Code and VSCodium.

## Supported file extensions

- `.zxx` — Zsharpend
- `.zsp` — Z#++
- `.zsx` — Z#++ variant
- `.zsg` — Z#++ GPU/shader syntax

## Features

- Syntax coloring for comments, strings, numbers, keywords, types, operators, and function names
- Keyword and common built-in completion suggestions
- Context-aware module suggestions after typing `import`
- Hover help for recognized language keywords
- Snippets for functions, variables, enums, conditionals, loops, switch, printing, imports
- Command Palette: **Zsharpend/Z#++: Open Syntax Book**

## Build/package locally

Requirements: Node.js, npm, and VS Code or VSCodium.

```sh
cd vscode-extension
npm install
npm run compile
```

To package as a VSIX (install `vsce` first):

```sh
npx @vscode/vsce package
```

Install the generated VSIX in VS Code/VSCodium using **Extensions → Install from VSIX**.

## Language reference

Open `docs/syntax-book.md` from the extension folder, or use the Command Palette command above.
