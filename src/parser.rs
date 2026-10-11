use crate::ast::*;
use crate::error::CompileError;
use crate::token::{Token, TokenKind};

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.peek().kind
    }

    fn is_eof(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Eof)
    }

    fn advance(&mut self) -> Token {
        let tok = self.peek().clone();
        if !self.is_eof() {
            self.pos += 1;
        }
        tok
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek_kind(), TokenKind::Newline) {
            self.advance();
        }
    }

    fn expect(&mut self, expected: TokenKind) -> Result<Token, CompileError> {
        let tok = self.peek().clone();
        if std::mem::discriminant(&tok.kind) == std::mem::discriminant(&expected) {
            self.advance();
            Ok(tok)
        } else {
            Err(CompileError::new(
                "PAR_001",
                format!("expected {}, found {}", expected.describe(), tok.kind.describe()),
                Some(tok.span),
            ))
        }
    }

    fn match_token(&mut self, kind: TokenKind) -> bool {
        if std::mem::discriminant(self.peek_kind()) == std::mem::discriminant(&kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn parse_ident(&mut self, what: &str) -> Result<String, CompileError> {
        match self.peek_kind() {
            TokenKind::Ident(id) => {
                let s = id.clone();
                self.advance();
                Ok(s)
            }
            _ => Err(CompileError::new(
                "PAR_004",
                format!("expected {}", what),
                Some(self.peek().span),
            )),
        }
    }

    fn consume_stmt_terminator(&mut self) {
        if matches!(self.peek_kind(), TokenKind::Semicolon) {
            self.advance();
        }
        self.skip_newlines();
    }

    pub fn parse_program(&mut self) -> Result<Program, CompileError> {
        let mut items = Vec::new();
        self.skip_newlines();

        while !self.is_eof() {
            items.push(self.parse_item()?);
            self.skip_newlines();
        }

        Ok(Program { items })
    }

    fn parse_item(&mut self) -> Result<Item, CompileError> {
        self.skip_newlines();
        match self.peek_kind() {
            TokenKind::Import => self.parse_import().map(Item::Import),
            TokenKind::Fn => self.parse_fn(false).map(Item::Fn),
            TokenKind::Extern => {
                self.advance();
                self.parse_fn(true).map(Item::Fn)
            }
            TokenKind::Const => self.parse_const().map(Item::Const),
            TokenKind::Struct => self.parse_struct().map(Item::Struct),
            _ => Err(CompileError::new(
                "PAR_002",
                format!(
                    "expected an item (fn, struct, const, or import), found {}",
                    self.peek_kind().describe()
                ),
                Some(self.peek().span),
            )),
        }
    }

    fn parse_import(&mut self) -> Result<Import, CompileError> {
        let start = self.expect(TokenKind::Import)?.span;
        let path = match self.peek_kind() {
            TokenKind::StringLit(s) => {
                let s = s.clone();
                self.advance();
                s
            }
            _ => {
                return Err(CompileError::new(
                    "PAR_001",
                    format!(
                        "expected an import path string, found {}",
                        self.peek_kind().describe()
                    ),
                    Some(self.peek().span),
                ))
            }
        };

        let mut alias = None;
        if self.match_token(TokenKind::Ident("as".into())) {
            alias = Some(self.parse_ident("an alias identifier after 'as'")?);
        }

        self.consume_stmt_terminator();
        Ok(Import { path, alias, span: start })
    }

    fn parse_fn(&mut self, is_extern: bool) -> Result<FnDef, CompileError> {
        let start = self.expect(TokenKind::Fn)?.span;
        let name = self.parse_ident("a function name")?;

        self.expect(TokenKind::LParen)?;
        let mut params = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
            let p_span = self.peek().span;
            let p_name = self.parse_ident("a parameter name")?;
            self.expect(TokenKind::Colon)?;
            let ty = self.parse_type()?;
            params.push(Param { name: p_name, ty, span: p_span });

            if !self.match_token(TokenKind::Comma) {
                break;
            }
        }
        self.expect(TokenKind::RParen)?;

        let mut ret_ty = Type::Void;
        if self.match_token(TokenKind::Arrow) {
            ret_ty = self.parse_type()?;
        }

        let body = if is_extern {
            self.consume_stmt_terminator();
            None
        } else {
            self.skip_newlines();
            self.expect(TokenKind::LBrace)?;
            let stmts = self.parse_block_stmts()?;
            self.expect(TokenKind::RBrace)?;
            Some(stmts)
        };

        Ok(FnDef {
            name,
            params,
            ret_ty,
            body,
            is_extern,
            stack_size: 0,
            span: start,
        })
    }

    fn parse_struct(&mut self) -> Result<StructDef, CompileError> {
        let start = self.expect(TokenKind::Struct)?.span;
        let name = self.parse_ident("a struct name")?;
        self.skip_newlines();
        self.expect(TokenKind::LBrace)?;

        let mut fields = Vec::new();
        while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
            self.skip_newlines();
            let field_span = self.peek().span;
            let field_name = self.parse_ident("a field name")?;
            self.expect(TokenKind::Colon)?;
            let field_ty = self.parse_type()?;
            fields.push(FieldDef {
                name: field_name,
                ty: field_ty,
                span: field_span,
            });
            self.match_token(TokenKind::Comma);
            self.skip_newlines();
        }
        self.expect(TokenKind::RBrace)?;
        self.consume_stmt_terminator();

        Ok(StructDef {
            name,
            fields,
            span: start,
        })
    }

    /// `Name { ... }` is a struct literal only when the brace opens an empty
    /// body or a `field: value` pair; otherwise the brace belongs to a
    /// following block (e.g. the body of `if cond {`).
    fn looks_like_struct_literal(&self) -> bool {
        match self.tokens.get(self.pos + 1).map(|t| &t.kind) {
            Some(TokenKind::RBrace) => true,
            Some(TokenKind::Ident(_)) => matches!(
                self.tokens.get(self.pos + 2).map(|t| &t.kind),
                Some(TokenKind::Colon)
            ),
            _ => false,
        }
    }

    fn parse_type(&mut self) -> Result<Type, CompileError> {
        if self.match_token(TokenKind::Star) {
            let inner = self.parse_type()?;
            return Ok(Type::Pointer(Box::new(inner)));
        }

        if self.match_token(TokenKind::LBracket) {
            let elem = self.parse_type()?;
            self.expect(TokenKind::Semicolon)?;
            let len_tok = self.advance();
            let len = match len_tok.kind {
                TokenKind::IntLit(n) if n > 0 && n <= 65536 => n as usize,
                _ => {
                    return Err(CompileError::new(
                        "PAR_006",
                        format!("array type needs a length literal up to 65536, found {}", len_tok.kind.describe()),
                        Some(len_tok.span),
                    ))
                }
            };
            self.expect(TokenKind::RBracket)?;
            return Ok(Type::Array(Box::new(elem), len));
        }

        let tok = self.advance();
        match tok.kind {
            TokenKind::TypeI8 => Ok(Type::I8),
            TokenKind::TypeI16 => Ok(Type::I16),
            TokenKind::TypeI32 => Ok(Type::I32),
            TokenKind::TypeU8 => Ok(Type::U8),
            TokenKind::TypeU16 => Ok(Type::U16),
            TokenKind::TypeU32 => Ok(Type::U32),
            TokenKind::TypeBool => Ok(Type::Bool),
            TokenKind::TypeVoid => Ok(Type::Void),
            TokenKind::Ident(name) => Ok(Type::Struct(name)),
            _ => Err(CompileError::new(
                "PAR_003",
                format!("expected a type, found {}", tok.kind.describe()),
                Some(tok.span),
            )),
        }
    }

    fn parse_const(&mut self) -> Result<Stmt, CompileError> {
        let start = self.expect(TokenKind::Const)?.span;
        let name = self.parse_ident("a constant name")?;
        let ty = if self.match_token(TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect(TokenKind::Eq)?;
        let init = self.parse_expr()?;
        self.consume_stmt_terminator();

        Ok(Stmt::Const { name, ty, init, span: start })
    }

    fn parse_block_stmts(&mut self) -> Result<Vec<Stmt>, CompileError> {
        let mut stmts = Vec::new();
        self.skip_newlines();

        while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
            stmts.push(self.parse_stmt()?);
            self.skip_newlines();
        }

        Ok(stmts)
    }

    fn parse_stmt(&mut self) -> Result<Stmt, CompileError> {
        self.skip_newlines();
        match self.peek_kind() {
            TokenKind::Let => self.parse_let(),
            TokenKind::Return => self.parse_return(),
            TokenKind::If => self.parse_if(),
            TokenKind::While => self.parse_while(),
            TokenKind::For => self.parse_for(),
            TokenKind::LBrace => {
                let start = self.advance().span;
                let inner = self.parse_block_stmts()?;
                self.expect(TokenKind::RBrace)?;
                Ok(Stmt::Block(inner, start))
            }
            _ => {
                let expr = self.parse_expr()?;
                let span = expr.span();
                self.consume_stmt_terminator();
                Ok(Stmt::Expr(expr, span))
            }
        }
    }

    fn parse_let(&mut self) -> Result<Stmt, CompileError> {
        let start = self.expect(TokenKind::Let)?.span;
        let is_mut = self.match_token(TokenKind::Mut);
        let name = self.parse_ident("an identifier after 'let'")?;

        let mut ty = None;
        if self.match_token(TokenKind::Colon) {
            ty = Some(self.parse_type()?);
        }

        let mut init = None;
        if self.match_token(TokenKind::Eq) {
            init = Some(self.parse_expr()?);
        }

        self.consume_stmt_terminator();
        Ok(Stmt::Let { name, is_mut, ty, init, span: start })
    }

    fn parse_return(&mut self) -> Result<Stmt, CompileError> {
        let start = self.expect(TokenKind::Return)?.span;
        let value = if matches!(self.peek_kind(), TokenKind::Semicolon | TokenKind::Newline | TokenKind::RBrace | TokenKind::Eof) {
            None
        } else {
            Some(self.parse_expr()?)
        };

        self.consume_stmt_terminator();
        Ok(Stmt::Return { value, span: start })
    }

    fn parse_if(&mut self) -> Result<Stmt, CompileError> {
        let start = self.expect(TokenKind::If)?.span;
        let has_paren = self.match_token(TokenKind::LParen);
        let cond = self.parse_expr()?;
        if has_paren {
            self.expect(TokenKind::RParen)?;
        }

        self.skip_newlines();
        self.expect(TokenKind::LBrace)?;
        let then_branch = self.parse_block_stmts()?;
        self.expect(TokenKind::RBrace)?;

        self.skip_newlines();
        let mut else_branch = None;
        if self.match_token(TokenKind::Else) {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::If) {
                let nested_if = self.parse_if()?;
                else_branch = Some(vec![nested_if]);
            } else {
                self.expect(TokenKind::LBrace)?;
                let branch = self.parse_block_stmts()?;
                self.expect(TokenKind::RBrace)?;
                else_branch = Some(branch);
            }
        }

        Ok(Stmt::If { cond, then_branch, else_branch, span: start })
    }

    fn parse_while(&mut self) -> Result<Stmt, CompileError> {
        let start = self.expect(TokenKind::While)?.span;
        let has_paren = self.match_token(TokenKind::LParen);
        let cond = self.parse_expr()?;
        if has_paren {
            self.expect(TokenKind::RParen)?;
        }

        self.skip_newlines();
        self.expect(TokenKind::LBrace)?;
        let body = self.parse_block_stmts()?;
        self.expect(TokenKind::RBrace)?;

        Ok(Stmt::While { cond, body, span: start })
    }

    fn parse_for(&mut self) -> Result<Stmt, CompileError> {
        let start = self.expect(TokenKind::For)?.span;
        self.expect(TokenKind::LParen)?;

        // init statement (e.g., "let i = 0" or "i = 0")
        let init_span = self.peek().span;
        let init = if matches!(self.peek_kind(), TokenKind::Let) {
            let let_stmt = self.parse_let()?;
            Box::new(Stmt::LetWrapper(Box::new(let_stmt)))
        } else {
            let expr = self.parse_expr()?;
            self.expect(TokenKind::Semicolon)?;
            Box::new(Stmt::ExprInit(Box::new(expr), init_span))
        };

        // condition expression
        let cond = self.parse_expr()?;

        self.expect(TokenKind::Semicolon)?;

        // step expression (may be empty)
        let step = if matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
            Box::new(Expr::IntLit(1, start)) // default increment: +1
        } else {
            Box::new(self.parse_expr()?)
        };

        self.expect(TokenKind::RParen)?;

        // body
        self.skip_newlines();
        self.expect(TokenKind::LBrace)?;
        let body = self.parse_block_stmts()?;
        self.expect(TokenKind::RBrace)?;

        Ok(Stmt::For {
            init,
            cond,
            step,
            body,
            span: start,
        })
    }

    pub fn parse_expr(&mut self) -> Result<Expr, CompileError> {
        self.parse_assignment()
    }

    fn parse_assignment(&mut self) -> Result<Expr, CompileError> {
        let expr = self.parse_logical_or()?;
        if self.match_token(TokenKind::Eq) {
            let val = self.parse_assignment()?;
            let span = expr.span();
            return Ok(Expr::Assign {
                target: Box::new(expr),
                value: Box::new(val),
                span,
            });
        }
        Ok(expr)
    }

    fn parse_logical_or(&mut self) -> Result<Expr, CompileError> {
        let mut left = self.parse_logical_and()?;
        while self.match_token(TokenKind::PipePipe) {
            let span = left.span();
            let right = self.parse_logical_and()?;
            left = Expr::Binary(BinaryOp::Or, Box::new(left), Box::new(right), span);
        }
        Ok(left)
    }

    fn parse_logical_and(&mut self) -> Result<Expr, CompileError> {
        let mut left = self.parse_bitwise_or()?;
        while self.match_token(TokenKind::AmpAmp) {
            let span = left.span();
            let right = self.parse_bitwise_or()?;
            left = Expr::Binary(BinaryOp::And, Box::new(left), Box::new(right), span);
        }
        Ok(left)
    }

    fn parse_bitwise_or(&mut self) -> Result<Expr, CompileError> {
        let mut left = self.parse_bitwise_xor()?;
        while self.match_token(TokenKind::Pipe) {
            let span = left.span();
            let right = self.parse_bitwise_xor()?;
            left = Expr::Binary(BinaryOp::BitOr, Box::new(left), Box::new(right), span);
        }
        Ok(left)
    }

    fn parse_bitwise_xor(&mut self) -> Result<Expr, CompileError> {
        let mut left = self.parse_bitwise_and()?;
        while self.match_token(TokenKind::Caret) {
            let span = left.span();
            let right = self.parse_bitwise_and()?;
            left = Expr::Binary(BinaryOp::BitXor, Box::new(left), Box::new(right), span);
        }
        Ok(left)
    }

    fn parse_bitwise_and(&mut self) -> Result<Expr, CompileError> {
        let mut left = self.parse_equality()?;
        while self.match_token(TokenKind::Amp) {
            let span = left.span();
            let right = self.parse_equality()?;
            left = Expr::Binary(BinaryOp::BitAnd, Box::new(left), Box::new(right), span);
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expr, CompileError> {
        let mut left = self.parse_relational()?;
        while matches!(self.peek_kind(), TokenKind::EqEq | TokenKind::NotEq) {
            let is_eq = matches!(self.peek_kind(), TokenKind::EqEq);
            self.advance();
            let span = left.span();
            let right = self.parse_relational()?;
            let op = if is_eq { BinaryOp::Eq } else { BinaryOp::Ne };
            left = Expr::Binary(op, Box::new(left), Box::new(right), span);
        }
        Ok(left)
    }

    fn parse_relational(&mut self) -> Result<Expr, CompileError> {
        let mut left = self.parse_shift()?;
        while matches!(self.peek_kind(), TokenKind::Lt | TokenKind::LtEq | TokenKind::Gt | TokenKind::GtEq) {
            let op = match self.peek_kind() {
                TokenKind::Lt => BinaryOp::Lt,
                TokenKind::LtEq => BinaryOp::Le,
                TokenKind::Gt => BinaryOp::Gt,
                TokenKind::GtEq => BinaryOp::Ge,
                _ => unreachable!(),
            };
            self.advance();
            let span = left.span();
            let right = self.parse_shift()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right), span);
        }
        Ok(left)
    }

    fn parse_shift(&mut self) -> Result<Expr, CompileError> {
        let mut left = self.parse_additive()?;
        while matches!(self.peek_kind(), TokenKind::Shl | TokenKind::Shr) {
            let is_shl = matches!(self.peek_kind(), TokenKind::Shl);
            self.advance();
            let span = left.span();
            let right = self.parse_additive()?;
            let op = if is_shl { BinaryOp::Shl } else { BinaryOp::Shr };
            left = Expr::Binary(op, Box::new(left), Box::new(right), span);
        }
        Ok(left)
    }

    fn parse_additive(&mut self) -> Result<Expr, CompileError> {
        let mut left = self.parse_multiplicative()?;
        while matches!(self.peek_kind(), TokenKind::Plus | TokenKind::Minus) {
            let is_plus = matches!(self.peek_kind(), TokenKind::Plus);
            self.advance();
            let span = left.span();
            let right = self.parse_multiplicative()?;
            let op = if is_plus { BinaryOp::Add } else { BinaryOp::Sub };
            left = Expr::Binary(op, Box::new(left), Box::new(right), span);
        }
        Ok(left)
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, CompileError> {
        let mut left = self.parse_unary()?;
        while matches!(self.peek_kind(), TokenKind::Star | TokenKind::Slash | TokenKind::Percent) {
            let op = match self.peek_kind() {
                TokenKind::Star => BinaryOp::Mul,
                TokenKind::Slash => BinaryOp::Div,
                TokenKind::Percent => BinaryOp::Mod,
                _ => unreachable!(),
            };
            self.advance();
            let span = left.span();
            let right = self.parse_unary()?;
            left = Expr::Binary(op, Box::new(left), Box::new(right), span);
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, CompileError> {
        let span = self.peek().span;
        match self.peek_kind() {
            TokenKind::Minus => {
                self.advance();
                let inner = self.parse_unary()?;
                Ok(Expr::Unary(UnaryOp::Neg, Box::new(inner), span))
            }
            TokenKind::Bang => {
                self.advance();
                let inner = self.parse_unary()?;
                Ok(Expr::Unary(UnaryOp::Not, Box::new(inner), span))
            }
            TokenKind::Tilde => {
                self.advance();
                let inner = self.parse_unary()?;
                Ok(Expr::Unary(UnaryOp::BitNot, Box::new(inner), span))
            }
            TokenKind::Star => {
                self.advance();
                let inner = self.parse_unary()?;
                Ok(Expr::Unary(UnaryOp::Deref, Box::new(inner), span))
            }
            TokenKind::Amp => {
                self.advance();
                let inner = self.parse_unary()?;
                Ok(Expr::Unary(UnaryOp::AddrOf, Box::new(inner), span))
            }
            _ => self.parse_postfix(),
        }
    }

    fn parse_postfix(&mut self) -> Result<Expr, CompileError> {
        let mut expr = self.parse_primary()?;

        loop {
            if self.match_token(TokenKind::LParen) {
                let mut args = Vec::new();
                while !matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                    args.push(self.parse_expr()?);
                    if !self.match_token(TokenKind::Comma) {
                        break;
                    }
                }
                let span = expr.span();
                self.expect(TokenKind::RParen)?;
                expr = Expr::Call {
                    callee: Box::new(expr),
                    args,
                    span,
                };
            } else if self.match_token(TokenKind::Dot) {
                let span = self.peek().span;
                let field = self.parse_ident("a field name after '.'")?;
                expr = Expr::Field(Box::new(expr), field, span);
            } else if self.match_token(TokenKind::LBracket) {
                let span = self.peek().span;
                let index = self.parse_expr()?;
                self.expect(TokenKind::RBracket)?;
                expr = Expr::Index(Box::new(expr), Box::new(index), span);
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expr, CompileError> {
        let tok = self.advance();
        match tok.kind {
            TokenKind::IntLit(n) => Ok(Expr::IntLit(n, tok.span)),
            TokenKind::StringLit(s) => Ok(Expr::StringLit(s, tok.span)),
            TokenKind::True => Ok(Expr::BoolLit(true, tok.span)),
            TokenKind::False => Ok(Expr::BoolLit(false, tok.span)),
            TokenKind::Syscall => {
                self.expect(TokenKind::LParen)?;
                let number = self.parse_expr()?;
                let mut args = Vec::new();
                while self.match_token(TokenKind::Comma) {
                    args.push(self.parse_expr()?);
                }
                self.expect(TokenKind::RParen)?;
                Ok(Expr::Syscall {
                    number: Box::new(number),
                    args,
                    span: tok.span,
                })
            }
            TokenKind::Ident(id) => {
                if self.match_token(TokenKind::PathSep) {
                    let mut path = vec![id];
                    path.push(self.parse_ident("an identifier after '::'")?);
                    Ok(Expr::Path(path, tok.span))
                } else if matches!(self.peek_kind(), TokenKind::LBrace)
                    && self.looks_like_struct_literal()
                {
                    self.advance();
                    let mut inits = Vec::new();
                    while !matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                        let field = self.parse_ident("a field name")?;
                        self.expect(TokenKind::Colon)?;
                        let value = self.parse_expr()?;
                        inits.push((field, value));
                        if !self.match_token(TokenKind::Comma) {
                            break;
                        }
                    }
                    self.expect(TokenKind::RBrace)?;
                    Ok(Expr::StructLit(id, inits, tok.span))
                } else {
                    Ok(Expr::Ident(id, tok.span))
                }
            }
            TokenKind::LBracket => {
                let mut elems = Vec::new();
                while !matches!(self.peek_kind(), TokenKind::RBracket | TokenKind::Eof) {
                    elems.push(self.parse_expr()?);
                    if !self.match_token(TokenKind::Comma) {
                        break;
                    }
                }
                self.expect(TokenKind::RBracket)?;
                Ok(Expr::ArrayLit(elems, tok.span))
            }
            TokenKind::LParen => {
                let inner = self.parse_expr()?;
                self.expect(TokenKind::RParen)?;
                Ok(inner)
            }
            _ => Err(CompileError::new(
                "PAR_005",
                format!("expected an expression, found {}", tok.kind.describe()),
                Some(tok.span),
            )),
        }
    }
}
