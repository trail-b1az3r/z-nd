# spire — the Zsharpend edition

An interactive shell written in Zsharpend (`.zxx`), using the [`os`](../../docs/site/library-os.md) script extender for the terminal, processes and files. It's a port of [spire](https://github.com/trail-b1az3r/spire-shell), the C shell.

```sh
cargo build --release -p whetstone
examples/spire/spire                     # interactive
examples/spire/spire -c 'ls | wc -l'     # one command
examples/spire/spire script.fish a b     # a script, with $argv
```

## What it does

- **Both syntaxes**: `if … else if … end` and `if …; then … elif … fi`; `while`/`until`, `for` (`end` or `do … done`), `switch`/`case`, `case … esac`, `function f -a x … end` and `f() { … }`, `begin … end`, `{ … }`, `cmd; and …`, `cmd; or …`, `not`/`!`, `&&`, `||`, `|`, `|&`, `&`.
- **Expansion**: quotes and escapes, `$var`, fish lists (`$list[1]`, `$list[-1]`, `$list[2..3]`), `${var:-default}`, `${#var}`, `$#`, `$@`, `$1`, `(cmd)` and `$(cmd)`, `{a,b}` and `{1..5}`, `~`, `*` `?` `[…]` `**` globs.
- **Redirection**: `>`, `>>`, `<`, `2>`, `2>>`, `&>`, `2>&1`, `>&2`, `<<<`, on commands and on blocks.
- **Builtins**: `cd pwd echo set export unset alias abbr source read type history functions status set_color ls ll la count contains string math jobs fg return exit break continue emit fish_add_path prompt_pwd fish_vcs_prompt bind command builtin eval isatty help true false`.
- **The prompt**: `user ❯ ` then what you type; the `❯` turns red after a failure, the git branch shows on the right inside a repository, and each new prompt fades in (`set spire_animate no` turns that off).
- **Editing**: fish's colors for syntax highlighting (unknown commands red, valid paths underlined), autosuggestions from history (→ accepts, Alt-→ one word), ↑/↓ history search by prefix, Ctrl-R, Tab completion of commands, files and `$variables` with a menu (Tab again to choose), abbreviations expanding on space, "did you mean" for typos, and `cd`-less directory names.
- **Fish configs**: at startup it runs `~/.config/fish/conf.d/*.fish` and `config.fish`, autoloads `functions/NAME.fish`, and uses your `fish_prompt`, `fish_right_prompt` and `fish_greeting`. Functions with `--on-event fish_prompt/fish_preexec/fish_postexec` or `--on-variable PWD` fire.
- **ls, ll, la** behave like fish's: color and `-F` indicators on a terminal, plain when piped.

## Files

| File | |
|---|---|
| `src/main.zxx` | start-up, the prompt loop, history, fish config loading |
| `src/lex.zxx`, `src/parse.zxx` | words and operators; one parser for both syntaxes |
| `src/vars.zxx`, `src/expand.zxx` | variables (fish-style lists) and word expansion |
| `src/exec.zxx` | running commands, pipelines, functions, redirections |
| `src/builtins.zxx` | the builtins |
| `src/edit.zxx` | the line editor, highlighting, completion, prompt |
| `tests/basics.fish` | run by `cargo test` (crates/whetstone/tests/spire.rs) |

## Compared with the C spire

This edition covers the everyday shell: it doesn't have the C version's `trap`, `getopts`, `printf` (it uses the system's), process substitution, here-documents, `[[ ]]`, arithmetic `$(( ))`, `${var#pattern}`-style operators, user-defined `complete` rules, or option completion from `--help`. A pipeline that contains a builtin or function runs its stages one after another rather than concurrently.
