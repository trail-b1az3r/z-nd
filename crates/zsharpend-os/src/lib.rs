//! **os** — the operating system, for Zsharpend programs that run outside
//! the sandbox: the terminal (raw keys, size, direct output), processes
//! (pipelines with redirections, process groups and job control), files,
//! directories and the environment. It's what lets a shell be written in
//! Zsharpend (see `examples/spire`).
//!
//! ```text
//! import os
//! os.write("hello\n")
//! let r = os.run([["ls", "-l"], ["wc", "-l"]], {})   // a pipeline
//! Printl(r.status)
//! ```
//!
//! A program that lists this extender leaves the sandbox on purpose, so its
//! time and step limits are lifted (calls may also nest deeper). `Printl`
//! output still works: it's written out before anything `os` does.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use zsharpend::extend::{namespace, Extender};
use zsharpend::{Interp, Value};

/// The `os` script extender. `args` are what `os.args()` returns.
pub struct Os {
    pub args: Vec<String>,
}

impl Os {
    /// Program arguments taken from this process's command line, after `--`
    /// (`whetstone run examples/spire -- -c "echo hi"`).
    pub fn from_process_args() -> Os {
        let all: Vec<String> = std::env::args().collect();
        let args = match all.iter().position(|a| a == "--") {
            Some(i) => all[i + 1..].to_vec(),
            None => vec![],
        };
        Os { args }
    }
}

type R = Result<Value, String>;

static ORIG_TERMIOS: Mutex<Option<libc::termios>> = Mutex::new(None);
static INTERACTIVE: Mutex<bool> = Mutex::new(false);

fn interactive() -> bool {
    *INTERACTIVE.lock().unwrap()
}

/// Writes out everything `Printl`/`Print` buffered so far.
fn flush_output(it: &mut Interp) {
    if it.output.is_empty() {
        return;
    }
    let mut out = std::io::stdout().lock();
    for s in it.output.drain(..) {
        let _ = out.write_all(s.as_bytes());
    }
    let _ = out.flush();
}

fn arg_str(a: &[Value], i: usize) -> Option<String> {
    match a.get(i) {
        Some(Value::Null) | None => None,
        Some(Value::Str(s)) => Some(s.to_string()),
        Some(v) => Some(v.to_string()),
    }
}

fn need_str(a: &[Value], i: usize, f: &str) -> Result<String, String> {
    arg_str(a, i).ok_or_else(|| format!("os.{f}: argument {} must be text", i + 1))
}

fn str_list(v: &Value) -> Vec<String> {
    match v {
        Value::List(l) => l.borrow().iter().map(|x| x.to_string()).collect(),
        Value::Null => vec![],
        other => vec![other.to_string()],
    }
}

fn list_of(items: Vec<String>) -> Value {
    Value::list(items.iter().map(|s| Value::str(s)).collect())
}

fn map_of(items: Vec<(&str, Value)>) -> Value {
    Value::map(items.into_iter().map(|(k, v)| (k.to_string(), v)).collect::<BTreeMap<_, _>>())
}

fn opt<'a>(m: &'a Option<BTreeMap<String, Value>>, k: &str) -> Option<&'a Value> {
    m.as_ref().and_then(|m| m.get(k)).filter(|v| !matches!(v, Value::Null))
}

fn opt_str(m: &Option<BTreeMap<String, Value>>, k: &str) -> Option<String> {
    opt(m, k).map(|v| v.to_string())
}

fn opt_bool(m: &Option<BTreeMap<String, Value>>, k: &str) -> bool {
    opt(m, k).is_some_and(|v| v.truthy())
}

// ---------------------------------------------------------------- terminal

fn set_raw(on: bool) -> Result<(), String> {
    unsafe {
        if !on {
            if let Some(t) = *ORIG_TERMIOS.lock().unwrap() {
                libc::tcsetattr(0, libc::TCSADRAIN, &t);
            }
            return Ok(());
        }
        let mut t: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(0, &mut t) != 0 {
            return Err("os.raw: stdin isn't a terminal".into());
        }
        let mut orig = ORIG_TERMIOS.lock().unwrap();
        if orig.is_none() {
            *orig = Some(t);
        }
        let mut raw = orig.unwrap();
        raw.c_lflag &= !(libc::ECHO | libc::ICANON | libc::ISIG | libc::IEXTEN);
        raw.c_iflag &= !(libc::IXON | libc::ICRNL | libc::BRKINT | libc::INPCK | libc::ISTRIP);
        raw.c_oflag &= !libc::OPOST;
        raw.c_cc[libc::VMIN] = 1;
        raw.c_cc[libc::VTIME] = 0;
        libc::tcsetattr(0, libc::TCSADRAIN, &raw);
    }
    Ok(())
}

/// One byte from stdin, -1 at end of input. `timeout_ms` >= 0 returns -2 on timeout.
fn read_byte(timeout_ms: i64) -> i64 {
    unsafe {
        if timeout_ms >= 0 {
            let mut p = libc::pollfd { fd: 0, events: libc::POLLIN, revents: 0 };
            if libc::poll(&mut p, 1, timeout_ms as i32) <= 0 {
                return -2;
            }
        }
        let mut b = 0u8;
        loop {
            let n = libc::read(0, (&mut b) as *mut u8 as *mut libc::c_void, 1);
            if n == 1 {
                return b as i64;
            }
            if n < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return -1;
        }
    }
}

