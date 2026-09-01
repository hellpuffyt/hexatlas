//! Hand-written lexer for the hexatlas template DSL.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Ident(String),
    Number(u64),
    Str(String),
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Colon,
    Semicolon,
    Comma,
    Eof,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Ident(s) => write!(f, "identifier `{s}`"),
            Token::Number(n) => write!(f, "number `{n}`"),
            Token::Str(s) => write!(f, "string \"{s}\""),
            Token::LBrace => write!(f, "'{{'"),
            Token::RBrace => write!(f, "'}}'"),
            Token::LBracket => write!(f, "'['"),
            Token::RBracket => write!(f, "']'"),
            Token::Colon => write!(f, "':'"),
            Token::Semicolon => write!(f, "';'"),
            Token::Comma => write!(f, "','"),
            Token::Eof => write!(f, "end of file"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpannedToken {
    pub token: Token,
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub message: String,
    pub line: usize,
    pub col: usize,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}, col {}: {}", self.line, self.col, self.message)
    }
}

pub fn lex(source: &str) -> Result<Vec<SpannedToken>, LexError> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0usize;
    let mut line = 1usize;
    let mut col = 1usize;

    macro_rules! advance {
        () => {{
            if chars[i] == '\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
            i += 1;
        }};
    }

    while i < chars.len() {
        let c = chars[i];

        if c == '#' {
            while i < chars.len() && chars[i] != '\n' {
                advance!();
            }
            continue;
        }
        if c.is_whitespace() {
            advance!();
            continue;
        }

        let (start_line, start_col) = (line, col);

        match c {
            '{' => {
                advance!();
                tokens.push(SpannedToken {
                    token: Token::LBrace,
                    line: start_line,
                    col: start_col,
                });
            }
            '}' => {
                advance!();
                tokens.push(SpannedToken {
                    token: Token::RBrace,
                    line: start_line,
                    col: start_col,
                });
            }
            '[' => {
                advance!();
                tokens.push(SpannedToken {
                    token: Token::LBracket,
                    line: start_line,
                    col: start_col,
                });
            }
            ']' => {
                advance!();
                tokens.push(SpannedToken {
                    token: Token::RBracket,
                    line: start_line,
                    col: start_col,
                });
            }
            ':' => {
                advance!();
                tokens.push(SpannedToken {
                    token: Token::Colon,
                    line: start_line,
                    col: start_col,
                });
            }
            ';' => {
                advance!();
                tokens.push(SpannedToken {
                    token: Token::Semicolon,
                    line: start_line,
                    col: start_col,
                });
            }
            ',' => {
                advance!();
                tokens.push(SpannedToken {
                    token: Token::Comma,
                    line: start_line,
                    col: start_col,
                });
            }
            '"' => {
                advance!(); // consume opening quote
                let mut s = String::new();
                let mut closed = false;
                while i < chars.len() {
                    if chars[i] == '"' {
                        advance!();
                        closed = true;
                        break;
                    }
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        advance!();
                        let escaped = chars[i];
                        let literal = match escaped {
                            'n' => '\n',
                            't' => '\t',
                            '0' => '\0',
                            '\\' => '\\',
                            '"' => '"',
                            other => other,
                        };
                        s.push(literal);
                        advance!();
                        continue;
                    }
                    s.push(chars[i]);
                    advance!();
                }
                if !closed {
                    return Err(LexError {
                        message: "unterminated string literal".to_string(),
                        line: start_line,
                        col: start_col,
                    });
                }
                tokens.push(SpannedToken {
                    token: Token::Str(s),
                    line: start_line,
                    col: start_col,
                });
            }
            c if c.is_ascii_digit() => {
                let mut s = String::new();
                if c == '0' && i + 1 < chars.len() && (chars[i + 1] == 'x' || chars[i + 1] == 'X') {
                    s.push(chars[i]);
                    advance!();
                    s.push(chars[i]);
                    advance!();
                    while i < chars.len() && (chars[i].is_ascii_hexdigit() || chars[i] == '_') {
                        if chars[i] != '_' {
                            s.push(chars[i]);
                        }
                        advance!();
                    }
                    let value = u64::from_str_radix(&s[2..], 16).map_err(|e| LexError {
                        message: format!("invalid hex literal `{s}`: {e}"),
                        line: start_line,
                        col: start_col,
                    })?;
                    tokens.push(SpannedToken {
                        token: Token::Number(value),
                        line: start_line,
                        col: start_col,
                    });
                } else {
                    while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '_') {
                        if chars[i] != '_' {
                            s.push(chars[i]);
                        }
                        advance!();
                    }
                    let value = s.parse::<u64>().map_err(|e| LexError {
                        message: format!("invalid number literal `{s}`: {e}"),
                        line: start_line,
                        col: start_col,
                    })?;
                    tokens.push(SpannedToken {
                        token: Token::Number(value),
                        line: start_line,
                        col: start_col,
                    });
                }
            }
            c if c.is_alphabetic() || c == '_' => {
                let mut s = String::new();
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    s.push(chars[i]);
                    advance!();
                }
                tokens.push(SpannedToken {
                    token: Token::Ident(s),
                    line: start_line,
                    col: start_col,
                });
            }
            other => {
                return Err(LexError {
                    message: format!("unexpected character '{other}'"),
                    line: start_line,
                    col: start_col,
                });
            }
        }
    }

    tokens.push(SpannedToken {
        token: Token::Eof,
        line,
        col,
    });
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_basic_tokens() {
        let toks = lex("struct A { x: u8; }").unwrap();
        let kinds: Vec<Token> = toks.into_iter().map(|t| t.token).collect();
        assert_eq!(
            kinds,
            vec![
                Token::Ident("struct".into()),
                Token::Ident("A".into()),
                Token::LBrace,
                Token::Ident("x".into()),
                Token::Colon,
                Token::Ident("u8".into()),
                Token::Semicolon,
                Token::RBrace,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn lexes_hex_and_decimal_numbers() {
        let toks = lex("0x89504E47 42 1_000").unwrap();
        let nums: Vec<u64> = toks
            .into_iter()
            .filter_map(|t| match t.token {
                Token::Number(n) => Some(n),
                _ => None,
            })
            .collect();
        assert_eq!(nums, vec![0x89504E47, 42, 1000]);
    }

    #[test]
    fn lexes_string_with_escapes() {
        let toks = lex(r#""PNG\0" "a\"b""#).unwrap();
        let strs: Vec<String> = toks
            .into_iter()
            .filter_map(|t| match t.token {
                Token::Str(s) => Some(s),
                _ => None,
            })
            .collect();
        assert_eq!(strs, vec!["PNG\0".to_string(), "a\"b".to_string()]);
    }

    #[test]
    fn skips_comments() {
        let toks = lex("# a comment\nstruct # trailing\nA {}").unwrap();
        let kinds: Vec<Token> = toks.into_iter().map(|t| t.token).collect();
        assert_eq!(
            kinds,
            vec![
                Token::Ident("struct".into()),
                Token::Ident("A".into()),
                Token::LBrace,
                Token::RBrace,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn reports_unterminated_string() {
        let err = lex("\"abc").unwrap_err();
        assert!(err.message.contains("unterminated"));
    }

    #[test]
    fn reports_unexpected_character() {
        let err = lex("struct A { x: u8 ~ }").unwrap_err();
        assert!(err.message.contains("unexpected character"));
    }

    #[test]
    fn tracks_line_and_col() {
        let toks = lex("a\nb").unwrap();
        assert_eq!(toks[0].line, 1);
        assert_eq!(toks[1].line, 2);
        assert_eq!(toks[1].col, 1);
    }
}
