use crate::error::CompileError;
use crate::token::{Span, Token, TokenKind};

pub struct Lexer<'a> {
    src: &'a [u8],
    cursor: usize,
    line: usize,
    col: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            src: source.as_bytes(),
            cursor: 0,
            line: 1,
            col: 1,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.cursor).copied()
    }

    fn peek_next(&self) -> Option<u8> {
        self.src.get(self.cursor + 1).copied()
    }

    fn advance(&mut self) -> Option<u8> {
        let ch = self.peek()?;
        self.cursor += 1;
        if ch == b'\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(ch)
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, CompileError> {
        let mut tokens: Vec<Token> = Vec::new();

        while let Some(ch) = self.peek() {
            let span = Span::new(self.line, self.col);

            match ch {
                b' ' | b'\t' | b'\r' => {
                    self.advance();
                }
                b'\n' => {
                    self.advance();
                    // Avoid duplicate newlines
                    if let Some(last) = tokens.last() {
                        if last.kind != TokenKind::Newline {
                            tokens.push(Token::new(TokenKind::Newline, span));
                        }
                    }
                }
                b'/' => {
                    if self.peek_next() == Some(b'/') {
                        // Single-line comment
                        while let Some(c) = self.peek() {
                            if c == b'\n' {
                                break;
                            }
                            self.advance();
                        }
                    } else if self.peek_next() == Some(b'*') {
                        // Multi-line comment
                        self.advance(); // consume '/'
                        self.advance(); // consume '*'
                        let mut closed = false;
                        while let Some(c) = self.peek() {
                            if c == b'*' && self.peek_next() == Some(b'/') {
                                self.advance();
                                self.advance();
                                closed = true;
                                break;
                            }
                            self.advance();
                        }
                        if !closed {
                            return Err(CompileError::new(
                                "LEX_003",
                                "unterminated block comment",
                                Some(span),
                            ));
                        }
                    } else {
                        self.advance();
                        tokens.push(Token::new(TokenKind::Slash, span));
                    }
                }
                b'"' => {
                    let s = self.lex_string(span)?;
                    tokens.push(Token::new(TokenKind::StringLit(s), span));
                }
                b'\'' => {
                    let c = self.lex_char(span)?;
                    tokens.push(Token::new(TokenKind::CharLit(c), span));
                }
                b'0'..=b'9' => {
                    let n = self.lex_number(span)?;
                    tokens.push(Token::new(TokenKind::IntLit(n), span));
                }
                b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                    let ident = self.lex_ident();
                    let kind = match ident.as_str() {
                        "fn" => TokenKind::Fn,
                        "let" => TokenKind::Let,
                        "mut" => TokenKind::Mut,
                        "const" => TokenKind::Const,
                        "return" => TokenKind::Return,
                        "if" => TokenKind::If,
                        "else" => TokenKind::Else,
                        "while" => TokenKind::While,
                        "for" => TokenKind::For,
                        "break" => TokenKind::Break,
                        "continue" => TokenKind::Continue,
                        "import" => TokenKind::Import,
                        "syscall" => TokenKind::Syscall,
                        "extern" => TokenKind::Extern,
                        "struct" => TokenKind::Struct,
                        "true" => TokenKind::True,
                        "false" => TokenKind::False,
                        "i8" => TokenKind::TypeI8,
                        "i16" => TokenKind::TypeI16,
                        "i32" => TokenKind::TypeI32,
                        "u8" => TokenKind::TypeU8,
                        "u16" => TokenKind::TypeU16,
                        "u32" => TokenKind::TypeU32,
                        "bool" => TokenKind::TypeBool,
                        "void" => TokenKind::TypeVoid,
                        _ => TokenKind::Ident(ident),
                    };
                    tokens.push(Token::new(kind, span));
                }
                b'(' => { self.advance(); tokens.push(Token::new(TokenKind::LParen, span)); }
                b')' => { self.advance(); tokens.push(Token::new(TokenKind::RParen, span)); }
                b'{' => { self.advance(); tokens.push(Token::new(TokenKind::LBrace, span)); }
                b'}' => { self.advance(); tokens.push(Token::new(TokenKind::RBrace, span)); }
                b'[' => { self.advance(); tokens.push(Token::new(TokenKind::LBracket, span)); }
                b']' => { self.advance(); tokens.push(Token::new(TokenKind::RBracket, span)); }
                b',' => { self.advance(); tokens.push(Token::new(TokenKind::Comma, span)); }
                b';' => { self.advance(); tokens.push(Token::new(TokenKind::Semicolon, span)); }
                b'.' => { self.advance(); tokens.push(Token::new(TokenKind::Dot, span)); }
                b'+' => { self.advance(); tokens.push(Token::new(TokenKind::Plus, span)); }
                b'*' => { self.advance(); tokens.push(Token::new(TokenKind::Star, span)); }
                b'%' => { self.advance(); tokens.push(Token::new(TokenKind::Percent, span)); }
                b'^' => { self.advance(); tokens.push(Token::new(TokenKind::Caret, span)); }
                b'~' => { self.advance(); tokens.push(Token::new(TokenKind::Tilde, span)); }
                b'-' => {
                    self.advance();
                    if self.peek() == Some(b'>') {
                        self.advance();
                        tokens.push(Token::new(TokenKind::Arrow, span));
                    } else {
                        tokens.push(Token::new(TokenKind::Minus, span));
                    }
                }
                b':' => {
                    self.advance();
                    if self.peek() == Some(b':') {
                        self.advance();
                        tokens.push(Token::new(TokenKind::PathSep, span));
                    } else {
                        tokens.push(Token::new(TokenKind::Colon, span));
                    }
                }
                b'=' => {
                    self.advance();
                    if self.peek() == Some(b'=') {
                        self.advance();
                        tokens.push(Token::new(TokenKind::EqEq, span));
                    } else {
                        tokens.push(Token::new(TokenKind::Eq, span));
                    }
                }
                b'!' => {
                    self.advance();
                    if self.peek() == Some(b'=') {
                        self.advance();
                        tokens.push(Token::new(TokenKind::NotEq, span));
                    } else {
                        tokens.push(Token::new(TokenKind::Bang, span));
                    }
                }
                b'<' => {
                    self.advance();
                    if self.peek() == Some(b'=') {
                        self.advance();
                        tokens.push(Token::new(TokenKind::LtEq, span));
                    } else if self.peek() == Some(b'<') {
                        self.advance();
                        tokens.push(Token::new(TokenKind::Shl, span));
                    } else {
                        tokens.push(Token::new(TokenKind::Lt, span));
                    }
                }
                b'>' => {
                    self.advance();
                    if self.peek() == Some(b'=') {
                        self.advance();
                        tokens.push(Token::new(TokenKind::GtEq, span));
                    } else if self.peek() == Some(b'>') {
                        self.advance();
                        tokens.push(Token::new(TokenKind::Shr, span));
                    } else {
                        tokens.push(Token::new(TokenKind::Gt, span));
                    }
                }
                b'&' => {
                    self.advance();
                    if self.peek() == Some(b'&') {
                        self.advance();
                        tokens.push(Token::new(TokenKind::AmpAmp, span));
                    } else {
                        tokens.push(Token::new(TokenKind::Amp, span));
                    }
                }
                b'|' => {
                    self.advance();
                    if self.peek() == Some(b'|') {
                        self.advance();
                        tokens.push(Token::new(TokenKind::PipePipe, span));
                    } else {
                        tokens.push(Token::new(TokenKind::Pipe, span));
                    }
                }
                _ => {
                    return Err(CompileError::new(
                        "LEX_001",
                        format!("unexpected character '{}'", ch as char),
                        Some(Span::new(self.line, self.col)),
                    ));
                }
            }
        }

        tokens.push(Token::new(TokenKind::Eof, Span::new(self.line, self.col)));
        Ok(tokens)
    }

    fn lex_ident(&mut self) -> String {
        let mut s = String::new();
        while let Some(ch) = self.peek() {
            if ch.is_ascii_alphanumeric() || ch == b'_' {
                s.push(ch as char);
                self.advance();
            } else {
                break;
            }
        }
        s
    }

    fn lex_number(&mut self, span: Span) -> Result<i64, CompileError> {
        let mut s = String::new();
        if self.peek() == Some(b'0') {
            if let Some(next) = self.peek_next() {
                if next == b'x' || next == b'X' {
                    self.advance(); // 0
                    self.advance(); // x
                    while let Some(c) = self.peek() {
                        if c.is_ascii_hexdigit() {
                            s.push(c as char);
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    return i64::from_str_radix(&s, 16).map_err(|_| {
                        CompileError::new("LEX_004", "invalid hex literal", Some(span))
                    });
                } else if next == b'b' || next == b'B' {
                    self.advance(); // 0
                    self.advance(); // b
                    while let Some(c) = self.peek() {
                        if c == b'0' || c == b'1' {
                            s.push(c as char);
                            self.advance();
                        } else {
                            break;
                        }
                    }
                    return i64::from_str_radix(&s, 2).map_err(|_| {
                        CompileError::new("LEX_004", "invalid binary literal", Some(span))
                    });
                }
            }
        }

        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                s.push(c as char);
                self.advance();
            } else {
                break;
            }
        }
        s.parse::<i64>()
            .map_err(|_| CompileError::new("LEX_004", "invalid integer literal", Some(span)))
    }

    fn lex_string(&mut self, span: Span) -> Result<String, CompileError> {
        self.advance(); // consume opening '"'
        let mut s = String::new();
        while let Some(ch) = self.peek() {
            if ch == b'"' {
                self.advance();
                return Ok(s);
            }
            if ch == b'\\' {
                self.advance();
                let esc = self.advance().ok_or_else(|| {
                    CompileError::new("LEX_005", "unterminated escape sequence", Some(span))
                })?;
                match esc {
                    b'n' => s.push('\n'),
                    b't' => s.push('\t'),
                    b'r' => s.push('\r'),
                    b'0' => s.push('\0'),
                    b'\\' => s.push('\\'),
                    b'"' => s.push('"'),
                    _ => s.push(esc as char),
                }
            } else {
                s.push(ch as char);
                self.advance();
            }
        }
        Err(CompileError::new(
            "LEX_002",
            "unterminated string literal",
            Some(span),
        ))
    }

    fn lex_char(&mut self, span: Span) -> Result<u8, CompileError> {
        self.advance(); // consume opening '\''
        let ch = self.advance().ok_or_else(|| {
            CompileError::new("LEX_006", "empty character literal", Some(span))
        })?;
        let res = if ch == b'\\' {
            let esc = self.advance().ok_or_else(|| {
                CompileError::new("LEX_005", "unterminated escape sequence", Some(span))
            })?;
            match esc {
                b'n' => b'\n',
                b't' => b'\t',
                b'r' => b'\r',
                b'0' => 0,
                b'\\' => b'\\',
                b'\'' => b'\'',
                _ => esc,
            }
        } else {
            ch
        };
        if self.advance() != Some(b'\'') {
            return Err(CompileError::new("LEX_006", "unclosed character literal", Some(span)));
        }
        Ok(res)
    }
}