/// Reads one key: a whole UTF-8 character, or a whole escape sequence
/// (arrows come back as "\e[A"), as text. "" at end of input.
fn read_key() -> String {
    let b = read_byte(-1);
    if b < 0 {
        return String::new();
    }
    let mut bytes = vec![b as u8];
    if b == 0x1b {
        // an escape sequence follows within a few ms, or it's a lone Esc
        let n = read_byte(30);
        if n < 0 {
            return "\x1b".into();
        }
        bytes.push(n as u8);
        if n == b'[' as i64 || n == b'O' as i64 {
            loop {
                let c = read_byte(30);
                if c < 0 {
                    break;
                }
                bytes.push(c as u8);
                if (0x40..=0x7e).contains(&c) && !(n == b'[' as i64 && c == b'[' as i64 && bytes.len() == 3) {
                    break;
                }
            }
        }
    } else if b >= 0xc0 {
        let len = if b >= 0xf0 { 4 } else if b >= 0xe0 { 3 } else { 2 };
        for _ in 1..len {
            let c = read_byte(-1);
            if c < 0 {
                break;
            }
            bytes.push(c as u8);
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

fn term_size() -> (i64, i64) {
    unsafe {
        let mut ws: libc::winsize = std::mem::zeroed();
        if libc::ioctl(1, libc::TIOCGWINSZ, &mut ws) == 0 && ws.ws_col > 0 {
            return (ws.ws_col as i64, ws.ws_row as i64);
        }
    }
    (80, 24)
}

/// Terminal columns `s` takes: ANSI escape sequences count 0, wide (CJK,
/// emoji) characters 2.
pub fn display_width(s: &str) -> usize {
    let mut w = 0;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for d in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&d) {
                        break;
                    }
                }
            } else {
                chars.next();
            }
            continue;
        }
        if (c as u32) < 32 {
            continue;
        }
        w += char_width(c);
    }
    w
}

fn char_width(c: char) -> usize {
    let u = c as u32;
    // combining marks
    if (0x0300..=0x036f).contains(&u) || (0x200b..=0x200f).contains(&u) || (0xfe00..=0xfe0f).contains(&u) {
        return 0;
    }
    let wide = (0x1100..=0x115f).contains(&u)
        || (0x2e80..=0xa4cf).contains(&u)
        || (0xac00..=0xd7a3).contains(&u)
        || (0xf900..=0xfaff).contains(&u)
        || (0xfe30..=0xfe4f).contains(&u)
        || (0xff00..=0xff60).contains(&u)
        || (0xffe0..=0xffe6).contains(&u)
        || (0x1f300..=0x1f64f).contains(&u)
        || (0x1f900..=0x1f9ff).contains(&u)
        || (0x20000..=0x3fffd).contains(&u);
    if wide {
        2
    } else {
        1
    }
}

// ---------------------------------------------------------------- processes

fn sig_default_in_child() {
    unsafe {
        for s in [libc::SIGINT, libc::SIGQUIT, libc::SIGTSTP, libc::SIGTTIN, libc::SIGTTOU, libc::SIGPIPE, libc::SIGCHLD] {
            libc::signal(s, libc::SIG_DFL);
        }
    }
}

/// Becomes an interactive shell: its own process group, owning the
/// terminal, ignoring the keyboard's job-control signals (they go to the
/// foreground job instead).
fn become_interactive() {
    unsafe {
        for s in [libc::SIGINT, libc::SIGQUIT, libc::SIGTSTP, libc::SIGTTIN, libc::SIGTTOU] {
            libc::signal(s, libc::SIG_IGN);
        }
        let pid = libc::getpid();
        if libc::getpgrp() != pid {
            libc::setpgid(0, 0);
        }
        libc::tcsetpgrp(0, pid);
    }
    *INTERACTIVE.lock().unwrap() = true;
}

fn pipe_fds() -> Result<(OwnedFd, OwnedFd), String> {
    let mut fds = [0; 2];
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return Err(format!("pipe: {}", std::io::Error::last_os_error()));
    }
    Ok(unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) })
}

fn dup_fd(fd: &OwnedFd) -> Result<OwnedFd, String> {
    fd.try_clone().map_err(|e| e.to_string())
}

fn open_for(path: &str, write: bool, append: bool) -> Result<OwnedFd, String> {
    let f = if write {
        OpenOptions::new().write(true).create(true).append(append).truncate(!append).open(path)
    } else {
        File::open(path)
    };
    f.map(OwnedFd::from).map_err(|e| format!("{path}: {e}"))
}

fn wait_status(code: libc::c_int) -> (i64, bool) {
    if libc::WIFSTOPPED(code) {
        (128 + libc::WSTOPSIG(code) as i64, true)
    } else if libc::WIFSIGNALED(code) {
        (128 + libc::WTERMSIG(code) as i64, false)
    } else {
        (libc::WEXITSTATUS(code) as i64, false)
    }
}

