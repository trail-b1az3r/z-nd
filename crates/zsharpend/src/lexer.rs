//! Tokens. Z# ends statements at line ends (no semicolons); newlines are
//! tokens, and the parser skips them where an expression clearly continues
//! (after an operator, a comma or an opening bracket).

use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Int(i64),
    Float(f64),
    /// A string: literal parts and `${expr}` sources, alternating, starting with a literal.
    Str(Vec<StrPart>),
    /// `#rrggbb` / `#rrggbbaa` colour literal.
    Color(u32),
    Ident(String),
    Op(&'static str),
    Newline,
    Eof,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StrPart {
    Lit(String),
    /// Source of an interpolated expression and its line.
    Expr(String, usize),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub line: usize,
    pub col: usize,
}

impl fmt::Display for Tok {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Tok::Int(i) => write!(f, "{i}"),
            Tok::Float(x) => write!(f, "{x}"),
            Tok::Str(_) => write!(f, "a string"),
            Tok::Color(c) => write!(f, "#{c:08x}"),
            Tok::Ident(s) => write!(f, "`{s}`"),
            Tok::Op(o) => write!(f, "`{o}`"),
            Tok::Newline => write!(f, "end of line"),
            Tok::Eof => write!(f, "end of file"),
        }
    }
}

/// Longest first.
const OPS: &[&str] = &[
    "..=", "...", "|>", "=>", "->", "==", "!=", "<=", ">=", "&&", "||", "+=", "-=", "*=", "/=", "%=", "??", "?.", "..", "::", "**", "+", "-",
    "*", "/", "%", "=", "<", ">", "!", "(", ")", "{", "}", "[", "]", ",", ".", ":", "?", "|", "&", "@",
];

pub fn lex(src: &str) -> Result<Vec<Token>, String> {
    lex_at(src, 1)
}

