//! Expression language for schema field sizes, repeat counts, conditionals
//! and offsets.
//!
//! Grammar (in rough precedence order, lowest first):
//!
//! ```text
//! expr    = ternary
//! ternary = or ( "?" ternary ":" ternary )?
//! or      = and ( "||" and )*
//! and     = cmp  ( "&&" cmp  )*
//! cmp     = bitor ( ( "==" | "!=" | "<" | ">" | "<=" | ">=" ) bitor )?
//! bitor   = bitxor ( "|" bitxor )*
//! bitxor  = bitand ( "^" bitand )*
//! bitand  = shift  ( "&" shift  )*
//! shift   = add    ( ( "<<" | ">>" ) add )*
//! add     = mul    ( ( "+" | "-" ) mul )*
//! mul     = unary  ( ( "*" | "/" | "%" ) unary )*
//! unary   = ( "!" | "-" | "~" ) unary | postfix
//! postfix = primary ( "." IDENT ( "(" args ")" )? | "[" expr "]" )*
//! primary = INT | FLOAT | STRING | "true" | "false" | IDENT
//!         | "[" args "]" | "(" expr ")"
//! ```
//!
//! Values are typed: integers (`i128`, so every `u64` and `i64` is exact),
//! floats, booleans, strings, byte sequences and arrays. Integer arithmetic
//! that overflows is an error, `/` and `%` round towards negative infinity as
//! in Kaitai Struct, and the logical operators and `?:` accept an integer as a
//! condition (non-zero is true).

use std::cmp::Ordering;
use std::fmt;

use crate::error::{DoeError, Result};

// ─────────────────────────────────────────────────────────────────────────────
// AST
// ─────────────────────────────────────────────────────────────────────────────

/// A parsed expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Integer literal.
    Int(i128),
    /// Floating-point literal.
    Float(f64),
    /// Boolean literal.
    Bool(bool),
    /// String literal.
    Str(String),
    /// Array literal: `[1, 2, 3]`.
    Array(Vec<Expr>),
    /// Field reference, possibly qualified: `["_parent", "length"]`. Trailing
    /// segments that do not name a bound field are read as members of the
    /// value before them, so `name.length` is the length of the field `name`.
    Field(Vec<String>),
    /// A member without arguments of a value: `(a + b).to_s`.
    Member(Box<Expr>, String),
    /// A method call: `name.substring(1, 3)`.
    Call(Box<Expr>, String, Vec<Expr>),
    /// Indexing: `items[0]`.
    Index(Box<Expr>, Box<Expr>),
    /// Conditional: `cond ? a : b`.
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    /// Unary operator.
    Unary(UnaryOp, Box<Expr>),
    /// Binary operator.
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
}

/// A unary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// Arithmetic negation (`-x`).
    Neg,
    /// Logical not (`!x`).
    Not,
    /// Bitwise complement (`~x`).
    BitNot,
}

/// A binary operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    /// Addition (`+`).
    Add,
    /// Subtraction (`-`).
    Sub,
    /// Multiplication (`*`).
    Mul,
    /// Integer division (`/`).
    Div,
    /// Remainder (`%`).
    Rem,
    /// Left shift (`<<`).
    Shl,
    /// Right shift (`>>`).
    Shr,
    /// Bitwise and (`&`).
    BitAnd,
    /// Bitwise or (`|`).
    BitOr,
    /// Bitwise exclusive or (`^`).
    BitXor,
    /// Equality (`==`).
    Eq,
    /// Inequality (`!=`).
    Ne,
    /// Less than (`<`).
    Lt,
    /// Greater than (`>`).
    Gt,
    /// Less than or equal (`<=`).
    Le,
    /// Greater than or equal (`>=`).
    Ge,
    /// Short-circuit logical and (`&&`).
    And,
    /// Short-circuit logical or (`||`).
    Or,
}

impl fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Rem => "%",
            BinaryOp::Shl => "<<",
            BinaryOp::Shr => ">>",
            BinaryOp::BitAnd => "&",
            BinaryOp::BitOr => "|",
            BinaryOp::BitXor => "^",
            BinaryOp::Eq => "==",
            BinaryOp::Ne => "!=",
            BinaryOp::Lt => "<",
            BinaryOp::Gt => ">",
            BinaryOp::Le => "<=",
            BinaryOp::Ge => ">=",
            BinaryOp::And => "&&",
            BinaryOp::Or => "||",
        };
        write!(f, "{s}")
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Lexer
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Int(i128),
    Float(f64),
    Str(String),
    /// A literal that cannot be read; the message says why.
    BadLiteral(String),
    Ident(String),
    /// A character that is not part of the expression language.
    Unknown(char),
    // Punctuation
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Shl,
    Shr,
    Amp,
    Pipe,
    Caret,
    AmpAmp,
    PipePipe,
    Bang,
    EqEq,
    BangEq,
    Lt,
    Gt,
    Le,
    Ge,
    Dot,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Question,
    Colon,
    Tilde,
    Eof,
}

struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(s: &'a str) -> Self {
        Lexer {
            src: s.as_bytes(),
            pos: 0,
        }
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn peek2(&self) -> Option<u8> {
        self.src.get(self.pos + 1).copied()
    }

    fn advance(&mut self) -> Option<u8> {
        let b = self.src.get(self.pos).copied();
        if b.is_some() {
            self.pos += 1;
        }
        b
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.advance();
        }
    }

    fn read_number(&mut self) -> Token {
        let start = self.pos;
        // Hex literal
        if self.peek() == Some(b'0') && matches!(self.peek2(), Some(b'x' | b'X')) {
            self.advance();
            self.advance(); // consume "0x"
            while matches!(self.peek(), Some(b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F')) {
                self.advance();
            }
            let hex = self.slice_str(start.saturating_add(2), self.pos);
            return i128::from_str_radix(hex, 16).map_or_else(|_| self.bad_int(start), Token::Int);
        }
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.advance();
        }
        // A fraction needs a digit after the point, so `items.size` and
        // `1.to_s` are not floats.
        if self.peek() == Some(b'.') && matches!(self.peek2(), Some(b'0'..=b'9')) {
            self.advance();
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.advance();
            }
            return self
                .slice_str(start, self.pos)
                .parse()
                .map_or_else(|_| self.bad_int(start), Token::Float);
        }
        self.slice_str(start, self.pos)
            .parse()
            .map_or_else(|_| self.bad_int(start), Token::Int)
    }

    fn bad_int(&self, start: usize) -> Token {
        let text = self.slice_str(start, self.pos);
        Token::BadLiteral(format!(
            "number {text} is not valid or does not fit in 128 bits"
        ))
    }

    /// Read a string literal. `'...'` has no escapes; `"..."` understands
    /// `\\ \" \' \n \r \t \0 \xNN \uNNNN`.
    fn read_string(&mut self, quote: u8) -> Token {
        self.advance(); // the opening quote
        let mut out = String::new();
        loop {
            let Some(b) = self.advance() else {
                return Token::BadLiteral("string is not closed".to_owned());
            };
            if b == quote {
                return Token::Str(out);
            }
            if b != b'\\' || quote == b'\'' {
                // Collect the whole UTF-8 sequence this byte starts.
                let from = self.pos.saturating_sub(1);
                while matches!(self.peek(), Some(0x80..=0xbf)) {
                    self.advance();
                }
                out.push_str(self.slice_str(from, self.pos));
                continue;
            }
            let Some(escape) = self.advance() else {
                return Token::BadLiteral("string is not closed".to_owned());
            };
            let c = match escape {
                b'n' => '\n',
                b'r' => '\r',
                b't' => '\t',
                b'0' => '\0',
                b'\\' => '\\',
                b'"' => '"',
                b'\'' => '\'',
                b'x' => match self.read_hex(2) {
                    Some(c) => c,
                    None => return Token::BadLiteral("\\x needs two hex digits".to_owned()),
                },
                b'u' => match self.read_hex(4) {
                    Some(c) => c,
                    None => {
                        return Token::BadLiteral(
                            "\\u needs four hex digits of a character".to_owned(),
                        );
                    }
                },
                other => {
                    return Token::BadLiteral(format!(
                        "unknown escape \\{} in string",
                        char::from(other)
                    ));
                }
            };
            out.push(c);
        }
    }

    fn read_hex(&mut self, digits: usize) -> Option<char> {
        let start = self.pos;
        for _ in 0..digits {
            if !matches!(self.peek(), Some(b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F')) {
                return None;
            }
            self.advance();
        }
        let n = u32::from_str_radix(self.slice_str(start, self.pos), 16).ok()?;
        char::from_u32(n)
    }

    fn read_ident(&mut self) -> String {
        let start = self.pos;
        while matches!(
            self.peek(),
            Some(b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_')
        ) {
            self.advance();
        }
        self.slice_str(start, self.pos).to_owned()
    }

    /// The source text between two byte offsets (empty if out of range).
    fn slice_str(&self, from: usize, to: usize) -> &str {
        self.src
            .get(from..to)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .unwrap_or_default()
    }

    fn next_token(&mut self) -> Token {
        self.skip_ws();
        match self.peek() {
            None => Token::Eof,
            Some(b'0'..=b'9') => self.read_number(),
            Some(q @ (b'"' | b'\'')) => self.read_string(q),
            Some(b'a'..=b'z' | b'A'..=b'Z' | b'_') => Token::Ident(self.read_ident()),
            Some(b'+') => {
                self.advance();
                Token::Plus
            }
            Some(b'-') => {
                self.advance();
                Token::Minus
            }
            Some(b'*') => {
                self.advance();
                Token::Star
            }
            Some(b'/') => {
                self.advance();
                Token::Slash
            }
            Some(b'%') => {
                self.advance();
                Token::Percent
            }
            Some(b'^') => {
                self.advance();
                Token::Caret
            }
            Some(b'.') => {
                self.advance();
                Token::Dot
            }
            Some(b'(') => {
                self.advance();
                Token::LParen
            }
            Some(b')') => {
                self.advance();
                Token::RParen
            }
            Some(b'[') => {
                self.advance();
                Token::LBracket
            }
            Some(b']') => {
                self.advance();
                Token::RBracket
            }
            Some(b',') => {
                self.advance();
                Token::Comma
            }
            Some(b'?') => {
                self.advance();
                Token::Question
            }
            Some(b':') => {
                self.advance();
                Token::Colon
            }
            Some(b'~') => {
                self.advance();
                Token::Tilde
            }
            Some(b'<') => {
                self.advance();
                match self.peek() {
                    Some(b'<') => {
                        self.advance();
                        Token::Shl
                    }
                    Some(b'=') => {
                        self.advance();
                        Token::Le
                    }
                    _ => Token::Lt,
                }
            }
            Some(b'>') => {
                self.advance();
                match self.peek() {
                    Some(b'>') => {
                        self.advance();
                        Token::Shr
                    }
                    Some(b'=') => {
                        self.advance();
                        Token::Ge
                    }
                    _ => Token::Gt,
                }
            }
            Some(b'=') => {
                self.advance();
                if self.peek() == Some(b'=') {
                    self.advance();
                    Token::EqEq
                } else {
                    Token::Unknown('=') // lone `=` is not valid
                }
            }
            Some(b'!') => {
                self.advance();
                if self.peek() == Some(b'=') {
                    self.advance();
                    Token::BangEq
                } else {
                    Token::Bang
                }
            }
            Some(b'&') => {
                self.advance();
                if self.peek() == Some(b'&') {
                    self.advance();
                    Token::AmpAmp
                } else {
                    Token::Amp
                }
            }
            Some(b'|') => {
                self.advance();
                if self.peek() == Some(b'|') {
                    self.advance();
                    Token::PipePipe
                } else {
                    Token::Pipe
                }
            }
            Some(c) => {
                self.advance();
                // Surface unrecognised characters as a token the parser
                // rejects, rather than silently ending the expression.
                Token::Unknown(char::from(c))
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Parser
// ─────────────────────────────────────────────────────────────────────────────

struct Parser<'a> {
    lexer: Lexer<'a>,
    current: Token,
    src: &'a str,
    depth: usize,
}

/// The longest expression, in bytes, that [`parse`] accepts.
///
/// Evaluation and dropping an expression recurse once per tree level, and a
/// chain such as `1+1+1+…` is as deep as it is long, so an unbounded input
/// could overflow the stack.
pub const MAX_EXPR_LEN: usize = 1024;

/// The deepest nesting of parentheses and unary operators that [`parse`]
/// accepts. The parser recurses once per level.
pub const MAX_EXPR_DEPTH: usize = 64;

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        let mut lexer = Lexer::new(src);
        let current = lexer.next_token();
        Parser {
            lexer,
            current,
            src,
            depth: 0,
        }
    }

    /// Enter one level of nesting, failing past [`MAX_EXPR_DEPTH`].
    fn descend(&mut self) -> Result<()> {
        self.depth += 1;
        if self.depth > MAX_EXPR_DEPTH {
            return Err(self.err(&format!(
                "expression is nested more than {MAX_EXPR_DEPTH} levels deep"
            )));
        }
        Ok(())
    }

    fn bump(&mut self) -> Token {
        let prev = self.current.clone();
        self.current = self.lexer.next_token();
        prev
    }

    fn err(&self, msg: &str) -> DoeError {
        DoeError::ExprParse {
            expr: self.src.to_owned(),
            message: msg.to_owned(),
        }
    }

    // ── Grammar rules ─────────────────────────────────────────────────────

    fn parse_expr(&mut self) -> Result<Expr> {
        let cond = self.parse_or()?;
        if self.current != Token::Question {
            return Ok(cond);
        }
        self.bump();
        self.descend()?;
        let then = self.parse_expr()?;
        if self.current != Token::Colon {
            return Err(self.err("expected ':' in a conditional expression"));
        }
        self.bump();
        let otherwise = self.parse_expr()?;
        self.depth -= 1;
        Ok(Expr::Ternary(
            Box::new(cond),
            Box::new(then),
            Box::new(otherwise),
        ))
    }

    fn parse_or(&mut self) -> Result<Expr> {
        let mut lhs = self.parse_and()?;
        while self.current == Token::PipePipe {
            self.bump();
            let rhs = self.parse_and()?;
            lhs = Expr::Binary(BinaryOp::Or, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> Result<Expr> {
        let mut lhs = self.parse_cmp()?;
        while self.current == Token::AmpAmp {
            self.bump();
            let rhs = self.parse_cmp()?;
            lhs = Expr::Binary(BinaryOp::And, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_cmp(&mut self) -> Result<Expr> {
        let lhs = self.parse_bitor()?;
        let op = match self.current {
            Token::EqEq => BinaryOp::Eq,
            Token::BangEq => BinaryOp::Ne,
            Token::Lt => BinaryOp::Lt,
            Token::Gt => BinaryOp::Gt,
            Token::Le => BinaryOp::Le,
            Token::Ge => BinaryOp::Ge,
            _ => return Ok(lhs),
        };
        self.bump();
        let rhs = self.parse_bitor()?;
        Ok(Expr::Binary(op, Box::new(lhs), Box::new(rhs)))
    }

    fn parse_bitor(&mut self) -> Result<Expr> {
        let mut lhs = self.parse_bitxor()?;
        while self.current == Token::Pipe {
            self.bump();
            let rhs = self.parse_bitxor()?;
            lhs = Expr::Binary(BinaryOp::BitOr, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_bitxor(&mut self) -> Result<Expr> {
        let mut lhs = self.parse_bitand()?;
        while self.current == Token::Caret {
            self.bump();
            let rhs = self.parse_bitand()?;
            lhs = Expr::Binary(BinaryOp::BitXor, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_bitand(&mut self) -> Result<Expr> {
        let mut lhs = self.parse_shift()?;
        while self.current == Token::Amp {
            self.bump();
            let rhs = self.parse_shift()?;
            lhs = Expr::Binary(BinaryOp::BitAnd, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_shift(&mut self) -> Result<Expr> {
        let mut lhs = self.parse_add()?;
        loop {
            let op = match self.current {
                Token::Shl => BinaryOp::Shl,
                Token::Shr => BinaryOp::Shr,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_add()?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_add(&mut self) -> Result<Expr> {
        let mut lhs = self.parse_mul()?;
        loop {
            let op = match self.current {
                Token::Plus => BinaryOp::Add,
                Token::Minus => BinaryOp::Sub,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_mul()?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_mul(&mut self) -> Result<Expr> {
        let mut lhs = self.parse_unary()?;
        loop {
            let op = match self.current {
                Token::Star => BinaryOp::Mul,
                Token::Slash => BinaryOp::Div,
                Token::Percent => BinaryOp::Rem,
                _ => break,
            };
            self.bump();
            let rhs = self.parse_unary()?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<Expr> {
        match self.current.clone() {
            Token::Minus => {
                self.bump();
                self.descend()?;
                let e = self.parse_unary()?;
                self.depth -= 1;
                Ok(Expr::Unary(UnaryOp::Neg, Box::new(e)))
            }
            Token::Bang => {
                self.bump();
                self.descend()?;
                let e = self.parse_unary()?;
                self.depth -= 1;
                Ok(Expr::Unary(UnaryOp::Not, Box::new(e)))
            }
            Token::Tilde => {
                self.bump();
                self.descend()?;
                let e = self.parse_unary()?;
                self.depth -= 1;
                Ok(Expr::Unary(UnaryOp::BitNot, Box::new(e)))
            }
            _ => self.parse_postfix(),
        }
    }

    /// A primary expression followed by any number of `.member`,
    /// `.method(args)` and `[index]` suffixes.
    fn parse_postfix(&mut self) -> Result<Expr> {
        let mut base = self.parse_primary()?;
        loop {
            match self.current {
                Token::Dot => {
                    self.bump();
                    let Token::Ident(name) = self.current.clone() else {
                        return Err(self.err("expected identifier after '.'"));
                    };
                    self.bump();
                    if self.current == Token::LParen {
                        let args = self.parse_args(&Token::RParen)?;
                        base = Expr::Call(Box::new(base), name, args);
                    } else if let Expr::Field(mut parts) = base {
                        parts.push(name);
                        base = Expr::Field(parts);
                    } else {
                        base = Expr::Member(Box::new(base), name);
                    }
                }
                Token::LBracket => {
                    self.bump();
                    self.descend()?;
                    let index = self.parse_expr()?;
                    self.depth -= 1;
                    if self.current != Token::RBracket {
                        return Err(self.err("expected ']'"));
                    }
                    self.bump();
                    base = Expr::Index(Box::new(base), Box::new(index));
                }
                _ => return Ok(base),
            }
        }
    }

    /// Parse a comma-separated list up to `close`; the opening token is the
    /// current one.
    fn parse_args(&mut self, close: &Token) -> Result<Vec<Expr>> {
        self.bump();
        self.descend()?;
        let mut args = Vec::new();
        if self.current != *close {
            loop {
                args.push(self.parse_expr()?);
                if self.current == Token::Comma {
                    self.bump();
                } else {
                    break;
                }
            }
        }
        self.depth -= 1;
        if self.current != *close {
            return Err(self.err(&format!("expected {close:?} or ','")));
        }
        self.bump();
        Ok(args)
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        match self.current.clone() {
            Token::Int(n) => {
                self.bump();
                Ok(Expr::Int(n))
            }
            Token::Float(f) => {
                self.bump();
                Ok(Expr::Float(f))
            }
            Token::Str(text) => {
                self.bump();
                Ok(Expr::Str(text))
            }
            Token::Ident(name) => {
                self.bump();
                Ok(match name.as_str() {
                    "true" => Expr::Bool(true),
                    "false" => Expr::Bool(false),
                    _ => Expr::Field(vec![name]),
                })
            }
            Token::LBracket => Ok(Expr::Array(self.parse_args(&Token::RBracket)?)),
            Token::LParen => {
                self.bump();
                self.descend()?;
                let e = self.parse_expr()?;
                self.depth -= 1;
                if self.current != Token::RParen {
                    return Err(self.err("expected ')'"));
                }
                self.bump();
                Ok(e)
            }
            Token::BadLiteral(message) => Err(self.err(&message)),
            _ => Err(self.err(&format!("unexpected token {:?}", self.current))),
        }
    }
}

/// Parse an expression string into an AST.
///
/// # Errors
///
/// Returns [`DoeError::ExprParse`] on a syntax error, if `src` is longer than
/// [`MAX_EXPR_LEN`] bytes, or if it nests more than [`MAX_EXPR_DEPTH`] levels
/// of parentheses or unary operators.
pub fn parse(src: &str) -> Result<Expr> {
    if src.len() > MAX_EXPR_LEN {
        return Err(DoeError::ExprParse {
            expr: format!("{}…", src.chars().take(40).collect::<String>()),
            message: format!("expression is longer than {MAX_EXPR_LEN} bytes"),
        });
    }
    let mut p = Parser::new(src);
    let e = p.parse_expr()?;
    if p.current != Token::Eof {
        return Err(DoeError::ExprParse {
            expr: src.to_owned(),
            message: format!("unexpected token {:?} after expression", p.current),
        });
    }
    Ok(e)
}

// ─────────────────────────────────────────────────────────────────────────────
// Values
// ─────────────────────────────────────────────────────────────────────────────

/// The longest string or array an expression may build.
///
/// `s + s` can double a string, and an expression is only [`MAX_EXPR_LEN`]
/// bytes long, so without a limit a short expression could ask for
/// exponentially much memory.
pub const MAX_VALUE_LEN: usize = 1 << 20;

/// The result of evaluating an expression.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprValue {
    /// An integer. `i128` holds every `u64` and `i64` exactly.
    Int(i128),
    /// A floating-point number.
    Float(f64),
    /// A boolean.
    Bool(bool),
    /// A string.
    Str(String),
    /// A byte sequence.
    Bytes(Vec<u8>),
    /// An array of values.
    Array(Vec<ExprValue>),
}

impl ExprValue {
    /// A short name for the type, used in error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            ExprValue::Int(_) => "integer",
            ExprValue::Float(_) => "float",
            ExprValue::Bool(_) => "boolean",
            ExprValue::Str(_) => "string",
            ExprValue::Bytes(_) => "bytes",
            ExprValue::Array(_) => "array",
        }
    }

    /// The integer, if this is one.
    pub fn as_int(&self) -> Option<i128> {
        match self {
            ExprValue::Int(n) => Some(*n),
            _ => None,
        }
    }
}

impl fmt::Display for ExprValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExprValue::Int(n) => write!(f, "{n}"),
            ExprValue::Float(x) => write!(f, "{x}"),
            ExprValue::Bool(b) => write!(f, "{b}"),
            ExprValue::Str(s) => write!(f, "{s:?}"),
            ExprValue::Bytes(b) => write!(f, "<{} bytes>", b.len()),
            ExprValue::Array(a) => write!(f, "[{} items]", a.len()),
        }
    }
}

macro_rules! expr_value_from {
    ($($ty:ty => $variant:ident via $conv:expr;)*) => {$(
        impl From<$ty> for ExprValue {
            fn from(v: $ty) -> Self {
                ExprValue::$variant($conv(v))
            }
        }
    )*};
}

expr_value_from! {
    i8 => Int via i128::from;
    i16 => Int via i128::from;
    i32 => Int via i128::from;
    i64 => Int via i128::from;
    i128 => Int via std::convert::identity;
    u8 => Int via i128::from;
    u16 => Int via i128::from;
    u32 => Int via i128::from;
    u64 => Int via i128::from;
    f64 => Float via std::convert::identity;
    bool => Bool via std::convert::identity;
    String => Str via std::convert::identity;
    Vec<u8> => Bytes via std::convert::identity;
    Vec<ExprValue> => Array via std::convert::identity;
}

impl From<&str> for ExprValue {
    fn from(v: &str) -> Self {
        ExprValue::Str(v.to_owned())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Evaluation context
// ─────────────────────────────────────────────────────────────────────────────

/// A stack-based evaluation context.
///
/// Each "frame" is a flat mapping of field name → value.
/// `_parent` walks one frame up the stack.
#[derive(Debug)]
pub struct EvalContext {
    /// Frames stored outer-to-inner; last element is current scope.
    frames: Vec<std::collections::HashMap<String, ExprValue>>,
}

impl EvalContext {
    /// Create a context with one empty (root) frame.
    pub fn new() -> Self {
        EvalContext {
            frames: vec![std::collections::HashMap::new()],
        }
    }

    /// Enter a new, empty scope.
    pub fn push_frame(&mut self) {
        self.frames.push(std::collections::HashMap::new());
    }

    /// Leave the innermost scope. The root scope is never popped.
    pub fn pop_frame(&mut self) {
        if self.frames.len() > 1 {
            self.frames.pop();
        }
    }

    /// Bind `name` → `value` in the current (innermost) frame.
    pub fn bind(&mut self, name: &str, value: impl Into<ExprValue>) {
        if self.frames.is_empty() {
            self.frames.push(std::collections::HashMap::new());
        }
        if let Some(frame) = self.frames.last_mut() {
            frame.insert(name.to_owned(), value.into());
        }
    }

    /// Remove `name` and every `name.field` binding from the current frame.
    pub fn unbind_prefix(&mut self, name: &str) {
        let dotted = format!("{name}.");
        if let Some(frame) = self.frames.last_mut() {
            frame.retain(|k, _| k != name && !k.starts_with(&dotted));
        }
    }

    /// Resolve a qualified path like `["_parent", "count"]`, or one that ends
    /// in members of a bound value like `["name", "length"]`.
    fn resolve_path(&self, parts: &[String], src: &str) -> Result<ExprValue> {
        // Start at the innermost frame.
        let mut frame_idx = self
            .frames
            .len()
            .checked_sub(1)
            .ok_or_else(|| fail(src, "no scope available"))?;
        let mut rest = parts;

        // Walk _parent segments
        while let Some((first, tail)) = rest.split_first()
            && first == "_parent"
        {
            frame_idx = frame_idx
                .checked_sub(1)
                .ok_or_else(|| fail(src, "_parent: already at root scope"))?;
            rest = tail;
        }

        if rest.is_empty() {
            return Err(fail(src, "field path ends at _parent with no field name"));
        }
        let frame = self
            .frames
            .get(frame_idx)
            .ok_or_else(|| fail(src, "no scope available"))?;
        // A path into a parsed sub-type (`header.section_count`) is bound under
        // its dotted name; anything after the longest bound prefix is a member
        // of that value.
        for split in (1..=rest.len()).rev() {
            let (Some(head), Some(members)) = (rest.get(..split), rest.get(split..)) else {
                continue;
            };
            if let Some(value) = frame.get(head.join(".").as_str()) {
                let mut value = value.clone();
                for member in members {
                    value = call_method(value, member, &[], src)?;
                }
                return Ok(value);
            }
        }
        Err(fail(src, &format!("unknown field '{}'", parts.join("."))))
    }
}

impl Default for EvalContext {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Evaluator
// ─────────────────────────────────────────────────────────────────────────────

fn fail(src: &str, message: &str) -> DoeError {
    DoeError::ExprEval {
        expr: src.to_owned(),
        message: message.to_owned(),
    }
}

/// Evaluate an already-parsed expression against a context.
///
/// # Errors
///
/// Returns [`DoeError::ExprEval`] for unknown fields, a bad `_parent` path,
/// operands of the wrong type, integer overflow, division or remainder by
/// zero, an index out of range, or a string or array that would be longer
/// than [`MAX_VALUE_LEN`].
pub fn eval(expr: &Expr, ctx: &EvalContext, src: &str) -> Result<ExprValue> {
    // An explicit stack instead of recursion: a long chain such as `1+1+1+…`
    // is as deep as it is long, and recursing once per level would use an
    // amount of stack that depends on the build and the thread.
    let mut work = vec![Work::Eval(expr)];
    let mut values: Vec<ExprValue> = Vec::new();
    while let Some(step) = work.pop() {
        match step {
            Work::Eval(e) => schedule(e, ctx, src, &mut work, &mut values)?,
            Work::Unary(op) => {
                let v = pop(&mut values, src)?;
                values.push(unary(op, v, src)?);
            }
            Work::Binary(op) => {
                let (r, l) = (pop(&mut values, src)?, pop(&mut values, src)?);
                values.push(binary(op, l, r, src)?);
            }
            Work::Logic { and, rhs } => {
                let l = truthy(&pop(&mut values, src)?, src)?;
                if l == and {
                    // The left side did not decide the result.
                    work.push(Work::ToBool);
                    work.push(Work::Eval(rhs));
                } else {
                    values.push(ExprValue::Bool(l));
                }
            }
            Work::ToBool => {
                let v = truthy(&pop(&mut values, src)?, src)?;
                values.push(ExprValue::Bool(v));
            }
            Work::Branch { then, otherwise } => {
                let chosen = if truthy(&pop(&mut values, src)?, src)? {
                    then
                } else {
                    otherwise
                };
                work.push(Work::Eval(chosen));
            }
            Work::Array(n) => {
                let items = values.split_off(values.len().saturating_sub(n));
                values.push(ExprValue::Array(items));
            }
            Work::Member(name) => {
                let recv = pop(&mut values, src)?;
                values.push(call_method(recv, name, &[], src)?);
            }
            Work::Call(name, n) => {
                let args = values.split_off(values.len().saturating_sub(n));
                let recv = pop(&mut values, src)?;
                values.push(call_method(recv, name, &args, src)?);
            }
            Work::Index => {
                let (index, target) = (pop(&mut values, src)?, pop(&mut values, src)?);
                values.push(index_value(target, &index, src)?);
            }
        }
    }
    pop(&mut values, src)
}

/// One step of [`eval`].
enum Work<'e> {
    /// Evaluate a subexpression and push its value.
    Eval(&'e Expr),
    /// Apply an operator to the value(s) on top of the value stack.
    Unary(UnaryOp),
    Binary(BinaryOp),
    /// `&&` (`and`) or `||`: given the left value, either decide the result
    /// or evaluate `rhs`.
    Logic {
        and: bool,
        rhs: &'e Expr,
    },
    /// Replace the value on top with the boolean it stands for.
    ToBool,
    /// `?:`: given the condition, evaluate one branch.
    Branch {
        then: &'e Expr,
        otherwise: &'e Expr,
    },
    /// Collect the top `n` values into an array.
    Array(usize),
    Member(&'e str),
    /// Call a method with the top `n` values as arguments, on the one below.
    Call(&'e str, usize),
    Index,
}

fn pop(values: &mut Vec<ExprValue>, src: &str) -> Result<ExprValue> {
    values
        .pop()
        .ok_or_else(|| fail(src, "malformed expression"))
}

/// Queue the work for `e`, or push its value if it has no subexpressions.
fn schedule<'e>(
    e: &'e Expr,
    ctx: &EvalContext,
    src: &str,
    work: &mut Vec<Work<'e>>,
    values: &mut Vec<ExprValue>,
) -> Result<()> {
    match e {
        Expr::Int(n) => values.push(ExprValue::Int(*n)),
        Expr::Float(x) => values.push(ExprValue::Float(*x)),
        Expr::Bool(b) => values.push(ExprValue::Bool(*b)),
        Expr::Str(s) => values.push(ExprValue::Str(s.clone())),
        Expr::Field(parts) => values.push(ctx.resolve_path(parts, src)?),
        Expr::Unary(op, inner) => {
            work.push(Work::Unary(*op));
            work.push(Work::Eval(inner));
        }
        Expr::Binary(op @ (BinaryOp::And | BinaryOp::Or), lhs, rhs) => {
            work.push(Work::Logic {
                and: *op == BinaryOp::And,
                rhs,
            });
            work.push(Work::Eval(lhs));
        }
        Expr::Binary(op, lhs, rhs) => {
            work.push(Work::Binary(*op));
            work.push(Work::Eval(rhs));
            work.push(Work::Eval(lhs));
        }
        Expr::Ternary(cond, then, otherwise) => {
            work.push(Work::Branch { then, otherwise });
            work.push(Work::Eval(cond));
        }
        Expr::Array(items) => {
            work.push(Work::Array(items.len()));
            work.extend(items.iter().rev().map(Work::Eval));
        }
        Expr::Member(recv, name) => {
            work.push(Work::Member(name));
            work.push(Work::Eval(recv));
        }
        Expr::Call(recv, name, args) => {
            work.push(Work::Call(name, args.len()));
            work.extend(args.iter().rev().map(Work::Eval));
            work.push(Work::Eval(recv));
        }
        Expr::Index(target, index) => {
            work.push(Work::Index);
            work.push(Work::Eval(index));
            work.push(Work::Eval(target));
        }
    }
    Ok(())
}

fn unary(op: UnaryOp, v: ExprValue, src: &str) -> Result<ExprValue> {
    match (op, v) {
        (UnaryOp::Neg, ExprValue::Int(n)) => n
            .checked_neg()
            .map(ExprValue::Int)
            .ok_or_else(|| fail(src, "integer overflow")),
        (UnaryOp::Neg, ExprValue::Float(x)) => Ok(ExprValue::Float(-x)),
        (UnaryOp::BitNot, ExprValue::Int(n)) => Ok(ExprValue::Int(!n)),
        (UnaryOp::Not, v) => Ok(ExprValue::Bool(!truthy(&v, src)?)),
        (op, v) => Err(fail(
            src,
            &format!("operator {op:?} does not apply to a {}", v.type_name()),
        )),
    }
}

/// Evaluate an expression that must yield a boolean, or an integer read as
/// one (non-zero is true).
///
/// # Errors
///
/// Returns an error from [`eval`], or if the result is neither.
pub fn eval_bool(expr: &Expr, ctx: &EvalContext, src: &str) -> Result<bool> {
    truthy(&eval(expr, ctx, src)?, src)
}

/// Evaluate an expression that must yield an integer.
///
/// # Errors
///
/// Returns an error from [`eval`], or if the result is not an integer.
pub fn eval_int(expr: &Expr, ctx: &EvalContext, src: &str) -> Result<i128> {
    match eval(expr, ctx, src)? {
        ExprValue::Int(n) => Ok(n),
        other => Err(fail(
            src,
            &format!("expected an integer, found a {}", other.type_name()),
        )),
    }
}

fn truthy(v: &ExprValue, src: &str) -> Result<bool> {
    match v {
        ExprValue::Bool(b) => Ok(*b),
        ExprValue::Int(n) => Ok(*n != 0),
        other => Err(fail(
            src,
            &format!("expected a boolean, found a {}", other.type_name()),
        )),
    }
}

fn binary(op: BinaryOp, l: ExprValue, r: ExprValue, src: &str) -> Result<ExprValue> {
    use ExprValue::{Float, Int, Str};
    match op {
        BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Gt | BinaryOp::Le | BinaryOp::Ge => {
            let ord = compare(&l, &r, src)?;
            Ok(ExprValue::Bool(match op {
                BinaryOp::Eq => ord == Some(Ordering::Equal),
                BinaryOp::Ne => ord != Some(Ordering::Equal),
                BinaryOp::Lt => ord == Some(Ordering::Less),
                BinaryOp::Gt => ord == Some(Ordering::Greater),
                BinaryOp::Le => matches!(ord, Some(Ordering::Less | Ordering::Equal)),
                _ => matches!(ord, Some(Ordering::Greater | Ordering::Equal)),
            }))
        }
        _ => match (l, r) {
            (Int(a), Int(b)) => int_op(op, a, b, src).map(Int),
            (Str(a), Str(b)) if op == BinaryOp::Add => {
                if a.len().saturating_add(b.len()) > MAX_VALUE_LEN {
                    return Err(fail(
                        src,
                        &format!("string longer than {MAX_VALUE_LEN} bytes"),
                    ));
                }
                Ok(Str(a + &b))
            }
            (Float(a), Float(b)) => float_op(op, a, b, src),
            (Int(a), Float(b)) => float_op(op, to_float(a), b, src),
            (Float(a), Int(b)) => float_op(op, a, to_float(b), src),
            (l, r) => Err(fail(
                src,
                &format!(
                    "operator {op} does not apply to a {} and a {}",
                    l.type_name(),
                    r.type_name()
                ),
            )),
        },
    }
}

#[allow(
    clippy::cast_precision_loss,
    reason = "mixing an integer with a float is a float operation; precision beyond 53 bits is lost by definition"
)]
fn to_float(n: i128) -> f64 {
    n as f64
}

fn float_op(op: BinaryOp, a: f64, b: f64, src: &str) -> Result<ExprValue> {
    Ok(ExprValue::Float(match op {
        BinaryOp::Add => a + b,
        BinaryOp::Sub => a - b,
        BinaryOp::Mul => a * b,
        BinaryOp::Div => a / b,
        _ => {
            return Err(fail(
                src,
                &format!("operator {op} needs integers, not floats"),
            ));
        }
    }))
}

fn int_op(op: BinaryOp, a: i128, b: i128, src: &str) -> Result<i128> {
    let overflow = || fail(src, "integer overflow");
    match op {
        BinaryOp::Add => a.checked_add(b).ok_or_else(overflow),
        BinaryOp::Sub => a.checked_sub(b).ok_or_else(overflow),
        BinaryOp::Mul => a.checked_mul(b).ok_or_else(overflow),
        BinaryOp::Div => {
            if b == 0 {
                return Err(fail(src, "division by zero"));
            }
            // Round towards negative infinity, as Kaitai Struct does.
            let q = a.checked_div(b).ok_or_else(overflow)?;
            let r = a.checked_rem(b).ok_or_else(overflow)?;
            if r != 0 && ((r < 0) != (b < 0)) {
                q.checked_sub(1).ok_or_else(overflow)
            } else {
                Ok(q)
            }
        }
        BinaryOp::Rem => {
            if b == 0 {
                return Err(fail(src, "modulo by zero"));
            }
            // The result takes the sign of the divisor.
            let r = a.checked_rem(b).ok_or_else(overflow)?;
            if r != 0 && ((r < 0) != (b < 0)) {
                r.checked_add(b).ok_or_else(overflow)
            } else {
                Ok(r)
            }
        }
        BinaryOp::Shl => {
            let n = shift_amount(b, src)?;
            let factor = 1i128.checked_shl(n).ok_or_else(overflow)?;
            a.checked_mul(factor).ok_or_else(overflow)
        }
        BinaryOp::Shr => Ok(a >> shift_amount(b, src)?.min(127)),
        BinaryOp::BitAnd => Ok(a & b),
        BinaryOp::BitOr => Ok(a | b),
        BinaryOp::BitXor => Ok(a ^ b),
        _ => Err(fail(src, &format!("operator {op} is not arithmetic"))),
    }
}

fn shift_amount(n: i128, src: &str) -> Result<u32> {
    u32::try_from(n)
        .ok()
        .filter(|&n| n < 127)
        .ok_or_else(|| fail(src, &format!("shift by {n} is out of range")))
}

fn compare(l: &ExprValue, r: &ExprValue, src: &str) -> Result<Option<Ordering>> {
    use ExprValue::{Bool, Bytes, Float, Int, Str};
    Ok(match (l, r) {
        (Int(a), Int(b)) => Some(a.cmp(b)),
        (Float(a), Float(b)) => a.partial_cmp(b),
        (Int(a), Float(b)) => to_float(*a).partial_cmp(b),
        (Float(a), Int(b)) => a.partial_cmp(&to_float(*b)),
        (Str(a), Str(b)) => Some(a.cmp(b)),
        (Bytes(a), Bytes(b)) => Some(a.cmp(b)),
        (Bool(a), Bool(b)) => Some(a.cmp(b)),
        (l, r) => {
            return Err(fail(
                src,
                &format!(
                    "cannot compare a {} with a {}",
                    l.type_name(),
                    r.type_name()
                ),
            ));
        }
    })
}

fn index_value(target: ExprValue, index: &ExprValue, src: &str) -> Result<ExprValue> {
    let ExprValue::Int(i) = *index else {
        return Err(fail(
            src,
            &format!("an index must be an integer, not a {}", index.type_name()),
        ));
    };
    let out_of_range =
        |len: usize| fail(src, &format!("index {i} is out of range for {len} items"));
    match target {
        ExprValue::Array(items) => usize::try_from(i)
            .ok()
            .and_then(|i| items.get(i))
            .cloned()
            .ok_or_else(|| out_of_range(items.len())),
        ExprValue::Bytes(bytes) => usize::try_from(i)
            .ok()
            .and_then(|i| bytes.get(i))
            .map(|&b| ExprValue::Int(i128::from(b)))
            .ok_or_else(|| out_of_range(bytes.len())),
        other => Err(fail(
            src,
            &format!("a {} cannot be indexed", other.type_name()),
        )),
    }
}

fn expect_args(name: &str, args: &[ExprValue], counts: &[usize], src: &str) -> Result<()> {
    if counts.contains(&args.len()) {
        Ok(())
    } else {
        Err(fail(
            src,
            &format!("{name} takes {counts:?} arguments, not {}", args.len()),
        ))
    }
}

fn int_arg(args: &[ExprValue], at: usize, name: &str, src: &str) -> Result<i128> {
    args.get(at)
        .and_then(ExprValue::as_int)
        .ok_or_else(|| fail(src, &format!("{name} needs an integer argument")))
}

fn length(n: usize) -> ExprValue {
    ExprValue::Int(i128::try_from(n).unwrap_or(i128::MAX))
}

/// Members and methods of values: `.length`, `.to_i`, `.substring(a, b)`, ...
fn call_method(recv: ExprValue, name: &str, args: &[ExprValue], src: &str) -> Result<ExprValue> {
    use ExprValue::{Array, Bool, Bytes, Float, Int, Str};
    match (&recv, name) {
        (Str(s), "length" | "size") => {
            expect_args(name, args, &[0], src)?;
            Ok(length(s.chars().count()))
        }
        (Str(s), "to_i") => {
            expect_args(name, args, &[0, 1], src)?;
            let radix = if args.is_empty() {
                10
            } else {
                u32::try_from(int_arg(args, 0, name, src)?)
                    .ok()
                    .filter(|r| (2..=36).contains(r))
                    .ok_or_else(|| fail(src, "to_i: the radix must be between 2 and 36"))?
            };
            i128::from_str_radix(s, radix)
                .map(Int)
                .map_err(|_| fail(src, &format!("to_i: {s:?} is not a base {radix} integer")))
        }
        (Str(s), "reverse") => {
            expect_args(name, args, &[0], src)?;
            Ok(Str(s.chars().rev().collect()))
        }
        (Str(s), "substring") => {
            expect_args(name, args, &[2], src)?;
            let (from, to) = (int_arg(args, 0, name, src)?, int_arg(args, 1, name, src)?);
            let chars = s.chars().count();
            let bounds = usize::try_from(from)
                .ok()
                .zip(usize::try_from(to).ok())
                .filter(|&(from, to)| from <= to && to <= chars)
                .ok_or_else(|| {
                    fail(
                        src,
                        &format!("substring({from}, {to}) is out of range for {chars} characters"),
                    )
                })?;
            Ok(Str(s
                .chars()
                .skip(bounds.0)
                .take(bounds.1 - bounds.0)
                .collect()))
        }
        (Str(_), "to_s") | (Int(_), "to_i") | (Float(_), "to_f") => {
            expect_args(name, args, &[0], src)?;
            Ok(recv)
        }
        (Int(n), "to_s") => {
            expect_args(name, args, &[0], src)?;
            Ok(Str(n.to_string()))
        }
        (Float(x), "to_s") => {
            expect_args(name, args, &[0], src)?;
            Ok(Str(x.to_string()))
        }
        (Bool(b), "to_s") => {
            expect_args(name, args, &[0], src)?;
            Ok(Str(b.to_string()))
        }
        (Bool(b), "to_i") => {
            expect_args(name, args, &[0], src)?;
            Ok(Int(i128::from(*b)))
        }
        (Float(x), "to_i") => {
            expect_args(name, args, &[0], src)?;
            float_to_int(*x)
                .map(Int)
                .ok_or_else(|| fail(src, &format!("to_i: {x} is not an integer in range")))
        }
        (Bytes(b), "length" | "size") => {
            expect_args(name, args, &[0], src)?;
            Ok(length(b.len()))
        }
        (Array(a), "length" | "size") => {
            expect_args(name, args, &[0], src)?;
            Ok(length(a.len()))
        }
        (Bytes(b), "first" | "last" | "min" | "max") => {
            expect_args(name, args, &[0], src)?;
            let items: Vec<ExprValue> = b.iter().map(|&x| Int(i128::from(x))).collect();
            pick(&items, name, src)
        }
        (Array(items), "first" | "last" | "min" | "max") => {
            expect_args(name, args, &[0], src)?;
            pick(items, name, src)
        }
        _ => Err(fail(
            src,
            &format!("a {} has no member '{name}'", recv.type_name()),
        )),
    }
}

/// The result of `.first`, `.last`, `.min` or `.max` over `items`.
fn pick(items: &[ExprValue], which: &str, src: &str) -> Result<ExprValue> {
    let empty = || fail(src, &format!("{which}: there are no items"));
    match which {
        "first" => items.first().cloned().ok_or_else(empty),
        "last" => items.last().cloned().ok_or_else(empty),
        _ => {
            let mut best = items.first().ok_or_else(empty)?;
            for item in items.iter().skip(1) {
                let ord = compare(item, best, src)?;
                let better = if which == "min" {
                    ord == Some(Ordering::Less)
                } else {
                    ord == Some(Ordering::Greater)
                };
                if better {
                    best = item;
                }
            }
            Ok(best.clone())
        }
    }
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "the range is checked first; the cast truncates the fraction on purpose"
)]
fn float_to_int(x: f64) -> Option<i128> {
    // Well inside the range of an i128, and not NaN or infinite.
    (x.is_finite() && x.abs() < 1e38).then_some(x as i128)
}

/// Convenience: parse then evaluate in one call.
///
/// # Errors
///
/// Returns any error from [`parse`] or [`eval`].
pub fn eval_str(src: &str, ctx: &EvalContext) -> Result<ExprValue> {
    let ast = parse(src)?;
    eval(&ast, ctx, src)
}