fn wait_pid(pid: i32) -> (i64, bool) {
    let mut code = 0;
    loop {
        let r = unsafe { libc::waitpid(pid, &mut code, libc::WUNTRACED) };
        if r < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return (127, false);
        }
        return wait_status(code);
    }
}

/// `os.run(stages, opts)`: a pipeline of argv lists. Options: `stdin`,
/// `stdout`, `stderr` (file paths), `append`, `stderr_append`,
/// `stderr_to_stdout`, `input` (text fed to the first stage), `capture`
/// (return the last stage's output as `output` instead of showing it),
/// `background` (don't wait; returns `pgid`).
fn run_pipeline(stages: Vec<Vec<String>>, o: Option<BTreeMap<String, Value>>) -> R {
    if stages.is_empty() || stages.iter().any(|s| s.is_empty()) {
        return Err("os.run: every stage needs a command".into());
    }
    let n = stages.len();
    let capture = opt_bool(&o, "capture");
    let background = opt_bool(&o, "background");
    let mut prev_read: Option<OwnedFd> = None;
    let mut input_writer: Option<(OwnedFd, String)> = None;
    let timeout_ms = opt(&o, "timeout_ms").and_then(|v| v.as_i64());
    if let Some(fd) = opt(&o, "stdin_fd").and_then(|v| v.as_i64()) {
        prev_read = Some((fd as i32).try_into_owned()?);
    } else if let Some(s) = opt_str(&o, "stdin") {
        prev_read = Some(open_for(&s, false, false)?);
    } else if let Some(text) = opt_str(&o, "input") {
        let (r, w) = pipe_fds()?;
        prev_read = Some(r);
        input_writer = Some((w, text));
    }
    let stderr_file = match opt_str(&o, "stderr") {
        Some(p) => Some(open_for(&p, true, opt_bool(&o, "stderr_append"))?),
        None => None,
    };
    let mut capture_read: Option<OwnedFd> = None;
    let mut pids: Vec<i32> = vec![];
    let mut errors: Vec<String> = vec![];
    let mut pgid: i32 = 0;
    for (i, argv) in stages.iter().enumerate() {
        let last = i + 1 == n;
        let (stdout_fd, next_read) = if !last {
            let (r, w) = pipe_fds()?;
            (Some(w), Some(r))
        } else if capture {
            let (r, w) = pipe_fds()?;
            capture_read = Some(r);
            (Some(w), None)
        } else if let Some(fd) = opt(&o, "stdout_fd").and_then(|v| v.as_i64()) {
            (Some((fd as i32).try_into_owned()?), None)
        } else if let Some(p) = opt_str(&o, "stdout") {
            (Some(open_for(&p, true, opt_bool(&o, "append"))?), None)
        } else {
            (None, None)
        };
        let mut cmd = Command::new(&argv[0]);
        cmd.args(&argv[1..]);
        if let Some(r) = prev_read.take() {
            cmd.stdin(Stdio::from(r));
        }
        let stderr_dup = if opt_bool(&o, "stderr_to_stdout") {
            match &stdout_fd {
                Some(fd) => Some(dup_fd(fd)?),
                None => Some(libc::STDOUT_FILENO.try_into_owned()?),
            }
        } else {
            match &stderr_file {
                Some(fd) => Some(dup_fd(fd)?),
                None => None,
            }
        };
        if let Some(fd) = stdout_fd {
            cmd.stdout(Stdio::from(fd));
        }
        if let Some(fd) = stderr_dup {
            cmd.stderr(Stdio::from(fd));
        }
        let target = pgid;
        let job_control = interactive();
        unsafe {
            cmd.pre_exec(move || {
                if job_control {
                    libc::setpgid(0, target);
                }
                sig_default_in_child();
                Ok(())
            });
        }
        match cmd.spawn() {
            Ok(child) => {
                let pid = child.id() as i32;
                if job_control {
                    if pgid == 0 {
                        pgid = pid;
                    }
                    unsafe {
                        libc::setpgid(pid, pgid);
                    }
                }
                pids.push(pid);
                std::mem::forget(child); // we reap it with waitpid
            }
            Err(e) => {
                let msg = if e.kind() == std::io::ErrorKind::NotFound {
                    format!("command not found: {}", argv[0])
                } else {
                    format!("{}: {e}", argv[0])
                };
                errors.push(msg);
                pids.push(-1);
            }
        }
        drop(cmd); // closes our copies of this stage's fds
        prev_read = next_read;
    }
    if let Some((w, text)) = input_writer {
        std::thread::spawn(move || {
            let mut f = File::from(w);
            let _ = f.write_all(text.as_bytes());
        });
    }
    let live: Vec<i32> = pids.iter().copied().filter(|p| *p > 0).collect();
    if background {
        return Ok(map_of(vec![
            ("pgid", Value::Int(if pgid != 0 { pgid as i64 } else { live.first().copied().unwrap_or(-1) as i64 })),
            ("pids", Value::list(live.iter().map(|p| Value::Int(*p as i64)).collect())),
            ("errors", list_of(errors)),
            ("status", Value::Int(0)),
        ]));
    }
    let give_tty = interactive() && pgid != 0 && !capture;
    if give_tty {
        unsafe {
            libc::tcsetpgrp(0, pgid);
        }
    }
    let mut output = String::new();
    if let Some(r) = capture_read {
        let mut buf = vec![];
        match timeout_ms {
            None => {
                let _ = File::from(r).read_to_end(&mut buf);
            }
            Some(ms) => {
                // read until EOF or the deadline; then stop the commands
                let deadline = std::time::Instant::now() + Duration::from_millis(ms.max(0) as u64);
                let mut f = File::from(r);
                loop {
                    let left = deadline.saturating_duration_since(std::time::Instant::now());
                    if left.is_zero() {
                        for p in &live {
                            unsafe {
                                libc::kill(*p, libc::SIGKILL);
                            }
                        }
                        break;
                    }
                    let mut pfd = libc::pollfd { fd: f.as_raw_fd(), events: libc::POLLIN, revents: 0 };
                    if unsafe { libc::poll(&mut pfd, 1, left.as_millis().min(i32::MAX as u128) as i32) } <= 0 {
                        continue;
                    }
                    let mut chunk = [0u8; 8192];
                    match f.read(&mut chunk) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                }
            }
        }
        output = String::from_utf8_lossy(&buf).into_owned();
    }
    let mut statuses = vec![];
    let mut stopped = false;
    for p in &pids {
        if *p < 0 {
            statuses.push(Value::Int(127));
            continue;
        }
        let (st, stop) = wait_pid(*p);
        stopped |= stop;
        statuses.push(Value::Int(st));
    }
    if give_tty {
        unsafe {
            libc::tcsetpgrp(0, libc::getpgrp());
        }
    }
    let status = statuses.last().cloned().unwrap_or(Value::Int(0));
    Ok(map_of(vec![
        ("status", status),
        ("statuses", Value::list(statuses)),
        ("output", Value::str(&output)),
        ("errors", list_of(errors)),
        ("stopped", Value::Bool(stopped)),
        ("pgid", Value::Int(pgid as i64)),
    ]))
}

