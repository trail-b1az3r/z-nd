## ZAP package manager

ZAP is a local-first package manager for Zsharpend. This first implementation provides project initialization, run/check via Whetstone, local dependency path references, a lock file, dependency removal/listing, and copying package folders into a local packages store.

### Commands
- `zap init [name]` creates `zap.json`, `zap.lock`, and `src/main.zxx`.
- `zap run [file]` invokes Whetstone to run the entry script.
- `zap check` invokes Whetstone static checks.
- `zap add <name> <local-folder>` records a local dependency path.
- `zap remove <name>` removes a dependency reference.
- `zap list` shows project metadata and dependencies.
- `zap install <folder>` copies a package into `$ZSHARPEND_HOME/packages` (default `~/.zsharpend/packages`).

Manifest example: `{"name":"demo","version":"0.1.0","entry":"src/main.zxx","description":"","dependencies":{}}`

Dependencies are currently local path entries only. ZAP does not yet resolve dependency imports, download registry packages, verify signatures, or compile a remote package registry. Local dependency paths are trusted, not sandboxed.
