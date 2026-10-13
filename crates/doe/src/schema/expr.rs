//! Expression language for schema field sizes, repeat counts, and conditionals.
//!
//! Grammar (in rough precedence order, lowest first):
//!
//! ```text
//! expr    = or
//! or      = and ( "||" and )*
//! and     = cmp  ( "&&" cmp  )*
//! cmp     = bitor ( ( "==" | "!=" | "<" | ">" | "<=" | ">=" ) bitor )?
//! bitor   = bitxor ( "|" bitxor )*
//! bitxor  = bitand ( "^" bitand )*
//! bitand  = shift  ( "&" shift  )*
//! shift   = add    ( ( "<<" | ">>" ) add )*
//! add     = mul    ( ( "+" | "-" ) mul )*
//! mul     = unary  ( ( "*" | "/" | "%" ) unary )*
//! unary   = ( "!" | "-" ) unary | primary
//! primary = INT | "true" | "false" | IDENT ( "." IDENT )* | "(" expr ")"
//! ```
//!
//! All integer arithmetic uses `i64`.  Booleans are represented as `i64`
//! (0 = false, 1 = true) so that `if` expressions and size expressions share
//! the same value type.

use std::fmt;

use crate::error::{DoeError, Result};

// ─────────────────────────────────────────────────────────────────────────────
// AST
// ─────────────────────────────────────────────────────────────────────────────