trait IntoOwned {
    fn try_into_owned(self) -> Result<OwnedFd, String>;
}

impl IntoOwned for i32 {
    fn try_into_owned(self) -> Result<OwnedFd, String> {
        let d = unsafe { libc::fcntl(self, libc::F_DUPFD_CLOEXEC, 3) };
        if d < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        Ok(unsafe { OwnedFd::from_raw_fd(d) })
    }
}

// ---------------------------------------------------------------- signals

static PENDING: AtomicU64 = AtomicU64::new(0);

extern "C" fn note_signal(sig: libc::c_int) {
    if (0..64).contains(&sig) {
        PENDING.fetch_or(1 << sig, Ordering::SeqCst);
    }
}

/// mode: "catch" (remember it for pending_signals), "ignore", or "default"
fn trap_signal(sig: i32, mode: &str) -> Result<(), String> {
    if !(1..64).contains(&sig) || sig == libc::SIGKILL || sig == libc::SIGSTOP {
        return Err(format!("os.trap: can't trap signal {sig}"));
    }
    let handler = match mode {
        "catch" => note_signal as extern "C" fn(libc::c_int) as libc::sighandler_t,
        "ignore" => libc::SIG_IGN,
        _ => {
            let job_sig = [libc::SIGINT, libc::SIGQUIT, libc::SIGTSTP, libc::SIGTTIN, libc::SIGTTOU].contains(&sig);
            if job_sig && interactive() {
                libc::SIG_IGN
            } else {
                libc::SIG_DFL
            }
        }
    };
    unsafe {
        libc::signal(sig, handler);
    }
    Ok(())
}

fn pending_signals() -> Vec<i64> {
    let bits = PENDING.swap(0, Ordering::SeqCst);
    (1..64).filter(|s| bits & (1 << s) != 0).collect()
}

// ---------------------------------------------------------------- regex

/// POSIX extended regex: the whole match and its groups, or None.
pub fn regex_match(pattern: &str, text: &str) -> Result<Option<Vec<String>>, String> {
    let (Ok(cp), Ok(ct)) = (std::ffi::CString::new(pattern), std::ffi::CString::new(text)) else {
        return Err("os.regex: text contains a NUL".into());
    };
    unsafe {
        let mut re: libc::regex_t = std::mem::zeroed();
        if libc::regcomp(&mut re, cp.as_ptr(), libc::REG_EXTENDED) != 0 {
            return Err(format!("os.regex: bad pattern {pattern:?}"));
        }
        let mut m: [libc::regmatch_t; 10] = std::mem::zeroed();
        let rc = libc::regexec(&re, ct.as_ptr(), m.len(), m.as_mut_ptr(), 0);
        libc::regfree(&mut re);
        if rc != 0 {
            return Ok(None);
        }
        let bytes = text.as_bytes();
        let mut groups = vec![];
        for g in m.iter() {
            if g.rm_so < 0 {
                break;
            }
            groups.push(String::from_utf8_lossy(&bytes[g.rm_so as usize..g.rm_eo as usize]).into_owned());
        }
        Ok(Some(groups))
    }
}