pub fn lex_at(src: &str, first_line: usize) -> Result<Vec<Token>, String> {
    let c: Vec<char> = src.chars().collect();
    let (mut i, mut line, mut col) = (0usize, first_line, 1usize);
    let mut out = Vec::new();
    macro_rules! push {
        ($t:expr, $l:expr, $c:expr) => {
            out.push(Token { tok: $t, line: $l, col: $c })
        };
    }
    while i < c.len() {
        let ch = c[i];
        let (l0, c0) = (line, col);
        if ch == '\n' {
            push!(Tok::Newline, line, col);
            i += 1;
            line += 1;
            col = 1;
            continue;
        }
        if ch == ';' {
            // Z#++ allows `;` as an explicit statement end.
            push!(Tok::Newline, line, col);
            i += 1;
            col += 1;
            continue;
        }
        if ch.is_whitespace() {
            i += 1;
            col += 1;
            continue;
        }
        if ch == '/' && c.get(i + 1) == Some(&'/') {
            while i < c.len() && c[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if ch == '/' && c.get(i + 1) == Some(&'*') {
            i += 2;
            col += 2;
            while i < c.len() && !(c[i] == '*' && c.get(i + 1) == Some(&'/')) {
                if c[i] == '\n' {
                    line += 1;
                    col = 1;
                } else {
                    col += 1;
                }
                i += 1;
            }
            if i >= c.len() {
                return Err(format!("line {l0}: comment never closed"));
            }
            i += 2;
            col += 2;
            continue;
        }
        if ch.is_ascii_digit() {
            let s = i;
            let mut float = false;
            if ch == '0' && matches!(c.get(i + 1), Some('x') | Some('X')) {
                i += 2;
                while i < c.len() && (c[i].is_ascii_hexdigit() || c[i] == '_') {
                    i += 1;
                }
                let t: String = c[s + 2..i].iter().filter(|x| **x != '_').collect();
                let v = i64::from_str_radix(&t, 16).map_err(|_| format!("line {l0}: bad hex number"))?;
                col += i - s;
                push!(Tok::Int(v), l0, c0);
                continue;
            }
            while i < c.len() && (c[i].is_ascii_digit() || c[i] == '_') {
                i += 1;
            }
            if i + 1 < c.len() && c[i] == '.' && c[i + 1].is_ascii_digit() {
                float = true;
                i += 1;
                while i < c.len() && (c[i].is_ascii_digit() || c[i] == '_') {
                    i += 1;
                }
            }
            if i < c.len() && (c[i] == 'e' || c[i] == 'E') && c.get(i + 1).is_some_and(|x| x.is_ascii_digit() || *x == '-' || *x == '+') {
                float = true;
                i += 2;
                while i < c.len() && c[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let t: String = c[s..i].iter().filter(|x| **x != '_').collect();
            col += i - s;
            if float || (i < c.len() && c[i] == 'f' && !c.get(i + 1).is_some_and(|x| x.is_alphanumeric())) {
                if i < c.len() && c[i] == 'f' {
                    i += 1;
                    col += 1;
                }
                push!(Tok::Float(t.parse().map_err(|_| format!("line {l0}: bad number"))?), l0, c0);
            } else {
                push!(Tok::Int(t.parse().map_err(|_| format!("line {l0}: number too large"))?), l0, c0);
            }
            continue;
        }
        // HolyC / C: `#include "file"` is `import "file"`.
        if ch == '#' && c[i + 1..].iter().take(7).collect::<String>() == "include" {
            push!(Tok::Ident("import".into()), l0, c0);
            i += 8;
            col += 8;
            continue;
        }
        if ch == '#' && c.get(i + 1).is_some_and(|x| x.is_ascii_hexdigit()) {
            let s = i + 1;
            let mut j = s;
            while j < c.len() && c[j].is_ascii_hexdigit() {
                j += 1;
            }
            let hex: String = c[s..j].iter().collect();
            let v = match hex.len() {
                3 => {
                    let d: Vec<u32> = hex.chars().map(|h| h.to_digit(16).unwrap()).collect();
                    ((d[0] * 17) << 24) | ((d[1] * 17) << 16) | ((d[2] * 17) << 8) | 0xff
                }
                6 => (u32::from_str_radix(&hex, 16).unwrap() << 8) | 0xff,
                8 => u32::from_str_radix(&hex, 16).unwrap(),
                _ => return Err(format!("line {l0}: a colour is #rgb, #rrggbb or #rrggbbaa")),
            };
            col += j - i;
            i = j;
            push!(Tok::Color(v), l0, c0);
            continue;
        }
        if ch.is_alphabetic() || ch == '_' {
            let s = i;
            while i < c.len() && (c[i].is_alphanumeric() || c[i] == '_') {
                i += 1;
            }
            col += i - s;
            push!(Tok::Ident(c[s..i].iter().collect()), l0, c0);
            continue;
        }
        if ch == '"' || ch == '\'' {
            let q = ch;
            i += 1;
            col += 1;
            let mut parts = vec![];
            let mut lit = String::new();
            loop {
                let Some(&x) = c.get(i) else { return Err(format!("line {l0}: string never closed")) };
                if x == q {
                    i += 1;
                    col += 1;
                    break;
                }
                if x == '\n' {
                    return Err(format!("line {line}: string runs past the end of the line (use \\n)"));
                }
                if x == '\\' {
                    let e = *c.get(i + 1).ok_or(format!("line {line}: string never closed"))?;
                    // `\xHH` and `\u{H…}`: a character by its code
                    if e == 'x' || e == 'u' {
                        let (digits, skip) = if e == 'x' {
                            let d: String = c[i + 2..].iter().take(2).take_while(|d| d.is_ascii_hexdigit()).collect();
                            let n = d.len();
                            (d, 2 + n)
                        } else if c.get(i + 2) == Some(&'{') {
                            let d: String = c[i + 3..].iter().take_while(|d| d.is_ascii_hexdigit()).collect();
                            let n = d.len();
                            if c.get(i + 3 + n) != Some(&'}') {
                                return Err(format!("line {line}: \\u{{…}} never closed"));
                            }
                            (d, 4 + n)
                        } else {
                            (String::new(), 2)
                        };
                        let ch = u32::from_str_radix(&digits, 16).ok().and_then(char::from_u32);
                        match ch {
                            Some(ch) if !digits.is_empty() => lit.push(ch),
                            _ => return Err(format!("line {line}: bad \\{e} escape (use \\x1b or \\u{{1F600}})")),
                        }
                        i += skip;
                        col += skip;
                        continue;
                    }
                    lit.push(match e {
                        'n' => '\n',
                        't' => '\t',
                        'r' => '\r',
                        '0' => '\0',
                        'e' => '\x1b',
                        'a' => '\x07',
                        'b' => '\x08',
                        '$' => '$',
                        other => other,
                    });
                    i += 2;
                    col += 2;
                    continue;
                }
                if x == '$' && c.get(i + 1) == Some(&'{') {
                    let mut depth = 1;
                    let mut j = i + 2;
                    while j < c.len() && depth > 0 {
                        match c[j] {
                            '{' => depth += 1,
                            '}' => depth -= 1,
                            '\n' => return Err(format!("line {line}: `${{` never closed")),
                            _ => {}
                        }
                        j += 1;
                    }
                    if depth > 0 {
                        return Err(format!("line {line}: `${{` never closed"));
                    }
                    parts.push(StrPart::Lit(std::mem::take(&mut lit)));
                    parts.push(StrPart::Expr(c[i + 2..j - 1].iter().collect(), line));
                    col += j - i;
                    i = j;
                    continue;
                }
                lit.push(x);
                i += 1;
                col += 1;
            }
            parts.push(StrPart::Lit(lit));
            push!(Tok::Str(parts), l0, c0);
            continue;
        }
        let rest: String = c[i..(i + 3).min(c.len())].iter().collect();
        match OPS.iter().find(|o| rest.starts_with(**o)) {
            Some(o) => {
                i += o.len();
                col += o.len();
                push!(Tok::Op(o), l0, c0);
            }
            None => return Err(format!("line {line}, column {col}: unexpected character `{ch}`")),
        }
    }
    out.push(Token { tok: Tok::Newline, line, col });
    out.push(Token { tok: Tok::Eof, line, col });
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens() {
        let t: Vec<Tok> = lex("int x = 0x1F + 2.5 // c\nlet s = \"a${x}b\" #f80").unwrap().into_iter().map(|t| t.tok).collect();
        assert_eq!(t[0], Tok::Ident("int".into()));
        assert_eq!(t[3], Tok::Int(31));
        assert_eq!(t[5], Tok::Float(2.5));
        assert_eq!(t[6], Tok::Newline);
        assert_eq!(
            t[10],
            Tok::Str(vec![StrPart::Lit("a".into()), StrPart::Expr("x".into(), 2), StrPart::Lit("b".into())])
        );
        assert_eq!(t[11], Tok::Color(0xff8800ff));
        assert!(lex("\"open").is_err());
    }
}
