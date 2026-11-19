//! Syntax highlighting for the docs, following Z#++'s own lexing rules
//! (crates/stonedgem-zsharp/src/lexer.rs): `//` and `/* */` comments,
//! strings with `${…}` interpolation, `#rgb`/`#rrggbb`/`#rrggbbaa` colour
//! literals (shown with a swatch), numbers with `_` separators and `0x`,
//! keywords from the parser, built-ins from the standard library.
//!
//! Rust blocks get a smaller version of the same thing.

const KEYWORDS: &[&str] = &[
    "func", "let", "const", "var", "global", "if", "else", "while", "for", "in", "return", "break", "continue", "struct", "impl", "on",
    "import", "match", "try", "catch", "throw", "true", "false", "null", "and", "or", "not", "extern", "kernel", "fragment", "buf",
    "uniform", "self", "fn", "enum", "trait", "loop", "elif", "switch", "case", "default", "mut", "from", "as", "None", "True", "False",
];

const TYPES: &[&str] = &[
    "int", "float", "bool", "string", "vec2", "vec3", "vec4", "color", "list", "map", "void", "Vec2", "Canvas", "f32", "f64", "i8", "i16", "i32", "i64", "u8",
    "u16", "u32", "u64", "usize", "U0", "U8", "U16", "U32", "U64", "I8", "I16", "I32", "I64", "F64", "Bool",
];

const RUST_KEYWORDS: &[&str] = &[
    "fn", "let", "mut", "pub", "use", "mod", "impl", "struct", "enum", "trait", "match", "if", "else", "for", "in", "while", "loop",
    "return", "move", "ref", "as", "where", "const", "static", "self", "Self", "crate", "super", "true", "false", "dyn", "type", "unsafe",
];

pub fn escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            c => o.push(c),
        }
    }
    o
}

fn span(out: &mut String, class: &str, text: &str) {
    out.push_str(&format!("<span class=\"{class}\">{}</span>", escape(text)));
}

fn builtins() -> &'static [&'static str] {
    zsharpend::stdlib::GLOBALS
}

/// Z#++ source → HTML (the inside of a `<code>`).
pub fn zsharp(src: &str) -> String {
    let c: Vec<char> = src.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let at = |i: usize| c.get(i).copied().unwrap_or('\0');
    while i < c.len() {
        let ch = c[i];
        // Comments
        if ch == '/' && at(i + 1) == '/' {
            let s = i;
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
            span(&mut out, "c", &c[s..i].iter().collect::<String>());
            continue;
        }
        if ch == '/' && at(i + 1) == '*' {
            let s = i;
            i += 2;
            while i < c.len() && !(c[i] == '*' && at(i + 1) == '/') {
                i += 1;
            }
            i = (i + 2).min(c.len());
            span(&mut out, "c", &c[s..i].iter().collect::<String>());
            continue;
        }
        // Strings, with interpolations highlighted as code
        if ch == '"' || ch == '\'' {
            let q = ch;
            out.push_str("<span class=\"s\">");
            out.push_str(&escape(&q.to_string()));
            i += 1;
            while i < c.len() && c[i] != q && c[i] != '\n' {
                if c[i] == '\\' && i + 1 < c.len() {
                    out.push_str(&format!("<span class=\"e\">{}</span>", escape(&c[i..i + 2].iter().collect::<String>())));
                    i += 2;
                    continue;
                }
                if c[i] == '$' && at(i + 1) == '{' {
                    let mut depth = 1;
                    let mut j = i + 2;
                    while j < c.len() && depth > 0 && c[j] != '\n' {
                        match c[j] {
                            '{' => depth += 1,
                            '}' => depth -= 1,
                            _ => {}
                        }
                        j += 1;
                    }
                    let inner: String = c[i + 2..j.saturating_sub(1).max(i + 2)].iter().collect();
                    out.push_str(&format!("<span class=\"i\">${{</span><span class=\"ic\">{}</span><span class=\"i\">}}</span>", zsharp(&inner)));
                    i = j;
                    continue;
                }
                out.push_str(&escape(&c[i].to_string()));
                i += 1;
            }
            if i < c.len() && c[i] == q {
                out.push_str(&escape(&q.to_string()));
                i += 1;
            }
            out.push_str("</span>");
            continue;
        }
        // Colour literals: a swatch shows the colour
        if ch == '#' && at(i + 1).is_ascii_hexdigit() {
            let mut j = i + 1;
            while j < c.len() && c[j].is_ascii_hexdigit() {
                j += 1;
            }
            let hex: String = c[i + 1..j].iter().collect();
            if matches!(hex.len(), 3 | 6 | 8) {
                let css = match hex.len() {
                    8 => format!("#{}", hex),
                    _ => format!("#{hex}"),
                };
                out.push_str(&format!("<span class=\"k-col\"><i class=\"sw\" style=\"--sw:{css}\"></i>#{hex}</span>"));
                i = j;
                continue;
            }
        }
        // Numbers
        if ch.is_ascii_digit() {
            let s = i;
            if ch == '0' && matches!(at(i + 1), 'x' | 'X') {
                i += 2;
                while i < c.len() && (c[i].is_ascii_hexdigit() || c[i] == '_') {
                    i += 1;
                }
            } else {
                while i < c.len() && (c[i].is_ascii_digit() || c[i] == '_') {
                    i += 1;
                }
                if at(i) == '.' && at(i + 1).is_ascii_digit() {
                    i += 1;
                    while i < c.len() && (c[i].is_ascii_digit() || c[i] == '_') {
                        i += 1;
                    }
                }
                if at(i) == 'f' && !at(i + 1).is_alphanumeric() {
                    i += 1;
                }
            }
            span(&mut out, "n", &c[s..i].iter().collect::<String>());
            continue;
        }
        // Words
        if ch.is_alphabetic() || ch == '_' {
            let s = i;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_') {
                i += 1;
            }
            let w: String = c[s..i].iter().collect();
            let mut j = i;
            while at(j) == ' ' {
                j += 1;
            }
            let called = at(j) == '(';
            let prev_dot = s > 0 && c[s - 1] == '.';
            if KEYWORDS.contains(&w.as_str()) && !prev_dot {
                span(&mut out, "k", &w);
            } else if TYPES.contains(&w.as_str()) {
                span(&mut out, "t", &w);
            } else if (builtins().contains(&w.as_str()) && !prev_dot) || w == "ZS" {
                span(&mut out, "b", &w);
            } else if called {
                span(&mut out, "f", &w);
            } else if w.chars().next().is_some_and(char::is_uppercase) {
                span(&mut out, "t", &w);
            } else {
                out.push_str(&escape(&w));
            }
            continue;
        }
        // Operators worth seeing
        for op in ["|>", "=>", "->", "??", "?.", "..=", ".."] {
            if c[i..].iter().take(op.len()).collect::<String>() == op {
                span(&mut out, "o", op);
                i += op.len();
                break;
            }
        }
        if i < c.len() && c[i] == ch {
            out.push_str(&escape(&ch.to_string()));
            i += 1;
        }
    }
    out
}