/// A parsed expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Integer literal.
    Int(i64),
    /// Boolean literal (currently produced only by direct construction;
    /// the parser normalises `true`/`false` to `Int(1)`/`Int(0)`).
    Bool(bool),
    /// Field reference, possibly qualified: `["_parent", "length"]`.
    Field(Vec<String>),
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
    Int(i64),
    /// An integer literal that is empty (`0x`) or does not fit in an `i64`.
    BadInt(String),
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

    fn read_int(&mut self) -> Token {
        let start = self.pos;
        // Hex literal
        if self.peek() == Some(b'0') && matches!(self.peek2(), Some(b'x' | b'X')) {
            self.advance();
            self.advance(); // consume "0x"
            while matches!(self.peek(), Some(b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F')) {
                self.advance();
            }
            let hex = self.slice_str(start.saturating_add(2), self.pos);
            return i64::from_str_radix(hex, 16).map_or_else(|_| self.bad_int(start), Token::Int);
        }
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.advance();
        }
        self.slice_str(start, self.pos)
            .parse()
            .map_or_else(|_| self.bad_int(start), Token::Int)
    }

    fn bad_int(&self, start: usize) -> Token {
        Token::BadInt(self.slice_str(start, self.pos).to_owned())
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
            Some(b'0'..=b'9') => self.read_int(),
            Some(b'a'..=b'z' | b'A'..=b'Z' | b'_') => {
                let id = self.read_ident();
                match id.as_str() {
                    "true" => Token::Int(1),
                    "false" => Token::Int(0),
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
        self.parse_or()
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
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        match self.current.clone() {
            Token::Int(n) => {
                self.bump();
                Ok(Expr::Int(n))
            }
            Token::Ident(name) => {
                self.bump();
                let mut parts = vec![name];
                while self.current == Token::Dot {
                    self.bump();
                    if let Token::Ident(part) = self.current.clone() {
                        self.bump();
                        parts.push(part);
                    } else {
                        return Err(self.err("expected identifier after '.'"));
                    }
                }
                Ok(Expr::Field(parts))
            }
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
            Token::BadInt(text) => Err(self.err(&format!(
                "integer literal {text} is not a number between 0 and {}",
                i64::MAX
            ))),
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
// Evaluation context
// ─────────────────────────────────────────────────────────────────────────────

/// A stack-based evaluation context.
///
/// Each "frame" is a flat mapping of field name → integer value.
/// `_parent` walks one frame up the stack.
#[derive(Debug)]
pub struct EvalContext {
    /// Frames stored outer-to-inner; last element is current scope.
    frames: Vec<std::collections::HashMap<String, i64>>,
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
    pub fn bind(&mut self, name: &str, value: i64) {
        if self.frames.is_empty() {
            self.frames.push(std::collections::HashMap::new());
        }
        if let Some(frame) = self.frames.last_mut() {
            frame.insert(name.to_owned(), value);
        }
    }

    /// Resolve a qualified path like `["_parent", "count"]`.
    fn resolve_path(&self, parts: &[String], src: &str) -> Result<i64> {
        let eval_err = |message: &str| DoeError::ExprEval {
            expr: src.to_owned(),
            message: message.to_owned(),
        };
        // Start at the innermost frame.
        let mut frame_idx = self
            .frames
            .len()
            .checked_sub(1)
            .ok_or_else(|| eval_err("no scope available"))?;
        let mut rest = parts;

        // Walk _parent segments
        while let Some((first, tail)) = rest.split_first()
            && first == "_parent"
        {
            frame_idx = frame_idx
                .checked_sub(1)
                .ok_or_else(|| eval_err("_parent: already at root scope"))?;
            rest = tail;
        }

        if rest.is_empty() {
            return Err(eval_err("field path ends at _parent with no field name"));
        }
        // A path into a parsed sub-type (`header.section_count`) is bound under
        // its dotted name.
        let name = rest.join(".");
        self.frames
            .get(frame_idx)
            .and_then(|frame| frame.get(name.as_str()))
            .copied()
            .ok_or_else(|| eval_err(&format!("unknown field '{}'", parts.join("."))))
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

/// Evaluate an already-parsed expression against a context.
///
/// # Errors
///
/// Returns [`DoeError::ExprEval`] for unknown fields, a bad `_parent`
/// path, or division or remainder by zero.
pub fn eval(expr: &Expr, ctx: &EvalContext, src: &str) -> Result<i64> {
    match expr {
        Expr::Int(n) => Ok(*n),
        Expr::Bool(b) => Ok(i64::from(*b)),

        Expr::Field(parts) => ctx.resolve_path(parts, src),

        Expr::Unary(op, inner) => {
            let v = eval(inner, ctx, src)?;
            match op {
                UnaryOp::Neg => Ok(v.wrapping_neg()),
                UnaryOp::Not => Ok(i64::from(v == 0)),
            }
        }

        Expr::Binary(op, lhs, rhs) => {
            let l = eval(lhs, ctx, src)?;
            // Short-circuit logical operators before evaluating rhs
            match op {
                BinaryOp::And => return Ok(if l == 0 { 0 } else { eval(rhs, ctx, src)? }),
                BinaryOp::Or => return Ok(if l != 0 { 1 } else { eval(rhs, ctx, src)? }),
                _ => {}
            }
            let r = eval(rhs, ctx, src)?;
            match op {
                BinaryOp::Add => Ok(l.wrapping_add(r)),
                BinaryOp::Sub => Ok(l.wrapping_sub(r)),
                BinaryOp::Mul => Ok(l.wrapping_mul(r)),
                BinaryOp::Div => {
                    if r == 0 {
                        return Err(DoeError::ExprEval {
                            expr: src.to_owned(),
                            message: "division by zero".to_owned(),
                        });
                    }
                    Ok(l.wrapping_div(r))
                }
                BinaryOp::Rem => {
                    if r == 0 {
                        return Err(DoeError::ExprEval {
                            expr: src.to_owned(),
                            message: "modulo by zero".to_owned(),
                        });
                    }
                    Ok(l.wrapping_rem(r))
                }
                BinaryOp::Shl => Ok(l.wrapping_shl(shift_amount(r))),
                BinaryOp::Shr => Ok(l.wrapping_shr(shift_amount(r))),
                BinaryOp::BitAnd => Ok(l & r),
                BinaryOp::BitOr => Ok(l | r),
                BinaryOp::BitXor => Ok(l ^ r),
                BinaryOp::Eq => Ok(i64::from(l == r)),
                BinaryOp::Ne => Ok(i64::from(l != r)),
                BinaryOp::Lt => Ok(i64::from(l < r)),
                BinaryOp::Gt => Ok(i64::from(l > r)),
                BinaryOp::Le => Ok(i64::from(l <= r)),
                BinaryOp::Ge => Ok(i64::from(l >= r)),
                // Handled above with short-circuit evaluation; kept consistent for exhaustiveness.
                BinaryOp::And => Ok(if l == 0 { 0 } else { r }),
                BinaryOp::Or => Ok(if l != 0 { 1 } else { r }),
            }
        }
    }
}

/// The shift distance for `<<` and `>>`: the low six bits of `r`, which is
/// what a 64-bit shift uses.
fn shift_amount(r: i64) -> u32 {
    u32::try_from(r & 63).unwrap_or(0)
}

/// Convenience: parse then evaluate in one call.
///
/// # Errors
///
/// Returns any error from [`parse`] or [`eval`].
pub fn eval_str(src: &str, ctx: &EvalContext) -> Result<i64> {
    let ast = parse(src)?;
    eval(&ast, ctx, src)
}
