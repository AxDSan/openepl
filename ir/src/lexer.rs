//! Tokenizer for the OpenEPL IR text encoding (`.oir`).
//!
//! Line structure is significant only in that statements end at a newline; the
//! lexer emits explicit `Newline` tokens and the parser consumes them.  Blank
//! lines and `# ...` comments are skipped here.

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    // Keywords
    Module,
    Sub,
    End,
    Let,
    Var,
    Call,
    If,
    Else,
    While,
    For,
    Break,
    Continue,
    And,
    Or,
    Not,
    /// `bnot` — bitwise complement. Reserved, exactly as `not` is.
    ///
    /// The *infix* bitwise words (`band`, `bor`, `bxor`, `shl`, `shr`, `ushr`)
    /// are soft keywords the parser recognises only in operator position,
    /// where an identifier could never have appeared — so a name spelled like
    /// one keeps working. A prefix operator has no such shelter: `bnot(x)` and
    /// `bnot -1` read as the operator and as a call or a subtraction equally
    /// well, and choosing wrong is a wrong answer rather than an error. So
    /// this one is a keyword, and a name spelled `bnot` is refused where it is
    /// written instead of quietly meaning something else.
    BNot,
    True,
    False,
    Use,
    Form,
    On,
    Return,
    // Literals / identifiers
    Ident(String),
    Int(i64),
    /// A hex or binary literal — `0xFF`, `0b1010`. Kept as the raw **bit
    /// pattern** rather than a signed value, because how wide it is depends on
    /// where it lands: `0x8000_0000` is `int` -2147483648 on its own and
    /// `int64` 2147483648 where an `int64` is wanted, and folding either
    /// reading in here would make the other one unreachable.
    Bits(u64),
    Float(f64),
    Str(String),
    // Punctuation / operators
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Colon,
    Dot,
    Eq,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Lt,
    Le,
    Gt,
    Ge,
    Ne,
    Newline,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Spanned {
    pub tok: Tok,
    pub line: usize,
    /// 1-based column of the token's first character. Together with `line` this
    /// is what lets the language server point at a symbol rather than a line.
    pub col: usize,
    /// One past the token's last byte, so a diagnostic can underline exactly
    /// the token — a string literal's spelling is longer than its value.
    pub end_col: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub line: usize,
    pub msg: String,
}