/// Rust source → HTML (keywords, strings, comments, numbers, types).
pub fn rust(src: &str) -> String {
    let c: Vec<char> = src.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let at = |i: usize| c.get(i).copied().unwrap_or('\0');
    while i < c.len() {
        let ch = c[i];
        if ch == '/' && at(i + 1) == '/' {
            let s = i;
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
            span(&mut out, "c", &c[s..i].iter().collect::<String>());
        } else if ch == '"' {
            let s = i;
            i += 1;
            while i < c.len() && c[i] != '"' {
                if c[i] == '\\' {
                    i += 1;
                }
                i += 1;
            }
            i = (i + 1).min(c.len());
            span(&mut out, "s", &c[s..i].iter().collect::<String>());
        } else if ch.is_ascii_digit() {
            let s = i;
            while i < c.len() && (c[i].is_ascii_alphanumeric() || c[i] == '_' || c[i] == '.') && !(c[i] == '.' && at(i + 1) == '.') {
                i += 1;
            }
            span(&mut out, "n", &c[s..i].iter().collect::<String>());
        } else if ch.is_alphabetic() || ch == '_' {
            let s = i;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_') {
                i += 1;
            }
            let w: String = c[s..i].iter().collect();
            if RUST_KEYWORDS.contains(&w.as_str()) {
                span(&mut out, "k", &w);
            } else if at(i) == '!' {
                span(&mut out, "b", &w);
            } else if at(i) == '(' {
                span(&mut out, "f", &w);
            } else if w.chars().next().is_some_and(char::is_uppercase) {
                span(&mut out, "t", &w);
            } else {
                out.push_str(&escape(&w));
            }
        } else {
            out.push_str(&escape(&ch.to_string()));
            i += 1;
        }
    }
    out
}

/// TOML (`.wtf` files): sections, keys, strings, comments.
pub fn toml(src: &str) -> String {
    let mut out = String::new();
    for (i, line) in src.lines().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let t = line.trim_start();
        if t.starts_with('#') {
            span(&mut out, "c", line);
        } else if t.starts_with('[') {
            span(&mut out, "k", line);
        } else if let Some((k, v)) = line.split_once('=') {
            span(&mut out, "f", k);
            out.push('=');
            let (val, comment) = match v.find(" #") {
                Some(p) => (&v[..p], Some(&v[p..])),
                None => (v, None),
            };
            let cls = if val.trim().starts_with('"') || val.trim().starts_with('[') { "s" } else { "n" };
            span(&mut out, cls, val);
            if let Some(c) = comment {
                span(&mut out, "c", c);
            }
        } else {
            out.push_str(&escape(line));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn highlights_zsharp() {
        let h = zsharp("let c = #f80 // warm\nPrintl(\"hi ${name}\")");
        assert!(h.contains("<span class=\"k\">let</span>"));
        assert!(h.contains("--sw:#f80"));
        assert!(h.contains("<span class=\"c\">// warm</span>"));
        assert!(h.contains("<span class=\"b\">Printl</span>"));
        assert!(h.contains("<span class=\"i\">${</span>"));
        assert!(zsharp("a < b").contains("&lt;"));
    }
}
