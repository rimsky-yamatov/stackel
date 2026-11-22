//! Lexer: turns source text into tokens.
//!
//! Tokens are separated by whitespace, except that `[ ] ( ) ,` are always
//! tokens of their own. `#` starts a comment that runs to the end of the line.

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Int(i64),
    Float(f64),
    Str(String),
    Word(String),
    LBracket,
    RBracket,
    LParen,
    RParen,
    Comma,
    /// `|`
    Bar,
    /// `::`
    ColonColon,
    /// `=`
    Equals,
    /// `--` (separates inputs from outputs in a stack-effect type)
    DashDash,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub line: usize,
    /// 0-based column (used for layout: a token at column 0 starts a top-level item).
    pub col: usize,
}

fn is_delim(c: char) -> bool {
    c.is_whitespace() || matches!(c, '[' | ']' | '(' | ')' | ',')
}

fn classify(w: &str) -> std::result::Result<Tok, String> {
    match w {
        "|" => return Ok(Tok::Bar),
        "::" => return Ok(Tok::ColonColon),
        "=" => return Ok(Tok::Equals),
        "--" => return Ok(Tok::DashDash),
        _ => {}
    }
    let mut it = w.chars();
    let numeric = match it.next() {
        Some(c) if c.is_ascii_digit() => true,
        Some('-') | Some('+') => matches!(it.next(), Some(d) if d.is_ascii_digit()),
        _ => false,
    };
    if !numeric {
        return Ok(Tok::Word(w.to_string()));
    }
    let digits = w
        .strip_prefix('-')
        .or_else(|| w.strip_prefix('+'))
        .unwrap_or(w);
    if digits.chars().all(|c| c.is_ascii_digit()) {
        return w
            .parse::<i64>()
            .map(Tok::Int)
            .map_err(|_| format!("integer literal out of range: {}", w));
    }
    w.parse::<f64>()
        .map(Tok::Float)
        .map_err(|_| format!("invalid number literal: {}", w))
}

pub fn lex(src: &str) -> Result<Vec<Token>> {
    let chars: Vec<char> = src.chars().collect();
    let mut toks = Vec::new();
    let (mut i, mut line, mut col) = (0usize, 1usize, 0usize);

    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            col = 0;
            i += 1;
            continue;
        }
        if c.is_whitespace() {
            i += 1;
            col += 1;
            continue;
        }
        if c == '#' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
                col += 1;
            }
            continue;
        }

        let (sl, sc) = (line, col);

        let simple = match c {
            '[' => Some(Tok::LBracket),
            ']' => Some(Tok::RBracket),
            '(' => Some(Tok::LParen),
            ')' => Some(Tok::RParen),
            ',' => Some(Tok::Comma),
            _ => None,
        };
        if let Some(t) = simple {
            toks.push(Token {
                tok: t,
                line: sl,
                col: sc,
            });
            i += 1;
            col += 1;
            continue;
        }

        if c == '"' {
            i += 1;
            col += 1;
            let mut s = String::new();
            loop {
                if i >= chars.len() {
                    return Err(Error::new("unterminated string literal", sl, sc + 1));
                }
                let d = chars[i];
                i += 1;
                col += 1;
                match d {
                    '"' => break,
                    '\\' => {
                        if i >= chars.len() {
                            return Err(Error::new("unterminated string literal", sl, sc + 1));
                        }
                        let e = chars[i];
                        i += 1;
                        col += 1;
                        match e {
                            'n' => s.push('\n'),
                            't' => s.push('\t'),
                            '\\' => s.push('\\'),
                            '"' => s.push('"'),
                            other => {
                                return Err(Error::new(
                                    format!("unknown escape sequence: \\{}", other),
                                    line,
                                    col,
                                ))
                            }
                        }
                    }
                    '\n' => {
                        line += 1;
                        col = 0;
                        s.push('\n');
                    }
                    _ => s.push(d),
                }
            }
            toks.push(Token {
                tok: Tok::Str(s),
                line: sl,
                col: sc,
            });
            continue;
        }

        let start = i;
        while i < chars.len() && !is_delim(chars[i]) {
            i += 1;
            col += 1;
        }
        let w: String = chars[start..i].iter().collect();
        match classify(&w) {
            Ok(t) => toks.push(Token {
                tok: t,
                line: sl,
                col: sc,
            }),
            Err(m) => return Err(Error::new(m, sl, sc + 1)),
        }
    }
    Ok(toks)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &str) -> Vec<Tok> {
        lex(s).unwrap().into_iter().map(|t| t.tok).collect()
    }

    fn word(s: &str) -> Tok {
        Tok::Word(s.to_string())
    }

    #[test]
    fn numbers_and_words() {
        assert_eq!(
            toks("1 -2 +3 3.5 dup - inf"),
            vec![
                Tok::Int(1),
                Tok::Int(-2),
                Tok::Int(3),
                Tok::Float(3.5),
                word("dup"),
                word("-"),
                word("inf"),
            ]
        );
    }

    #[test]
    fn symbols() {
        assert_eq!(
            toks("f :: Int -- Int ! IO = [ ] | ( , )"),
            vec![
                word("f"),
                Tok::ColonColon,
                word("Int"),
                Tok::DashDash,
                word("Int"),
                word("!"),
                word("IO"),
                Tok::Equals,
                Tok::LBracket,
                Tok::RBracket,
                Tok::Bar,
                Tok::LParen,
                Tok::Comma,
                Tok::RParen,
            ]
        );
    }

    #[test]
    fn brackets_split_words() {
        assert_eq!(
            toks("[dup]"),
            vec![Tok::LBracket, word("dup"), Tok::RBracket]
        );
    }

    #[test]
    fn string_literals_and_escapes() {
        assert_eq!(
            toks("\"a\\nb\\\"c\""),
            vec![Tok::Str("a\nb\"c".to_string())]
        );
    }

    #[test]
    fn comments_are_skipped() {
        assert_eq!(toks("1 # comment here\n2"), vec![Tok::Int(1), Tok::Int(2)]);
    }

    #[test]
    fn positions() {
        let t = lex("a\n  b").unwrap();
        assert_eq!((t[0].line, t[0].col), (1, 0));
        assert_eq!((t[1].line, t[1].col), (2, 2));
    }

    #[test]
    fn errors() {
        assert!(lex("\"abc").is_err());
        assert!(lex("99999999999999999999").is_err());
        assert!(lex("1.2.3").is_err());
        assert!(lex("\"\\q\"").is_err());
    }
}