/// Is `path` a compiled program (ELF or Mach-O) rather than a script?
pub fn is_native_binary(path: &str) -> bool {
    let mut m = [0u8; 4];
    let Ok(mut f) = File::open(path) else { return false };
    if f.read_exact(&mut m).is_err() {
        return false;
    }
    m == [0x7f, b'E', b'L', b'F'] || m == [0xcf, 0xfa, 0xed, 0xfe] || m == [0xce, 0xfa, 0xed, 0xfe]
}

// ---------------------------------------------------------------- files

fn glob_match(pat: &[char], s: &[char]) -> bool {
    match pat.first() {
        None => s.is_empty(),
        Some('*') => (0..=s.len()).any(|i| glob_match(&pat[1..], &s[i..])),
        Some('?') => !s.is_empty() && glob_match(&pat[1..], &s[1..]),
        Some('[') => {
            let Some(close) = pat.iter().skip(1).position(|c| *c == ']').map(|p| p + 1) else {
                return s.first() == Some(&'[') && glob_match(&pat[1..], &s[1..]);
            };
            let Some(c) = s.first() else { return false };
            let set = &pat[1..close];
            let (neg, set) = if matches!(set.first(), Some('!') | Some('^')) { (true, &set[1..]) } else { (false, set) };
            let mut hit = false;
            let mut i = 0;
            while i < set.len() {
                if i + 2 < set.len() && set[i + 1] == '-' {
                    if set[i] <= *c && *c <= set[i + 2] {
                        hit = true;
                    }
                    i += 3;
                } else {
                    if set[i] == *c {
                        hit = true;
                    }
                    i += 1;
                }
            }
            hit != neg && glob_match(&pat[close + 1..], &s[1..])
        }
        Some(c) => s.first() == Some(c) && glob_match(&pat[1..], &s[1..]),
    }
}

/// Shell-style `*`, `?`, `[...]` matching.
pub fn fnmatch(pattern: &str, s: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = s.chars().collect();
    glob_match(&p, &t)
}

fn has_magic(s: &str) -> bool {
    s.contains(['*', '?', '['])
}

/// Expands a path pattern, component by component; `**` matches any
/// number of directories. Sorted; empty when nothing matches.
pub fn glob(pattern: &str) -> Vec<String> {
    let absolute = pattern.starts_with('/');
    let comps: Vec<&str> = pattern.split('/').filter(|c| !c.is_empty()).collect();
    let mut out = vec![];
    let start = if absolute { "/".to_string() } else { String::new() };
    glob_walk(&start, &comps, &mut out, 0);
    out.sort();
    out.dedup();
    out
}

fn join(base: &str, name: &str) -> String {
    if base.is_empty() {
        name.to_string()
    } else if base.ends_with('/') {
        format!("{base}{name}")
    } else {
        format!("{base}/{name}")
    }
}

fn glob_walk(base: &str, comps: &[&str], out: &mut Vec<String>, depth: usize) {
    if depth > 40 {
        return;
    }
    let Some((first, rest)) = comps.split_first() else {
        if !base.is_empty() {
            out.push(base.to_string());
        }
        return;
    };
    let dir = if base.is_empty() { "." } else { base };
    if *first == "**" {
        glob_walk(base, rest, out, depth + 1);
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') {
                    continue;
                }
                if e.file_type().is_ok_and(|t| t.is_dir()) {
                    glob_walk(&join(base, &name), comps, out, depth + 1);
                }
            }
        }
        return;
    }
    if !has_magic(first) {
        let p = join(base, first);
        if rest.is_empty() {
            if std::fs::symlink_metadata(&p).is_ok() {
                out.push(p);
            }
        } else if std::path::Path::new(&p).is_dir() {
            glob_walk(&p, rest, out, depth + 1);
        }
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') && !first.starts_with('.') {
            continue;
        }
        if !fnmatch(first, &name) {
            continue;
        }
        let p = join(base, &name);
        if rest.is_empty() {
            out.push(p);
        } else if std::path::Path::new(&p).is_dir() {
            glob_walk(&p, rest, out, depth + 1);
        }
    }
}

fn stat(path: &str) -> Value {
    let Ok(lm) = std::fs::symlink_metadata(path) else { return Value::Null };
    let m = std::fs::metadata(path).ok();
    let is_dir = m.as_ref().is_some_and(|m| m.is_dir());
    let is_file = m.as_ref().is_some_and(|m| m.is_file());
    let exec = m.as_ref().is_some_and(|m| m.permissions().mode() & 0o111 != 0) && !is_dir;
    map_of(vec![
        ("dir", Value::Bool(is_dir)),
        ("file", Value::Bool(is_file)),
        ("exec", Value::Bool(exec)),
        ("link", Value::Bool(lm.file_type().is_symlink())),
        ("size", Value::Int(m.as_ref().map(|m| m.len() as i64).unwrap_or(0))),
    ])
}

fn which(name: &str) -> Option<String> {
    if name.contains('/') {
        return is_executable(name).then(|| name.to_string());
    }
    let path = std::env::var("PATH").unwrap_or_default();
    path.split(':').filter(|d| !d.is_empty()).map(|d| join(d, name)).find(|p| is_executable(p))
}

