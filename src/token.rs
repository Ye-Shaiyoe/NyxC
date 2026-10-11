#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    pub col: usize,
}

impl Span {
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    // Keywords
    Fn,
    Let,
    Mut,
    Const,
    Return,
    If,
    Else,
    While,
    For,
    Import,
    Syscall,
    Extern,
    Struct,
    True,
    False,

    // Types
    TypeI8,
    TypeI16,
    TypeI32,
    TypeU8,
    TypeU16,
    TypeU32,
    TypeBool,
    TypeVoid,

    // Literals
    Ident(String),
    IntLit(i64),
    StringLit(String),
    CharLit(u8),

    // Symbols & Operators
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Colon,
    PathSep,      // ::
    Dot,          // .
    Semicolon,
    Arrow,        // ->
    Newline,

    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Eq,           // =
    EqEq,         // ==
    NotEq,        // !=
    Lt,           // <
    LtEq,         // <=
    Gt,           // >
    GtEq,         // >=
    AmpAmp,       // &&
    PipePipe,     // ||
    Bang,         // !
    Amp,          // &
    Pipe,         // |
    Caret,        // ^
    Tilde,        // ~
    Shl,          // <<
    Shr,          // >>

    Eof,
}

impl TokenKind {
    pub fn describe(&self) -> String {
        use TokenKind::*;
        match self {
            Fn => "keyword 'fn'".into(),
            Let => "keyword 'let'".into(),
            Mut => "keyword 'mut'".into(),
            Const => "keyword 'const'".into(),
            Return => "keyword 'return'".into(),
            If => "keyword 'if'".into(),
            Else => "keyword 'else'".into(),
            While => "keyword 'while'".into(),
            For => "keyword 'for'".into(),
            Import => "keyword 'import'".into(),
            Syscall => "keyword 'syscall'".into(),
            Extern => "keyword 'extern'".into(),
            Struct => "keyword 'struct'".into(),
            True => "literal 'true'".into(),
            False => "literal 'false'".into(),
            TypeI8 => "type 'i8'".into(),
            TypeI16 => "type 'i16'".into(),
            TypeI32 => "type 'i32'".into(),
            TypeU8 => "type 'u8'".into(),
            TypeU16 => "type 'u16'".into(),
            TypeU32 => "type 'u32'".into(),
            TypeBool => "type 'bool'".into(),
            TypeVoid => "type 'void'".into(),
            Ident(id) => format!("'{}'", id),
            IntLit(n) => format!("'{}'", n),
            StringLit(s) => format!("\"{}\"", s),
            CharLit(c) => format!("'{}'", *c as char),
            LParen => "'('".into(),
            RParen => "')'".into(),
            LBrace => "'{'".into(),
            RBrace => "'}'".into(),
            LBracket => "'['".into(),
            RBracket => "']'".into(),
            Comma => "','".into(),
            Colon => "':'".into(),
            PathSep => "'::'".into(),
            Dot => "'.'".into(),
            Semicolon => "';'".into(),
            Arrow => "'->'".into(),
            Newline => "newline".into(),
            Plus => "'+'".into(),
            Minus => "'-'".into(),
            Star => "'*'".into(),
            Slash => "'/'".into(),
            Percent => "'%'".into(),
            Eq => "'='".into(),
            EqEq => "'=='".into(),
            NotEq => "'!='".into(),
            Lt => "'<'".into(),
            LtEq => "'<='".into(),
            Gt => "'>'".into(),
            GtEq => "'>='".into(),
            AmpAmp => "'&&'".into(),
            PipePipe => "'||'".into(),
            Bang => "'!'".into(),
            Amp => "'&'".into(),
            Pipe => "'|'".into(),
            Caret => "'^'".into(),
            Tilde => "'~'".into(),
            Shl => "'<<'".into(),
            Shr => "'>>'".into(),
            Eof => "end of file".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}
