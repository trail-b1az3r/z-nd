# os — the operating system

::: module os — terminal, processes, files and environment, for programs that leave the sandbox

Zsharpend scripts run in a sandbox: no files, no processes, a time and step budget. A project that lists the `os` [script extender](extending.md#script-extenders) opts out of that and becomes a real program — it can drive the terminal, run commands, read and write files and change the environment. Its time and step limits are lifted, and calls may nest deeper (up to 256). That's enough to write real command-line tools — even an interactive shell.

```toml
[extenders]
use = ["os"]
```

```text
import os

fn Main() {
    os.write("\e[1mhello\e[0m\n")                       // straight to the terminal
    let r = os.run([["ls", "-l"], ["wc", "-l"]], {})     // a pipeline
    let n = os.run([["git", "rev-parse", "--short", "HEAD"]], {capture: true})
    Printl(r.status, n.output.trim(), os.args())
}
```

Run it with arguments after `--`: `whetstone run . -- one two`. Anything `Printl` buffered is written out before each `os` call that touches the terminal or a process, so output stays in order.

## Terminal

::: function os.write(values...)
Writes the values to standard output now (no line break added).
:::
::: function os.ewrite(values...)
The same, to standard error.
:::
::: function os.flush()
Writes out whatever `Printl` and `Print` have buffered.
:::
::: function os.raw(on)
Switches the terminal into raw mode (keys arrive one at a time, unechoed) or back.
:::
::: function os.read_key()
Waits for one key and returns it as text: a character, a control character (`"\x01"` for Ctrl-A), or a whole escape sequence (`"\e[A"` for ↑). `""` at the end of input.
:::
::: function os.key_waiting(ms=0)
Whether a key is ready, waiting up to `ms` milliseconds.
:::
::: function os.read_line()
A line from standard input without its line break, or `null` at the end.
:::
::: function os.read_all()
All of standard input.
:::
::: function os.isatty(fd)
Whether descriptor `fd` (0, 1, 2) is a terminal.
:::
::: function os.term_size()
`[columns, rows]`.
:::
::: function os.width(text)
How many columns `text` takes: escape sequences count 0, wide characters 2.
:::
::: function os.sleep_ms(ms)
Pauses.
:::
::: function os.ord(ch)
A character's code. `os.chr(n)` is the reverse.
:::

## Processes

::: function os.run(stages, options)
Runs a pipeline: `stages` is a list of argument lists (or one argument list). Options: `stdin`, `stdout`, `stderr` (file paths), `append`, `stderr_append`, `stderr_to_stdout`, `input` (text for the first command), `capture` (return the output instead of showing it), `timeout_ms` (stop a captured command that runs too long), `stdin_fd`/`stdout_fd` (descriptors from `os.pipe`) and `background` (don't wait). Returns a map: `status` (the last command's), `statuses`, `output`, `errors` (e.g. `"command not found: x"`, status 127), `stopped` and `pgid`.
:::
::: function os.interactive()
Makes the program an interactive shell: its own process group, owning the terminal; commands run as foreground jobs that receive Ctrl-C and Ctrl-Z.
:::
::: function os.wait(pid)
Waits for a process: `{status, stopped}`.
:::
::: function os.reap()
Finished background processes, without waiting: a list of `{pid, status, stopped}`.
:::
::: function os.foreground(pgid)
Continues a stopped or background job in the foreground and waits for it.
:::
::: function os.kill(pid, signal=15)
Sends a signal; a negative `pid` signals a process group.
:::
::: function os.fork()
Splits the process; returns 0 in the child (whose signals are reset and which has no job control) and the child's pid in the parent. With `os.pipe()` (`[read_fd, write_fd]`), `os.dup2(from, to)`, `os.open(path, mode)` (`"r"`, `"w"`, `"a"`; -1 on failure), `os.dup(fd)`, `os.read_fd(fd)` (everything until end of file), `os.close(fd)` and `os.inherit(fd)` (let commands inherit it, for `/dev/fd/N`), it's how a shell runs builtins concurrently in a pipeline. `os.run` accepts `stdin_fd` / `stdout_fd` options to connect commands to such descriptors.
:::
::: function os.trap(signal, mode="catch")
`"catch"` a signal (it's then reported by `os.pending_signals()`, a list of signal numbers), `"ignore"` it, or restore the `"default"`.
:::
::: function os.regex(pattern, text)
POSIX extended regular expression match: the whole match and its groups as a list, or `null`.
:::
::: function os.is_binary(path)
Whether a file is a compiled program (ELF or Mach-O) rather than a script.
:::
::: function os.exit(code=0)
Ends the program now.
:::
::: function os.args()
The program's arguments (what followed `--`).
:::
::: function os.pid()
This process's id.
:::

## Files

::: function os.cwd()
The working directory. `os.chdir(path)` changes it, returning an error message or `null`.
:::
::: function os.listdir(path=".")
The names in a directory, sorted, or `null`.
:::
::: function os.stat(path)
`{dir, file, exec, link, size}`, or `null` if nothing is there.
:::
::: function os.read_file(path)
The file's text, or `null`. `os.write_file(path, text, append=false)` writes it and returns whether it worked; `os.mkdir(path)` makes a directory and its parents.
:::
::: function os.glob(pattern)
Paths matching `*`, `?`, `[…]`, and `**` for any number of directories, sorted.
:::
::: function os.fnmatch(pattern, text)
Whether `text` matches a `*`/`?`/`[…]` pattern.
:::
::: function os.which(name)
Where a command is on `$PATH`, or `null`. `os.path_commands()` lists every command there.
:::
::: function os.realpath(path)
The absolute path with links resolved, or `null`.
:::

## Environment

::: function os.getenv(name)
An environment variable, or `null`. `os.setenv(name, value)` and `os.unsetenv(name)` change them; `os.environ()` is all of them as a map.
:::
::: function os.home()
`$HOME`. Also `os.user()`, `os.hostname()`, `os.is_root()`.
:::
::: function os.time_ms()
Milliseconds since 1970.
:::