fn is_executable(p: &str) -> bool {
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

fn path_commands() -> Vec<String> {
    let path = std::env::var("PATH").unwrap_or_default();
    let mut out = vec![];
    for d in path.split(':').filter(|d| !d.is_empty()) {
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if !name.starts_with('.') && is_executable(&join(d, &name)) {
                    out.push(name);
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

// ---------------------------------------------------------------- the module

fn module(args: Vec<String>) -> Value {
    let f = |name: &str, body: fn(&mut Interp, Vec<Value>) -> R| (name.to_string(), Value::native(&format!("os.{name}"), body));
    let args_val = args.clone();
    let mut items: Vec<(String, Value)> = vec![
        // terminal & output
        f("write", |it, a| {
            flush_output(it);
            let mut o = std::io::stdout().lock();
            for v in &a {
                let _ = o.write_all(v.to_string().as_bytes());
            }
            let _ = o.flush();
            Ok(Value::Null)
        }),
        f("ewrite", |it, a| {
            flush_output(it);
            let mut o = std::io::stderr().lock();
            for v in &a {
                let _ = o.write_all(v.to_string().as_bytes());
            }
            Ok(Value::Null)
        }),
        f("flush", |it, _| {
            flush_output(it);
            Ok(Value::Null)
        }),
        f("raw", |it, a| {
            flush_output(it);
            set_raw(a.first().is_some_and(|v| v.truthy()))?;
            Ok(Value::Null)
        }),
        f("read_key", |it, _| {
            flush_output(it);
            Ok(Value::str(&read_key()))
        }),
        f("key_waiting", |_, a| {
            let ms = a.first().and_then(|v| v.as_i64()).unwrap_or(0);
            let mut p = libc::pollfd { fd: 0, events: libc::POLLIN, revents: 0 };
            Ok(Value::Bool(unsafe { libc::poll(&mut p, 1, ms as i32) } > 0))
        }),
        f("read_line", |it, _| {
            // byte at a time from fd 0, never reading past the line: the
            // rest of the input belongs to whatever reads it next
            flush_output(it);
            let mut line = vec![];
            loop {
                let b = read_byte(-1);
                if b < 0 {
                    if line.is_empty() {
                        return Ok(Value::Null);
                    }
                    break;
                }
                if b == b'\n' as i64 {
                    break;
                }
                line.push(b as u8);
            }
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            Ok(Value::str(&String::from_utf8_lossy(&line)))
        }),
        f("read_all", |it, _| {
            flush_output(it);
            let mut buf = vec![];
            let mut chunk = [0u8; 8192];
            loop {
                let n = unsafe { libc::read(0, chunk.as_mut_ptr() as *mut libc::c_void, chunk.len()) };
                if n < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                if n <= 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n as usize]);
            }
            Ok(Value::str(&String::from_utf8_lossy(&buf)))
        }),
        f("isatty", |_, a| Ok(Value::Bool(unsafe { libc::isatty(a.first().and_then(|v| v.as_i64()).unwrap_or(0) as i32) } == 1))),
        f("term_size", |_, _| {
            let (c, r) = term_size();
            Ok(Value::list(vec![Value::Int(c), Value::Int(r)]))
        }),
        f("width", |_, a| Ok(Value::Int(display_width(&arg_str(&a, 0).unwrap_or_default()) as i64))),
        f("sleep_ms", |it, a| {
            flush_output(it);
            std::thread::sleep(Duration::from_millis(a.first().and_then(|v| v.as_i64()).unwrap_or(0).max(0) as u64));
            Ok(Value::Null)
        }),
        f("ord", |_, a| Ok(Value::Int(arg_str(&a, 0).and_then(|s| s.chars().next()).map(|c| c as i64).unwrap_or(-1)))),
        f("chr", |_, a| Ok(Value::str(&char::from_u32(a.first().and_then(|v| v.as_i64()).unwrap_or(0) as u32).unwrap_or('?').to_string()))),
        // processes
        f("interactive", |_, _| {
            become_interactive();
            Ok(Value::Null)
        }),
        f("run", |it, a| {
            flush_output(it);
            let stages: Vec<Vec<String>> = match a.first() {
                Some(Value::List(l)) => {
                    let l = l.borrow();
                    if l.iter().all(|x| matches!(x, Value::List(_))) {
                        l.iter().map(str_list).collect()
                    } else {
                        vec![l.iter().map(|x| x.to_string()).collect()]
                    }
                }
                _ => return Err("os.run: expected a list of commands".into()),
            };
            let opts = match a.get(1) {
                Some(Value::Map(m)) => Some(m.borrow().clone()),
                _ => None,
            };
            run_pipeline(stages, opts)
        }),
        f("wait", |_, a| {
            let pid = a.first().and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
            let (st, stopped) = wait_pid(pid);
            Ok(map_of(vec![("status", Value::Int(st)), ("stopped", Value::Bool(stopped))]))
        }),
        f("reap", |_, _| {
            // finished background processes, without blocking
            let mut done = vec![];
            loop {
                let mut code = 0;
                let pid = unsafe { libc::waitpid(-1, &mut code, libc::WNOHANG | libc::WUNTRACED) };
                if pid <= 0 {
                    break;
                }
                let (st, stopped) = wait_status(code);
                done.push(map_of(vec![("pid", Value::Int(pid as i64)), ("status", Value::Int(st)), ("stopped", Value::Bool(stopped))]));
            }
            Ok(Value::list(done))
        }),
        f("foreground", |_, a| {
            // resume a stopped/background job in the foreground and wait for it
            let pgid = a.first().and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
            unsafe {
                if interactive() {
                    libc::tcsetpgrp(0, pgid);
                }
                libc::kill(-pgid, libc::SIGCONT);
            }
            let (st, stopped) = wait_pid(-pgid);
            if interactive() {
                unsafe {
                    libc::tcsetpgrp(0, libc::getpgrp());
                }
            }
            Ok(map_of(vec![("status", Value::Int(st)), ("stopped", Value::Bool(stopped))]))
        }),
        f("kill", |_, a| {
            let pid = a.first().and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
            let sig = a.get(1).and_then(|v| v.as_i64()).unwrap_or(libc::SIGTERM as i64) as i32;
            Ok(Value::Bool(unsafe { libc::kill(pid, sig) } == 0))
        }),
        f("exit", |it, a| {
            flush_output(it);
            let _ = set_raw(false);
            std::process::exit(a.first().and_then(|v| v.as_i64()).unwrap_or(0) as i32)
        }),
        f("pid", |_, _| Ok(Value::Int(std::process::id() as i64))),
        // low-level process plumbing, for running shell code concurrently
        f("fork", |it, _| {
            flush_output(it);
            let _ = std::io::stdout().flush();
            let pid = unsafe { libc::fork() };
            if pid < 0 {
                return Err(format!("os.fork: {}", std::io::Error::last_os_error()));
            }
            if pid == 0 {
                // the child is a plain process: default signals, no job control
                sig_default_in_child();
                *INTERACTIVE.lock().unwrap() = false;
            }
            Ok(Value::Int(pid as i64))
        }),
        f("pipe", |_, _| {
            let mut fds = [0; 2];
            if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
                return Err(format!("os.pipe: {}", std::io::Error::last_os_error()));
            }
            Ok(Value::list(vec![Value::Int(fds[0] as i64), Value::Int(fds[1] as i64)]))
        }),
        f("dup2", |it, a| {
            flush_output(it);
            let _ = std::io::stdout().flush();
            let from = a.first().and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
            let to = a.get(1).and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
            Ok(Value::Bool(unsafe { libc::dup2(from, to) } >= 0))
        }),
        f("dup", |_, a| {
            let fd = a.first().and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
            Ok(Value::Int(unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 10) } as i64))
        }),
        f("open", |_, a| {
            // a descriptor for a file: mode "r", "w" (truncate) or "a" (append)
            let p = need_str(&a, 0, "open")?;
            let mode = arg_str(&a, 1).unwrap_or_else(|| "r".into());
            let f = match mode.as_str() {
                "w" => OpenOptions::new().write(true).create(true).truncate(true).open(&p),
                "a" => OpenOptions::new().append(true).create(true).open(&p),
                _ => File::open(&p),
            };
            Ok(match f {
                Ok(f) => {
                    let fd = OwnedFd::from(f);
                    let raw = fd.as_raw_fd();
                    std::mem::forget(fd); // the program closes it with os.close
                    Value::Int(raw as i64)
                }
                Err(_) => Value::Int(-1),
            })
        }),
        f("read_fd", |_, a| {
            // everything from a descriptor until end of file
            let fd = a.first().and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
            let mut buf = vec![];
            let mut chunk = [0u8; 8192];
            loop {
                let n = unsafe { libc::read(fd, chunk.as_mut_ptr() as *mut libc::c_void, chunk.len()) };
                if n < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                if n <= 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n as usize]);
            }
            Ok(Value::str(&String::from_utf8_lossy(&buf)))
        }),
        f("inherit", |_, a| {
            // let child commands inherit this descriptor (for /dev/fd/N)
            let fd = a.first().and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
            Ok(Value::Bool(unsafe { libc::fcntl(fd, libc::F_SETFD, 0) } == 0))
        }),
        f("close", |_, a| {
            let fd = a.first().and_then(|v| v.as_i64()).unwrap_or(-1) as i32;
            Ok(Value::Bool(unsafe { libc::close(fd) } == 0))
        }),
        f("trap", |_, a| {
            let sig = a.first().and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            trap_signal(sig, &arg_str(&a, 1).unwrap_or_else(|| "catch".into()))?;
            Ok(Value::Null)
        }),
        f("pending_signals", |_, _| Ok(Value::list(pending_signals().into_iter().map(Value::Int).collect()))),
        f("regex", |_, a| {
            let p = need_str(&a, 0, "regex")?;
            let t = arg_str(&a, 1).unwrap_or_default();
            Ok(match regex_match(&p, &t)? {
                Some(g) => list_of(g),
                None => Value::Null,
            })
        }),
        f("is_binary", |_, a| Ok(Value::Bool(is_native_binary(&need_str(&a, 0, "is_binary")?)))),
        // environment
        f("getenv", |_, a| Ok(std::env::var(need_str(&a, 0, "getenv")?).map(|v| Value::str(&v)).unwrap_or(Value::Null))),
        f("setenv", |_, a| {
            let k = need_str(&a, 0, "setenv")?;
            let v = arg_str(&a, 1).unwrap_or_default();
            if !k.is_empty() && !k.contains(['=', '\0']) && !v.contains('\0') {
                // SAFETY-free API: set_var is fine in this single-threaded interpreter
                std::env::set_var(k, v);
            }
            Ok(Value::Null)
        }),
        f("unsetenv", |_, a| {
            let k = need_str(&a, 0, "unsetenv")?;
            if !k.is_empty() && !k.contains(['=', '\0']) {
                std::env::remove_var(k);
            }
            Ok(Value::Null)
        }),
        f("environ", |_, _| Ok(Value::map(std::env::vars().map(|(k, v)| (k, Value::str(&v))).collect()))),
        f("home", |_, _| Ok(Value::str(&std::env::var("HOME").unwrap_or_else(|_| "/".into())))),
        f("user", |_, _| {
            if let Ok(u) = std::env::var("USER") {
                return Ok(Value::str(&u));
            }
            unsafe {
                let pw = libc::getpwuid(libc::getuid());
                if !pw.is_null() {
                    return Ok(Value::str(&std::ffi::CStr::from_ptr((*pw).pw_name).to_string_lossy()));
                }
            }
            Ok(Value::str("user"))
        }),
        f("is_root", |_, _| Ok(Value::Bool(unsafe { libc::geteuid() } == 0))),
        f("hostname", |_, _| {
            let mut buf = [0u8; 256];
            unsafe {
                libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len());
            }
            let end = buf.iter().position(|b| *b == 0).unwrap_or(buf.len());
            Ok(Value::str(&String::from_utf8_lossy(&buf[..end])))
        }),
        f("time_ms", |_, _| {
            let d = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
            Ok(Value::Int(d.as_millis() as i64))
        }),
        // files
        f("cwd", |_, _| Ok(Value::str(&std::env::current_dir().map(|p| p.display().to_string()).unwrap_or_else(|_| "?".into())))),
        f("chdir", |_, a| {
            let p = need_str(&a, 0, "chdir")?;
            Ok(match std::env::set_current_dir(&p) {
                Ok(()) => Value::Null,
                Err(e) => Value::str(&e.to_string()),
            })
        }),
        f("listdir", |_, a| {
            let p = arg_str(&a, 0).unwrap_or_else(|| ".".into());
            let Ok(rd) = std::fs::read_dir(&p) else { return Ok(Value::Null) };
            let mut names: Vec<String> = rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
            names.sort();
            Ok(list_of(names))
        }),
        f("stat", |_, a| Ok(stat(&need_str(&a, 0, "stat")?))),
        f("read_file", |_, a| Ok(std::fs::read_to_string(need_str(&a, 0, "read_file")?).map(|s| Value::str(&s)).unwrap_or(Value::Null))),
        f("write_file", |_, a| {
            let p = need_str(&a, 0, "write_file")?;
            let text = arg_str(&a, 1).unwrap_or_default();
            let append = a.get(2).is_some_and(|v| v.truthy());
            let r = OpenOptions::new().write(true).create(true).append(append).truncate(!append).open(&p).and_then(|mut f| f.write_all(text.as_bytes()));
            Ok(Value::Bool(r.is_ok()))
        }),
        f("mkdir", |_, a| Ok(Value::Bool(std::fs::create_dir_all(need_str(&a, 0, "mkdir")?).is_ok()))),
        f("glob", |_, a| Ok(list_of(glob(&need_str(&a, 0, "glob")?)))),
        f("fnmatch", |_, a| Ok(Value::Bool(fnmatch(&need_str(&a, 0, "fnmatch")?, &arg_str(&a, 1).unwrap_or_default())))),
        f("which", |_, a| Ok(which(&need_str(&a, 0, "which")?).map(|p| Value::str(&p)).unwrap_or(Value::Null))),
        f("path_commands", |_, _| Ok(list_of(path_commands()))),
        f("realpath", |_, a| Ok(std::fs::canonicalize(need_str(&a, 0, "realpath")?).map(|p| Value::str(&p.display().to_string())).unwrap_or(Value::Null))),
    ];
    items.push(("args".into(), Value::native("os.args", move |_, _| Ok(list_of(args_val.clone())))));
    namespace(items.iter().map(|(k, v)| (k.as_str(), v.clone())).collect())
}

impl Extender for Os {
    fn name(&self) -> &str {
        "os"
    }
    fn about(&self) -> &str {
        "terminal, processes, files and environment (leaves the sandbox)"
    }
    fn install(&self, it: &mut Interp) {
        // Opting into `os` means running as a real program, not a sandboxed
        // script: no time or step budget, and room for deeper recursion.
        it.limits.steps = u64::MAX;
        it.limits.time = Duration::from_secs(60 * 60 * 24 * 365 * 100);
        it.limits.depth = it.limits.depth.max(256);
    }
    fn modules(&self, _it: &mut Interp) -> Vec<(String, Value)> {
        vec![("os".into(), module(self.args.clone()))]
    }
}