pub fn lex(src: &str) -> Result<Vec<Spanned>, LexError> {
    let mut out = Vec::new();
    let mut line = 1usize;
    let bytes = src.as_bytes();
    let mut i = 0usize;
    let n = bytes.len();
    // Offset of the current line's first byte, so a column is `i - line_start`.
    let mut line_start = 0usize;

    // `end_col` is filled in after the arm has consumed the token: every arm
    // knows where its token starts, and only the loop knows where `i` ends up.
    let push = |out: &mut Vec<Spanned>, tok: Tok, line: usize, col: usize| {
        out.push(Spanned {
            tok,
            line,
            col,
            end_col: 0,
        })
    };

    while i < n {
        let c = bytes[i];
        // Column of the token about to be read, for language-server positions.
        let start_col = i - line_start + 1;
        let before = out.len();
        match c {
            b'\n' => {
                // Collapse runs of blank lines into a single Newline token so the
                // parser's statement loop stays simple.
                if !matches!(
                    out.last(),
                    Some(Spanned {
                        tok: Tok::Newline,
                        ..
                    }) | None
                ) {
                    push(&mut out, Tok::Newline, line, start_col);
                    // Measured here: the generic fix-up below would read the
                    // column against the line that starts after this token.
                    out.last_mut().unwrap().end_col = start_col + 1;
                }
                line += 1;
                i += 1;
                line_start = i;
            }
            b' ' | b'\t' | b'\r' => i += 1,
            b'#' => {
                while i < n && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'(' => {
                push(&mut out, Tok::LParen, line, start_col);
                i += 1;
            }
            b')' => {
                push(&mut out, Tok::RParen, line, start_col);
                i += 1;
            }
            b'[' => {
                push(&mut out, Tok::LBracket, line, start_col);
                i += 1;
            }
            b']' => {
                push(&mut out, Tok::RBracket, line, start_col);
                i += 1;
            }
            b'{' => {
                push(&mut out, Tok::LBrace, line, start_col);
                i += 1;
            }
            b'}' => {
                push(&mut out, Tok::RBrace, line, start_col);
                i += 1;
            }
            b',' => {
                push(&mut out, Tok::Comma, line, start_col);
                i += 1;
            }
            b':' => {
                push(&mut out, Tok::Colon, line, start_col);
                i += 1;
            }
            b'.' => {
                push(&mut out, Tok::Dot, line, start_col);
                i += 1;
            }
            b'=' => {
                push(&mut out, Tok::Eq, line, start_col);
                i += 1;
            }
            b'<' => {
                // `<=` and `<>` before bare `<`.
                if i + 1 < n && bytes[i + 1] == b'=' {
                    push(&mut out, Tok::Le, line, start_col);
                    i += 2;
                } else if i + 1 < n && bytes[i + 1] == b'>' {
                    push(&mut out, Tok::Ne, line, start_col);
                    i += 2;
                } else {
                    push(&mut out, Tok::Lt, line, start_col);
                    i += 1;
                }
            }
            b'>' => {
                if i + 1 < n && bytes[i + 1] == b'=' {
                    push(&mut out, Tok::Ge, line, start_col);
                    i += 2;
                } else {
                    push(&mut out, Tok::Gt, line, start_col);
                    i += 1;
                }
            }
            b'+' => {
                push(&mut out, Tok::Plus, line, start_col);
                i += 1;
            }
            b'-' => {
                push(&mut out, Tok::Minus, line, start_col);
                i += 1;
            }
            b'*' => {
                push(&mut out, Tok::Star, line, start_col);
                i += 1;
            }
            b'/' => {
                push(&mut out, Tok::Slash, line, start_col);
                i += 1;
            }
            b'%' => {
                push(&mut out, Tok::Percent, line, start_col);
                i += 1;
            }
            b'"' => {
                let (s, ni) = lex_string(bytes, i + 1, line)?;
                push(&mut out, Tok::Str(s), line, start_col);
                i = ni;
            }
            // `0x...` / `0b...` — a bit pattern, before the decimal path, which
            // would otherwise read `0b1010` as the number 0 and the name
            // `b1010`.
            _ if c == b'0'
                && i + 1 < n
                && matches!(bytes[i + 1], b'x' | b'X' | b'b' | b'B') =>
            {
                let base = if matches!(bytes[i + 1], b'x' | b'X') { 16u32 } else { 2u32 };
                let start = i;
                i += 2;
                let digits_start = i;
                let mut value: u64 = 0;
                let mut digits = 0usize;
                while i < n && (bytes[i] == b'_' || bytes[i].is_ascii_alphanumeric()) {
                    if bytes[i] == b'_' {
                        i += 1;
                        continue;
                    }
                    let d = (bytes[i] as char).to_digit(base).ok_or_else(|| LexError {
                        line,
                        msg: format!(
                            "`{}` is not a {} digit in `{}`",
                            bytes[i] as char,
                            if base == 16 { "hexadecimal" } else { "binary" },
                            &src[start..i + 1]
                        ),
                    })?;
                    // More than 64 bits has no type to land in, and silently
                    // dropping the top of a bit pattern is the worst possible
                    // answer.
                    value = value
                        .checked_mul(base as u64)
                        .and_then(|v| v.checked_add(d as u64))
                        .ok_or_else(|| LexError {
                            line,
                            msg: format!(
                                "the literal `{}` needs more than 64 bits",
                                &src[start..i + 1]
                            ),
                        })?;
                    digits += 1;
                    i += 1;
                }
                if digits == 0 {
                    return Err(LexError {
                        line,
                        msg: format!(
                            "`{}` has no digits after it",
                            &src[start..digits_start]
                        ),
                    });
                }
                push(&mut out, Tok::Bits(value), line, start_col);
            }
            _ if c.is_ascii_digit() => {
                let start = i;
                while i < n && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                // Fractional part -> floating-point literal (needs a digit after `.`).
                let is_float = i + 1 < n && bytes[i] == b'.' && bytes[i + 1].is_ascii_digit();
                if is_float {
                    i += 1; // consume '.'
                    while i < n && bytes[i].is_ascii_digit() {
                        i += 1;
                    }
                    let text = &src[start..i];
                    let v: f64 = text.parse().map_err(|_| LexError {
                        line,
                        msg: format!("invalid float literal: {text}"),
                    })?;
                    push(&mut out, Tok::Float(v), line, start_col);
                } else {
                    let text = &src[start..i];
                    let v: i64 = text.parse().map_err(|_| LexError {
                        line,
                        msg: format!("integer literal out of range: {text}"),
                    })?;
                    push(&mut out, Tok::Int(v), line, start_col);
                }
            }
            _ if is_ident_start(c) => {
                let start = i;
                while i < n && is_ident_cont(bytes[i]) {
                    i += 1;
                }
                let word = &src[start..i];
                let tok = match word {
                    "module" => Tok::Module,
                    "sub" => Tok::Sub,
                    "end" => Tok::End,
                    "let" => Tok::Let,
                    "var" => Tok::Var,
                    "if" => Tok::If,
                    "else" => Tok::Else,
                    "while" => Tok::While,
                    "for" => Tok::For,
                    "break" => Tok::Break,
                    "continue" => Tok::Continue,
                    "and" => Tok::And,
                    "or" => Tok::Or,
                    "not" => Tok::Not,
                    "bnot" => Tok::BNot,
                    "true" => Tok::True,
                    "false" => Tok::False,
                    "call" => Tok::Call,
                    "use" => Tok::Use,
                    "form" => Tok::Form,
                    "on" => Tok::On,
                    "return" => Tok::Return,
                    _ => Tok::Ident(word.to_string()),
                };
                push(&mut out, tok, line, start_col);
            }
            _ => {
                return Err(LexError {
                    line,
                    msg: format!("unexpected character: {:?}", c as char),
                });
            }
        }
        if out.len() > before {
            let last = out.last_mut().unwrap();
            if last.end_col == 0 {
                last.end_col = i - line_start + 1;
            }
        }
    }
    out.push(Spanned {
        tok: Tok::Eof,
        line,
        col: i - line_start + 1,
        end_col: i - line_start + 1,
    });
    Ok(out)
}

fn is_ident_start(c: u8) -> bool {
    c == b'_' || c.is_ascii_alphabetic()
}
fn is_ident_cont(c: u8) -> bool {
    c == b'_' || c.is_ascii_alphanumeric()
}

/// Lex a string body starting just after the opening quote; returns the decoded
/// string and the index just past the closing quote.  Supports `\n \t \\ \" \0`.
fn lex_string(bytes: &[u8], mut i: usize, line: usize) -> Result<(String, usize), LexError> {
    let mut s = String::new();
    let n = bytes.len();
    loop {
        if i >= n {
            return Err(LexError {
                line,
                msg: "unterminated string literal".into(),
            });
        }
        match bytes[i] {
            b'"' => return Ok((s, i + 1)),
            b'\n' => {
                return Err(LexError {
                    line,
                    msg: "newline in string literal".into(),
                })
            }
            b'\\' => {
                i += 1;
                if i >= n {
                    return Err(LexError {
                        line,
                        msg: "unterminated escape".into(),
                    });
                }
                match bytes[i] {
                    b'n' => s.push('\n'),
                    b't' => s.push('\t'),
                    b'\\' => s.push('\\'),
                    b'"' => s.push('"'),
                    b'0' => s.push('\0'),
                    other => {
                        return Err(LexError {
                            line,
                            msg: format!("unknown escape: \\{}", other as char),
                        })
                    }
                }
                i += 1;
            }
            _ => {
                // Pass raw bytes through as-is (UTF-8 preserved).
                let start = i;
                while i < n && bytes[i] != b'"' && bytes[i] != b'\\' && bytes[i] != b'\n' {
                    i += 1;
                }
                s.push_str(std::str::from_utf8(&bytes[start..i]).map_err(|_| LexError {
                    line,
                    msg: "invalid UTF-8 in string literal".into(),
                })?);
            }
        }
    }
}
