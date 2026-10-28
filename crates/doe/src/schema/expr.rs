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

use std::borrow::Cow;
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
    /// An enum member: `["kind_t", "data"]` for `kind_t::data`. The schema
    /// compiler replaces it with the member's value; evaluating one that was
    /// not replaced is an error.
    Enum(Vec<String>),
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
    ColonColon,
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
        // Hex, binary and octal literals. `_` may separate digits.
        if self.peek() == Some(b'0') {
            let radix = match self.peek2() {
                Some(b'x' | b'X') => Some(16),
                Some(b'b' | b'B') => Some(2),
                Some(b'o' | b'O') => Some(8),
                _ => None,
            };
            if let Some(radix) = radix {
                self.advance();
                self.advance(); // consume "0x", "0b" or "0o"
                while matches!(
                    self.peek(),
                    Some(b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F' | b'_')
                ) {
                    // A binary or octal literal does not swallow a letter that
                    // is not a digit of it, so that the error names it.
                    if radix != 16 && matches!(self.peek(), Some(b'a'..=b'f' | b'A'..=b'F')) {
                        break;
                    }
                    self.advance();
                }
                let digits: String = self
                    .slice_str(start.saturating_add(2), self.pos)
                    .chars()
                    .filter(|c| *c != '_')
                    .collect();
                return i128::from_str_radix(&digits, radix)
                    .map_or_else(|_| self.bad_int(start), Token::Int);
            }
        }
        while matches!(self.peek(), Some(b'0'..=b'9' | b'_')) {
            self.advance();
        }
        // A fraction needs a digit after the point, so `items.size` and
        // `1.to_s` are not floats; a point followed by something that cannot
        // start a member (`24000.`) ends a float.
        let after_point = self.peek2();
        if self.peek() == Some(b'.')
            && (matches!(after_point, Some(b'0'..=b'9'))
                || !matches!(after_point, Some(b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'.')))
        {
            self.advance();
            while matches!(self.peek(), Some(b'0'..=b'9' | b'_')) {
                self.advance();
            }
            let text: String = self
                .slice_str(start, self.pos)
                .chars()
                .filter(|c| *c != '_')
                .collect();
            return text
                .parse()
                .map_or_else(|_| self.bad_int(start), Token::Float);
        }
        let text: String = self
            .slice_str(start, self.pos)
            .chars()
            .filter(|c| *c != '_')
            .collect();
        text.parse()
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
            Some(b'a'..=b'z' | b'A'..=b'Z' | b'_') => {
                let id = self.read_ident();
                match id.as_str() {
                    // Kaitai Struct spells the logical operators as words.
                    "and" => Token::AmpAmp,
                    "or" => Token::PipePipe,
                    "not" => Token::Bang,
                    _ => Token::Ident(id),
                }
            }
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
                if self.peek() == Some(b':') {
                    self.advance();
                    Token::ColonColon
                } else {
                    Token::Colon
                }
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

/// The precedence of the comparisons, which do not chain.
const COMPARISON: u8 = 3;

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
pub const MAX_EXPR_LEN: usize = 64 * 1024;

/// The most operators and suffixes (`a + b + c`, `x.f(1)[2]`) that [`parse`]
/// accepts in an expression, which bounds how deep a chain of them can make
/// the tree. A long array literal is not limited by it.
pub const MAX_EXPR_LINKS: usize = 512;

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
        let cond = self.parse_binary(1)?;
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

    /// The binary operators by precedence, from `||` (the loosest) to `*`.
    /// Each is left-associative, except the comparisons, which do not chain.
    fn binary_op(token: &Token) -> Option<(u8, BinaryOp)> {
        Some(match token {
            Token::PipePipe => (1, BinaryOp::Or),
            Token::AmpAmp => (2, BinaryOp::And),
            Token::EqEq => (3, BinaryOp::Eq),
            Token::BangEq => (3, BinaryOp::Ne),
            Token::Lt => (3, BinaryOp::Lt),
            Token::Gt => (3, BinaryOp::Gt),
            Token::Le => (3, BinaryOp::Le),
            Token::Ge => (3, BinaryOp::Ge),
            Token::Pipe => (4, BinaryOp::BitOr),
            Token::Caret => (5, BinaryOp::BitXor),
            Token::Amp => (6, BinaryOp::BitAnd),
            Token::Shl => (7, BinaryOp::Shl),
            Token::Shr => (7, BinaryOp::Shr),
            Token::Plus => (8, BinaryOp::Add),
            Token::Minus => (8, BinaryOp::Sub),
            Token::Star => (9, BinaryOp::Mul),
            Token::Slash => (9, BinaryOp::Div),
            Token::Percent => (9, BinaryOp::Rem),
            _ => return None,
        })
    }

    /// Operators of at least precedence `min`. One function for all the
    /// levels, so that a level of nesting costs few stack frames.
    fn parse_binary(&mut self, min: u8) -> Result<Expr> {
        let mut lhs = self.parse_unary()?;
        let mut compared = false;
        while let Some((prec, op)) = Self::binary_op(&self.current) {
            if prec < min || (prec == COMPARISON && compared) {
                break;
            }
            compared = prec == COMPARISON;
            self.bump();
            let rhs = self.parse_binary(prec + 1)?;
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
            Token::Plus => {
                self.bump();
                self.descend()?;
                let e = self.parse_unary()?;
                self.depth -= 1;
                Ok(e)
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
                    if name == "as" && self.current == Token::Lt {
                        // `x.as<type>` names the type of a value that is one
                        // of several; the value is the same. Except for
                        // `bytes`: a list of numbers is read as bytes.
                        if self.parse_type_arg()? == "bytes" {
                            base = Expr::Call(Box::new(base), "as<bytes>".to_owned(), Vec::new());
                        }
                    } else if self.current == Token::LParen {
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

    /// Parse `<name>` or `<a::b>`, the type of `as<>` and `sizeof<>`; the
    /// current token is the `<`.
    fn parse_type_arg(&mut self) -> Result<String> {
        self.bump();
        let mut name = String::new();
        loop {
            let Token::Ident(part) = self.current.clone() else {
                return Err(self.err("expected a type name after '<'"));
            };
            self.bump();
            name.push_str(&part);
            if self.current == Token::ColonColon {
                self.bump();
                name.push_str("::");
            } else {
                break;
            }
        }
        if self.current != Token::Gt {
            return Err(self.err("expected '>' after the type name"));
        }
        self.bump();
        Ok(name)
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
                    // A list may end in a comma.
                    if self.current == *close {
                        break;
                    }
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

    /// A name: an enum member (`a::b`), `sizeof<type>`, a boolean or a field.
    /// Kept out of [`Parser::parse_primary`], which a level of nesting passes
    /// through, to keep its stack frame small.
    #[inline(never)]
    fn parse_name(&mut self, name: String) -> Result<Expr> {
        self.bump();
        if self.current == Token::ColonColon {
            let mut path = vec![name];
            while self.current == Token::ColonColon {
                self.bump();
                let Token::Ident(part) = self.current.clone() else {
                    return Err(self.err("expected a name after '::'"));
                };
                self.bump();
                path.push(part);
            }
            return Ok(Expr::Enum(path));
        }
        if matches!(name.as_str(), "sizeof" | "bitsizeof") && self.current == Token::Lt {
            // `sizeof<type>`: a marker the schema compiler replaces
            // with the size, so it is a name that no field can have.
            let ty = self.parse_type_arg()?;
            return Ok(Expr::Field(vec![format!("{name}<{ty}>")]));
        }
        Ok(match name.as_str() {
            "true" => Expr::Bool(true),
            "false" => Expr::Bool(false),
            _ => Expr::Field(vec![name]),
        })
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
            Token::Ident(name) => self.parse_name(name),
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

/// Fail if `src` has more operators and suffixes than [`MAX_EXPR_LINKS`].
///
/// A chain of them (`1+1+1+…`, `a.f().g()…`) makes the tree one level deeper
/// per link without any parenthesis, and the evaluation and dropping of a tree
/// recurse once per level in the worst case. Counting the tokens first leaves
/// the parser, which recurses per level of nesting, as small as it is.
fn check_links(src: &str) -> Result<()> {
    let mut lexer = Lexer::new(src);
    let mut links = 0usize;
    loop {
        match lexer.next_token() {
            Token::Eof => return Ok(()),
            Token::Plus
            | Token::Minus
            | Token::Star
            | Token::Slash
            | Token::Percent
            | Token::Shl
            | Token::Shr
            | Token::Amp
            | Token::Caret
            | Token::Pipe
            | Token::AmpAmp
            | Token::PipePipe
            | Token::EqEq
            | Token::BangEq
            | Token::Dot
            | Token::LBracket => {
                links += 1;
                if links > MAX_EXPR_LINKS {
                    return Err(DoeError::ExprParse {
                        expr: format!("{}…", src.chars().take(40).collect::<String>()),
                        message: format!(
                            "expression has more than {MAX_EXPR_LINKS} operators or suffixes"
                        ),
                    });
                }
            }
            _ => {}
        }
    }
}

/// Parse an expression string into an AST.
///
/// # Errors
///
/// Returns [`DoeError::ExprParse`] on a syntax error, if `src` is longer than
/// [`MAX_EXPR_LEN`] bytes, if it has more than [`MAX_EXPR_LINKS`] operators and
/// suffixes, or if it nests more than [`MAX_EXPR_DEPTH`] levels of parentheses
/// or unary operators.
pub fn parse(src: &str) -> Result<Expr> {
    if src.len() > MAX_EXPR_LEN {
        return Err(DoeError::ExprParse {
            expr: format!("{}…", src.chars().take(40).collect::<String>()),
            message: format!("expression is longer than {MAX_EXPR_LEN} bytes"),
        });
    }
    check_links(src)?;
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
    /// A parsed sub-type: its fields, in order.
    Struct(Vec<(String, ExprValue)>),
    /// A value too large to hold in an expression (a long `bytes` field, a
    /// very large array). Only its length is known, so `.size` and `.length`
    /// work and any other use is an error.
    Omitted {
        /// Its length: bytes, or items.
        len: usize,
    },
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
            ExprValue::Struct(_) => "struct",
            ExprValue::Omitted { .. } => "omitted value",
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
            ExprValue::Struct(fields) => write!(f, "<struct with {} fields>", fields.len()),
            ExprValue::Omitted { len } => write!(f, "<value of {len} too large to use>"),
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
/// Each "frame" is a flat mapping of field name → value, with the frame that
/// `_parent` means. The frames in use are a stack, and the last one is the
/// current scope. A frame that is [retained](EvalContext::retain_scope) outlives
/// its scope, so that something can be computed in it later.
#[derive(Debug)]
pub struct EvalContext {
    /// All the frames, by number; the stack and the retained frames are in it.
    frames: Vec<Frame>,
    /// The numbers of the frames in use, outermost first. Never empty.
    stack: Vec<usize>,
    /// Numbers of frames that can be used again.
    free: Vec<usize>,
    /// What `_io` stands for, if an expression is being evaluated while parsing.
    io: Option<Io>,
    /// The values of the instances that were computed after the struct they
    /// belong to was bound, by their number (see [`EvalContext::deferred_marker`]).
    resolved: std::collections::HashMap<u64, ExprValue>,
}

/// The name of the only field of the struct that stands for an instance that
/// is computed later.
const DEFERRED_FIELD: &str = "$deferred";

/// The name of the only field of the struct that stands for a field that was not read (its `if:` was false).
const ABSENT_FIELD: &str = "$absent";

/// The value of a field that is not there: it is equal to nothing, not even to itself.
pub fn absent_marker() -> ExprValue {
    ExprValue::Struct(vec![(ABSENT_FIELD.to_owned(), ExprValue::Int(0))])
}

/// Whether `value` stands for a field that is not there.
pub fn is_absent(value: &ExprValue) -> bool {
    matches!(value, ExprValue::Struct(fields) if matches!(fields.as_slice(), [(name, _)] if name == ABSENT_FIELD))
}

/// One scope of an [`EvalContext`].
#[derive(Debug, Default)]
struct Frame {
    vars: std::collections::HashMap<String, ExprValue>,
    /// The frame `_parent` means; `None` for the root.
    parent: Option<usize>,
    /// Whether the frame stays when its scope is left.
    keep: bool,
}

/// The stream the expression being evaluated is in.
#[derive(Debug, Clone, Copy)]
struct Io {
    start: usize,
    pos: usize,
    size: usize,
    eof: bool,
}

impl EvalContext {
    /// Create a context with one empty (root) frame.
    pub fn new() -> Self {
        EvalContext {
            frames: vec![Frame::default()],
            stack: vec![0],
            free: Vec::new(),
            io: None,
            resolved: std::collections::HashMap::new(),
        }
    }

    /// The value that stands, in the struct it belongs to, for the instance
    /// number `id`, which is computed later.
    pub fn deferred_marker(id: u64) -> ExprValue {
        ExprValue::Struct(vec![(
            DEFERRED_FIELD.to_owned(),
            ExprValue::Int(i128::from(id)),
        )])
    }

    /// Say what the instance number `id` turned out to be.
    pub fn resolve_deferred(&mut self, id: u64, value: ExprValue) {
        self.resolved.insert(id, value);
    }

    /// `value`, or the instance it stands for if it is the marker of one.
    fn settle(&self, value: ExprValue, src: &str) -> Result<ExprValue> {
        match deferred_id(&value) {
            Some(id) => self.resolved.get(&id).cloned().ok_or_else(|| {
                fail(
                    src,
                    "an instance that is computed later is not computed yet",
                )
            }),
            None => Ok(value),
        }
    }

    /// Set what `_io` stands for: the position within the stream the
    /// expression is in, its size and whether it is at its end.
    pub fn set_io(&mut self, pos: usize, size: usize, eof: bool) {
        self.set_io_at(0, pos, size, eof);
    }

    /// Like [`EvalContext::set_io`], for a stream that starts at `start` in the
    /// whole input.
    pub fn set_io_at(&mut self, start: usize, pos: usize, size: usize, eof: bool) {
        self.io = Some(Io {
            start,
            pos,
            size,
            eof,
        });
    }

    /// The number of the current frame.
    pub fn current_frame(&self) -> usize {
        self.stack.last().copied().unwrap_or(0)
    }

    /// The number of the frame `levels` below the current one on the stack.
    pub fn enclosing_frame(&self, levels: usize) -> Option<usize> {
        let at = self.stack.len().checked_sub(1)?.checked_sub(levels)?;
        self.stack.get(at).copied()
    }

    /// How many scopes are entered, the root included.
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// Enter a new, empty scope, and return its number.
    pub fn push_frame(&mut self) -> usize {
        let parent = self.current_frame();
        self.enter_new(Some(parent))
    }

    /// Enter a new scope whose `_parent` is not the scope it is entered from but
    /// the `_parent` of that, `up` times over (`parent: _parent` of a field).
    pub fn push_frame_under(&mut self, up: usize) -> usize {
        let mut target = Some(self.current_frame());
        for _ in 0..up {
            target = target.and_then(|frame| self.parent_of(frame));
        }
        self.enter_new(target)
    }

    fn enter_new(&mut self, parent: Option<usize>) -> usize {
        let id = if let Some(id) = self.free.pop() {
            if let Some(frame) = self.frames.get_mut(id) {
                *frame = Frame {
                    parent,
                    ..Frame::default()
                };
            }
            id
        } else {
            self.frames.push(Frame {
                parent,
                ..Frame::default()
            });
            self.frames.len() - 1
        };
        self.stack.push(id);
        id
    }

    /// The frame `_parent` means in frame `frame`.
    fn parent_of(&self, frame: usize) -> Option<usize> {
        self.frames.get(frame).and_then(|f| f.parent)
    }

    /// Leave the innermost scope. The root scope is never popped. The frame is
    /// forgotten, unless it was retained.
    pub fn pop_frame(&mut self) {
        if self.stack.len() > 1
            && let Some(id) = self.stack.pop()
            && let Some(frame) = self.frames.get_mut(id)
            && !frame.keep
        {
            frame.vars = std::collections::HashMap::new();
            self.free.push(id);
        }
    }

    /// Keep the current frame, and the ones its `_parent`s lead to, when their
    /// scopes are left, so that they can be [entered](EvalContext::enter_frame)
    /// again.
    pub fn retain_scope(&mut self) {
        let mut at = Some(self.current_frame());
        while let Some(id) = at {
            let Some(frame) = self.frames.get_mut(id) else {
                break;
            };
            frame.keep = true;
            at = frame.parent;
        }
    }

    /// Make frame `id` the current scope, with the frames its `_parent`s lead
    /// to around it, in place of the scopes entered now. Returns what to give to
    /// [`EvalContext::restore_frames`].
    pub fn enter_frame(&mut self, id: usize) -> Vec<usize> {
        let mut chain = Vec::new();
        let mut at = Some(id);
        while let Some(frame) = at {
            chain.push(frame);
            at = self.parent_of(frame);
        }
        chain.reverse();
        if chain.first() != Some(&0) {
            chain.insert(0, 0);
        }
        std::mem::replace(&mut self.stack, chain)
    }

    /// Go back to the scopes that were entered before [`EvalContext::enter_frame`].
    pub fn restore_frames(&mut self, stack: Vec<usize>) {
        self.stack = stack;
    }

    /// Bind `name` → `value` in the current (innermost) frame.
    pub fn bind(&mut self, name: &str, value: impl Into<ExprValue>) {
        let current = self.current_frame();
        if let Some(frame) = self.frames.get_mut(current) {
            frame.vars.insert(name.to_owned(), value.into());
        }
    }

    /// The value bound to `name` in the current frame, if any.
    pub fn get(&self, name: &str) -> Option<&ExprValue> {
        self.frames.get(self.current_frame())?.vars.get(name)
    }

    /// Remove `name` and every `name.field` binding from the current frame.
    pub fn unbind_prefix(&mut self, name: &str) {
        let dotted = format!("{name}.");
        let current = self.current_frame();
        if let Some(frame) = self.frames.get_mut(current) {
            frame
                .vars
                .retain(|k, _| k != name && !k.starts_with(&dotted));
        }
    }

    /// Resolve a qualified path like `["_parent", "count"]`, or one that ends
    /// in members of a bound value like `["name", "length"]`.
    fn resolve_path(&self, parts: &[String], src: &str) -> Result<ExprValue> {
        self.resolve_ref(parts, src).map(Cow::into_owned)
    }

    /// Like [`EvalContext::resolve_path`], but borrows the value when the path
    /// ends at one, so that indexing a large array copies only the element.
    fn resolve_ref<'a>(&'a self, parts: &[String], src: &str) -> Result<Cow<'a, ExprValue>> {
        let mut rest = parts;

        if let Some((first, tail)) = rest.split_first()
            && first == "_io"
        {
            let io = self
                .io
                .ok_or_else(|| fail(src, "_io is only available while parsing"))?;
            let stream = ExprValue::Struct(vec![
                ("start".to_owned(), length(io.start)),
                ("pos".to_owned(), length(io.pos)),
                ("size".to_owned(), length(io.size)),
                ("eof".to_owned(), ExprValue::Bool(io.eof)),
            ]);
            return apply_members(Cow::Owned(stream), tail, &self.resolved, src);
        }

        // Start at the innermost frame, or at the outermost for `_root`.
        let mut frame_idx = self.current_frame();
        if let Some((first, tail)) = rest.split_first()
            && first == "_root"
        {
            frame_idx = 0;
            rest = tail;
        }

        // Walk _parent segments
        while let Some((first, tail)) = rest.split_first()
            && first == "_parent"
        {
            frame_idx = self
                .parent_of(frame_idx)
                .ok_or_else(|| fail(src, "_parent: already at root scope"))?;
            rest = tail;
        }

        if rest.is_empty() {
            return Err(fail(
                src,
                "a field path ends at _root or _parent with no field name",
            ));
        }
        let frame = self
            .frames
            .get(frame_idx)
            .ok_or_else(|| fail(src, "no scope available"))?;
        // The longest prefix of the path that is bound names a value; anything
        // after it is a member of that value.
        for split in (1..=rest.len()).rev() {
            let (Some(head), Some(members)) = (rest.get(..split), rest.get(split..)) else {
                continue;
            };
            if let Some(value) = frame.vars.get(head.join(".").as_str()) {
                return apply_members(Cow::Borrowed(value), members, &self.resolved, src);
            }
        }
        Err(fail(src, &format!("unknown field '{}'", parts.join("."))))
    }
}

/// Apply `.member`s to a value: a field of a sub-type, or a method without
/// arguments. Fields are followed by reference, so only the final value is
/// copied.
fn apply_members<'a>(
    mut value: Cow<'a, ExprValue>,
    members: &[String],
    resolved: &'a std::collections::HashMap<u64, ExprValue>,
    src: &str,
) -> Result<Cow<'a, ExprValue>> {
    for member in members {
        value = settled(value, resolved, src)?;
        value = match value {
            Cow::Borrowed(ExprValue::Struct(fields)) => Cow::Borrowed(
                find_field(fields, member)
                    .ok_or_else(|| fail(src, &format!("a struct has no field '{member}'")))?,
            ),
            Cow::Owned(ExprValue::Struct(fields)) => Cow::Owned(
                fields
                    .into_iter()
                    .find(|(name, _)| {
                        name == member || name.strip_prefix(PARAM_PREFIX) == Some(member)
                    })
                    .map(|(_, v)| v)
                    .ok_or_else(|| fail(src, &format!("a struct has no field '{member}'")))?,
            ),
            other => Cow::Owned(call_method(&other, member, &[], src)?),
        };
    }
    settled(value, resolved, src)
}

/// The number of the instance that `value` stands for, if it is the marker of
/// one that is computed later.
pub fn deferred_id(value: &ExprValue) -> Option<u64> {
    match value {
        ExprValue::Struct(fields) => match fields.as_slice() {
            [(name, ExprValue::Int(id))] if name == DEFERRED_FIELD => u64::try_from(*id).ok(),
            _ => None,
        },
        _ => None,
    }
}

/// `value`, or the instance it stands for if it is the marker of one.
fn settled<'a>(
    value: Cow<'a, ExprValue>,
    resolved: &'a std::collections::HashMap<u64, ExprValue>,
    src: &str,
) -> Result<Cow<'a, ExprValue>> {
    match deferred_id(&value) {
        Some(id) => resolved.get(&id).map(Cow::Borrowed).ok_or_else(|| {
            fail(
                src,
                "an instance that is computed later is not computed yet",
            )
        }),
        None => Ok(value),
    }
}

/// The start of the name of a field that holds a parameter of a type, which
/// expressions read as a field of the struct (`color_mask_given.has_alpha_mask`)
/// and the output leaves out.
pub const PARAM_PREFIX: &str = "$param:";

fn find_field<'a>(fields: &'a [(String, ExprValue)], name: &str) -> Option<&'a ExprValue> {
    fields.iter().find_map(|(field, value)| {
        (field == name || field.strip_prefix(PARAM_PREFIX) == Some(name)).then_some(value)
    })
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
                let recv = ctx.settle(pop(&mut values, src)?, src)?;
                values.push(call_method(&recv, name, &[], src)?);
            }
            Work::Call(name, n) => {
                let args = values.split_off(values.len().saturating_sub(n));
                let recv = ctx.settle(pop(&mut values, src)?, src)?;
                values.push(call_method(&recv, name, &args, src)?);
            }
            Work::Index => {
                let (index, target) = (pop(&mut values, src)?, pop(&mut values, src)?);
                values.push(index_value(&target, &index, src)?);
            }
            Work::IndexField(parts) => {
                // Index a field in place, without copying all of it first.
                let index = pop(&mut values, src)?;
                let target = ctx.resolve_ref(parts, src)?;
                values.push(index_value(&target, &index, src)?);
            }
        }
    }
    ctx.settle(pop(&mut values, src)?, src)
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
    /// Index the field at this path by the value on top.
    IndexField(&'e [String]),
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
            // As in Kaitai Struct, a list of integer literals that all fit in
            // a byte is a byte sequence.
            let bytes: Option<Vec<u8>> = items
                .iter()
                .map(|item| match item {
                    Expr::Int(n) => u8::try_from(*n).ok(),
                    _ => None,
                })
                .collect();
            match bytes {
                Some(bytes) if !bytes.is_empty() => values.push(ExprValue::Bytes(bytes)),
                _ => {
                    work.push(Work::Array(items.len()));
                    work.extend(items.iter().rev().map(Work::Eval));
                }
            }
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
            if let Expr::Field(parts) = &**target {
                work.push(Work::IndexField(parts));
                work.push(Work::Eval(index));
            } else {
                work.push(Work::Index);
                work.push(Work::Eval(index));
                work.push(Work::Eval(target));
            }
        }
        Expr::Enum(path) => {
            return Err(fail(
                src,
                &format!(
                    "the enum member {} was not resolved: enum members are resolved when a schema is compiled",
                    path.join("::")
                ),
            ));
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

/// Whether the value of an expression is the same wherever and whenever it is
/// worked out: it names no field, no enum member and no stream.
pub fn is_constant(expr: &Expr) -> bool {
    match expr {
        Expr::Int(_) | Expr::Float(_) | Expr::Bool(_) | Expr::Str(_) => true,
        Expr::Array(items) => items.iter().all(is_constant),
        Expr::Enum(_) | Expr::Field(_) => false,
        Expr::Member(inner, _) | Expr::Unary(_, inner) => is_constant(inner),
        Expr::Call(inner, _, args) => is_constant(inner) && args.iter().all(is_constant),
        Expr::Index(left, right) | Expr::Binary(_, left, right) => {
            is_constant(left) && is_constant(right)
        }
        Expr::Ternary(a, b, c) => is_constant(a) && is_constant(b) && is_constant(c),
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
            // A field that is not there is equal to nothing (`vr == 'SQ'` is false when `vr` was not read).
            if is_absent(&l) || is_absent(&r) {
                return match op {
                    BinaryOp::Eq => Ok(ExprValue::Bool(false)),
                    BinaryOp::Ne => Ok(ExprValue::Bool(true)),
                    _ => Err(fail(src, "a field that is not there cannot be ordered")),
                };
            }
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

fn index_value(target: &ExprValue, index: &ExprValue, src: &str) -> Result<ExprValue> {
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
        ExprValue::Omitted { .. } => Err(fail(
            src,
            "the value is too large to use in an expression; only its size is available",
        )),
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
fn call_method(recv: &ExprValue, name: &str, args: &[ExprValue], src: &str) -> Result<ExprValue> {
    use ExprValue::{Array, Bool, Bytes, Float, Int, Omitted, Str, Struct};
    match (recv, name) {
        (Struct(fields), _) => {
            expect_args(name, args, &[0], src)?;
            find_field(fields, name)
                .cloned()
                .ok_or_else(|| fail(src, &format!("a struct has no field '{name}'")))
        }
        (Omitted { len }, "length" | "size") => {
            expect_args(name, args, &[0], src)?;
            Ok(length(*len))
        }
        (Omitted { .. }, _) => Err(fail(
            src,
            &format!(
                "'{name}': the value is too large to use in an expression; only its size is available"
            ),
        )),
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
        (Str(_), "to_s") | (Int(_), "to_i") | (Float(_), "to_f") | (Bytes(_), "as<bytes>") => {
            expect_args(name, args, &[0], src)?;
            Ok(recv.clone())
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
        (Array(items), "as<bytes>") => {
            expect_args(name, args, &[0], src)?;
            let bytes: Option<Vec<u8>> = items
                .iter()
                .map(|item| match item {
                    Int(n) => u8::try_from(*n).ok(),
                    _ => None,
                })
                .collect();
            bytes
                .map(Bytes)
                .ok_or_else(|| fail(src, "as<bytes>: the list has an item that is not a byte"))
        }
        (Bytes(b), "to_s") => {
            expect_args(name, args, &[1], src)?;
            let [Str(encoding)] = args else {
                return Err(fail(src, "to_s: the encoding is a string"));
            };
            let encoding = crate::schema::raw::RawEncoding::from_name(encoding)
                .ok_or_else(|| fail(src, &format!("to_s: unknown encoding '{encoding}'")))?;
            crate::parser::encoding::decode(
                b,
                crate::schema::compiler::compile_encoding(Some(encoding)),
            )
            .map(Str)
            .map_err(|e| fail(src, &format!("to_s: {e}")))
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

/// How two values order under `<`, or `None` if they are unordered (a NaN).
///
/// # Errors
///
/// Returns [`DoeError::ExprEval`] if they are of types that cannot be
/// compared, such as a string and an integer.
pub fn compare_values(a: &ExprValue, b: &ExprValue, src: &str) -> Result<Option<Ordering>> {
    compare(a, b, src)
}

/// Whether two values are equal under `==`.
///
/// # Errors
///
/// Returns [`DoeError::ExprEval`] if they are of types that cannot be
/// compared, such as a string and an integer.
pub fn values_equal(a: &ExprValue, b: &ExprValue, src: &str) -> Result<bool> {
    compare(a, b, src).map(|ord| ord == Some(Ordering::Equal))
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
