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

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// Integer literal.
    Int(i64),
    /// Boolean literal (currently produced only by direct construction;
    /// the parser normalises `true`/`false` to `Int(1)`/`Int(0)`).
    #[allow(dead_code)]
    Bool(bool),
    /// Field reference, possibly qualified: `["_parent", "length"]`.
    Field(Vec<String>),
    /// Unary operator.
    Unary(UnaryOp, Box<Expr>),
    /// Binary operator.
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Shl,
    Shr,
    BitAnd,
    BitOr,
    BitXor,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    And,
    Or,
}

impl fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            BinaryOp::Add => "+",   BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",   BinaryOp::Div => "/",
            BinaryOp::Rem => "%",   BinaryOp::Shl => "<<",
            BinaryOp::Shr => ">>",  BinaryOp::BitAnd => "&",
            BinaryOp::BitOr => "|", BinaryOp::BitXor => "^",
            BinaryOp::Eq => "==",   BinaryOp::Ne => "!=",
            BinaryOp::Lt => "<",    BinaryOp::Gt => ">",
            BinaryOp::Le => "<=",   BinaryOp::Ge => ">=",
            BinaryOp::And => "&&",  BinaryOp::Or => "||",
        };
        write!(f, "{}", s)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Lexer
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Int(i64),
    Ident(String),
    // Punctuation
    Plus, Minus, Star, Slash, Percent,
    Shl, Shr,
    Amp, Pipe, Caret,
    AmpAmp, PipePipe,
    Bang,
    EqEq, BangEq,
    Lt, Gt, Le, Ge,
    Dot,
    LParen, RParen,
    Eof,
}

struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(s: &'a str) -> Self {
        Lexer { src: s.as_bytes(), pos: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn peek2(&self) -> Option<u8> {
        self.src.get(self.pos + 1).copied()
    }

    fn advance(&mut self) -> Option<u8> {
        let b = self.src.get(self.pos).copied();
        if b.is_some() { self.pos += 1; }
        b
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.advance();
        }
    }

    fn read_int(&mut self) -> i64 {
        let start = self.pos;
        // Hex literal
        if self.peek() == Some(b'0') && matches!(self.peek2(), Some(b'x' | b'X')) {
            self.advance(); self.advance(); // consume "0x"
            while matches!(self.peek(), Some(b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F')) {
                self.advance();
            }
            let hex = std::str::from_utf8(&self.src[start + 2..self.pos]).unwrap();
            return i64::from_str_radix(hex, 16).unwrap_or(0);
        }
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.advance();
        }
        std::str::from_utf8(&self.src[start..self.pos])
            .unwrap()
            .parse()
            .unwrap_or(0)
    }

    fn read_ident(&mut self) -> String {
        let start = self.pos;
        while matches!(self.peek(), Some(b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_')) {
            self.advance();
        }
        String::from_utf8(self.src[start..self.pos].to_vec()).unwrap()
    }

    fn next_token(&mut self) -> Token {
        self.skip_ws();
        match self.peek() {
            None => Token::Eof,
            Some(b'0'..=b'9') => Token::Int(self.read_int()),
            Some(b'a'..=b'z' | b'A'..=b'Z' | b'_') => {
                let id = self.read_ident();
                match id.as_str() {
                    "true"  => Token::Int(1),
                    "false" => Token::Int(0),
                    _       => Token::Ident(id),
                }
            }
            Some(b'+') => { self.advance(); Token::Plus }
            Some(b'-') => { self.advance(); Token::Minus }
            Some(b'*') => { self.advance(); Token::Star }
            Some(b'/') => { self.advance(); Token::Slash }
            Some(b'%') => { self.advance(); Token::Percent }
            Some(b'^') => { self.advance(); Token::Caret }
            Some(b'.') => { self.advance(); Token::Dot }
            Some(b'(') => { self.advance(); Token::LParen }
            Some(b')') => { self.advance(); Token::RParen }
            Some(b'<') => {
                self.advance();
                match self.peek() {
                    Some(b'<') => { self.advance(); Token::Shl }
                    Some(b'=') => { self.advance(); Token::Le  }
                    _          => Token::Lt,
                }
            }
            Some(b'>') => {
                self.advance();
                match self.peek() {
                    Some(b'>') => { self.advance(); Token::Shr }
                    Some(b'=') => { self.advance(); Token::Ge  }
                    _          => Token::Gt,
                }
            }
            Some(b'=') => {
                self.advance();
                if self.peek() == Some(b'=') { self.advance(); Token::EqEq }
                else { Token::Eof } // lone `=` is not valid
            }
            Some(b'!') => {
                self.advance();
                if self.peek() == Some(b'=') { self.advance(); Token::BangEq }
                else { Token::Bang }
            }
            Some(b'&') => {
                self.advance();
                if self.peek() == Some(b'&') { self.advance(); Token::AmpAmp }
                else { Token::Amp }
            }
            Some(b'|') => {
                self.advance();
                if self.peek() == Some(b'|') { self.advance(); Token::PipePipe }
                else { Token::Pipe }
            }
            Some(c) => {
                self.advance();
                // Return Eof for unrecognised characters so the parser can
                // produce a meaningful error rather than looping.
                let _ = c;
                Token::Eof
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
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        let mut lexer = Lexer::new(src);
        let current = lexer.next_token();
        Parser { lexer, current, src }
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

    fn parse_expr(&mut self) -> Result<Expr> { self.parse_or() }

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
            Token::EqEq  => BinaryOp::Eq,
            Token::BangEq=> BinaryOp::Ne,
            Token::Lt    => BinaryOp::Lt,
            Token::Gt    => BinaryOp::Gt,
            Token::Le    => BinaryOp::Le,
            Token::Ge    => BinaryOp::Ge,
            _            => return Ok(lhs),
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
                _          => break,
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
                Token::Plus  => BinaryOp::Add,
                Token::Minus => BinaryOp::Sub,
                _            => break,
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
                Token::Star    => BinaryOp::Mul,
                Token::Slash   => BinaryOp::Div,
                Token::Percent => BinaryOp::Rem,
                _              => break,
            };
            self.bump();
            let rhs = self.parse_unary()?;
            lhs = Expr::Binary(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_unary(&mut self) -> Result<Expr> {
        match self.current.clone() {
            Token::Minus => { self.bump(); let e = self.parse_unary()?; Ok(Expr::Unary(UnaryOp::Neg, Box::new(e))) }
            Token::Bang  => { self.bump(); let e = self.parse_unary()?; Ok(Expr::Unary(UnaryOp::Not, Box::new(e))) }
            _            => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        match self.current.clone() {
            Token::Int(n) => { self.bump(); Ok(Expr::Int(n)) }
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
                let e = self.parse_expr()?;
                if self.current != Token::RParen {
                    return Err(self.err("expected ')'"));
                }
                self.bump();
                Ok(e)
            }
            _ => Err(self.err(&format!("unexpected token {:?}", self.current))),
        }
    }
}

/// Parse an expression string into an AST.
pub fn parse(src: &str) -> Result<Expr> {
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
pub struct EvalContext {
    /// Frames stored outer-to-inner; last element is current scope.
    frames: Vec<std::collections::HashMap<String, i64>>,
}

impl EvalContext {
    pub fn new() -> Self {
        EvalContext { frames: vec![std::collections::HashMap::new()] }
    }

    pub fn push_frame(&mut self) {
        self.frames.push(std::collections::HashMap::new());
    }

    pub fn pop_frame(&mut self) {
        if self.frames.len() > 1 {
            self.frames.pop();
        }
    }

    /// Bind `name` → `value` in the current (innermost) frame.
    pub fn bind(&mut self, name: &str, value: i64) {
        self.frames.last_mut().unwrap().insert(name.to_owned(), value);
    }

    /// Look up a simple name in the current frame.
    #[allow(dead_code)]
    fn lookup_current(&self, name: &str) -> Option<i64> {
        self.frames.last()?.get(name).copied()
    }

    /// Resolve a qualified path like `["_parent", "count"]`.
    fn resolve_path(&self, parts: &[String], src: &str) -> Result<i64> {
        let mut frame_idx = self.frames.len() - 1; // start at innermost
        let mut part_idx = 0;

        // Walk _parent segments
        while part_idx < parts.len() && parts[part_idx] == "_parent" {
            if frame_idx == 0 {
                return Err(DoeError::ExprEval {
                    expr: src.to_owned(),
                    message: "_parent: already at root scope".to_owned(),
                });
            }
            frame_idx -= 1;
            part_idx += 1;
        }

        if part_idx >= parts.len() {
            return Err(DoeError::ExprEval {
                expr: src.to_owned(),
                message: "field path ends at _parent with no field name".to_owned(),
            });
        }

        let name = &parts[part_idx];
        self.frames[frame_idx].get(name.as_str()).copied().ok_or_else(|| {
            DoeError::ExprEval {
                expr: src.to_owned(),
                message: format!("unknown field '{}'", parts.join(".")),
            }
        })
    }
}

impl Default for EvalContext {
    fn default() -> Self { Self::new() }
}

// ─────────────────────────────────────────────────────────────────────────────
// Evaluator
// ─────────────────────────────────────────────────────────────────────────────

/// Evaluate an already-parsed expression against a context.
pub fn eval(expr: &Expr, ctx: &EvalContext, src: &str) -> Result<i64> {
    match expr {
        Expr::Int(n)  => Ok(*n),
        Expr::Bool(b) => Ok(*b as i64),

        Expr::Field(parts) => ctx.resolve_path(parts, src),

        Expr::Unary(op, inner) => {
            let v = eval(inner, ctx, src)?;
            match op {
                UnaryOp::Neg => Ok(-v),
                UnaryOp::Not => Ok(if v == 0 { 1 } else { 0 }),
            }
        }

        Expr::Binary(op, lhs, rhs) => {
            let l = eval(lhs, ctx, src)?;
            // Short-circuit logical operators before evaluating rhs
            match op {
                BinaryOp::And => return Ok(if l == 0 { 0 } else { eval(rhs, ctx, src)? }),
                BinaryOp::Or  => return Ok(if l != 0 { 1 } else { eval(rhs, ctx, src)? }),
                _ => {}
            }
            let r = eval(rhs, ctx, src)?;
            match op {
                BinaryOp::Add    => Ok(l.wrapping_add(r)),
                BinaryOp::Sub    => Ok(l.wrapping_sub(r)),
                BinaryOp::Mul    => Ok(l.wrapping_mul(r)),
                BinaryOp::Div    => {
                    if r == 0 {
                        return Err(DoeError::ExprEval { expr: src.to_owned(), message: "division by zero".to_owned() });
                    }
                    Ok(l / r)
                }
                BinaryOp::Rem    => {
                    if r == 0 {
                        return Err(DoeError::ExprEval { expr: src.to_owned(), message: "modulo by zero".to_owned() });
                    }
                    Ok(l % r)
                }
                BinaryOp::Shl    => Ok(l.wrapping_shl(r as u32)),
                BinaryOp::Shr    => Ok(l.wrapping_shr(r as u32)),
                BinaryOp::BitAnd => Ok(l & r),
                BinaryOp::BitOr  => Ok(l | r),
                BinaryOp::BitXor => Ok(l ^ r),
                BinaryOp::Eq     => Ok((l == r) as i64),
                BinaryOp::Ne     => Ok((l != r) as i64),
                BinaryOp::Lt     => Ok((l <  r) as i64),
                BinaryOp::Gt     => Ok((l >  r) as i64),
                BinaryOp::Le     => Ok((l <= r) as i64),
                BinaryOp::Ge     => Ok((l >= r) as i64),
                BinaryOp::And | BinaryOp::Or => unreachable!(),
            }
        }
    }
}

/// Convenience: parse then evaluate in one call.
#[allow(dead_code)]
pub fn eval_str(src: &str, ctx: &EvalContext) -> Result<i64> {
    let ast = parse(src)?;
    eval(&ast, ctx, src)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> EvalContext { EvalContext::new() }

    fn eval(s: &str) -> i64 { eval_str(s, &ctx()).expect(s) }

    fn eval_with(bindings: &[(&str, i64)], s: &str) -> i64 {
        let mut c = ctx();
        for (k, v) in bindings { c.bind(k, *v); }
        eval_str(s, &c).expect(s)
    }

    fn parse_err(s: &str) -> bool { parse(s).is_err() }
    fn eval_err(s: &str) -> bool  { eval_str(s, &ctx()).is_err() }

    // ── Parser: literals ─────────────────────────────────────────────────────

    #[test]
    fn integer_literal() { assert_eq!(eval("42"), 42); }

    #[test]
    fn zero_literal() { assert_eq!(eval("0"), 0); }

    #[test]
    fn hex_literal() { assert_eq!(eval("0xff"), 255); }

    #[test]
    fn hex_literal_upper() { assert_eq!(eval("0xFF"), 255); }

    #[test]
    fn true_literal() { assert_eq!(eval("true"), 1); }

    #[test]
    fn false_literal() { assert_eq!(eval("false"), 0); }

    // ── Parser: arithmetic ───────────────────────────────────────────────────

    #[test]
    fn addition() { assert_eq!(eval("1 + 2"), 3); }

    #[test]
    fn subtraction() { assert_eq!(eval("10 - 3"), 7); }

    #[test]
    fn multiplication() { assert_eq!(eval("3 * 4"), 12); }

    #[test]
    fn division() { assert_eq!(eval("10 / 2"), 5); }

    #[test]
    fn remainder() { assert_eq!(eval("10 % 3"), 1); }

    #[test]
    fn precedence_mul_before_add() { assert_eq!(eval("2 + 3 * 4"), 14); }

    #[test]
    fn parentheses_override_precedence() { assert_eq!(eval("(2 + 3) * 4"), 20); }

    #[test]
    fn unary_negation() { assert_eq!(eval("-5"), -5); }

    #[test]
    fn double_negation() { assert_eq!(eval("--5"), 5); }

    // ── Parser: bitwise ──────────────────────────────────────────────────────

    #[test]
    fn bitwise_and() { assert_eq!(eval("0xff & 0x0f"), 0x0f); }

    #[test]
    fn bitwise_or() { assert_eq!(eval("0x0f | 0xf0"), 0xff); }

    #[test]
    fn bitwise_xor() { assert_eq!(eval("0xff ^ 0x0f"), 0xf0); }

    #[test]
    fn shift_left() { assert_eq!(eval("1 << 4"), 16); }

    #[test]
    fn shift_right() { assert_eq!(eval("0x10 >> 2"), 4); }

    // ── Parser: comparison ───────────────────────────────────────────────────

    #[test]
    fn eq_true()  { assert_eq!(eval("5 == 5"), 1); }
    #[test]
    fn eq_false() { assert_eq!(eval("5 == 6"), 0); }
    #[test]
    fn ne_true()  { assert_eq!(eval("5 != 6"), 1); }
    #[test]
    fn lt_true()  { assert_eq!(eval("3 < 5"),  1); }
    #[test]
    fn lt_false() { assert_eq!(eval("5 < 3"),  0); }
    #[test]
    fn le_eq()    { assert_eq!(eval("5 <= 5"), 1); }
    #[test]
    fn gt_true()  { assert_eq!(eval("5 > 3"),  1); }
    #[test]
    fn ge_eq()    { assert_eq!(eval("5 >= 5"), 1); }

    // ── Parser: logical ──────────────────────────────────────────────────────

    #[test]
    fn logical_and_both_true() { assert_eq!(eval("1 && 1"), 1); }
    #[test]
    fn logical_and_one_false() { assert_eq!(eval("1 && 0"), 0); }
    #[test]
    fn logical_or_one_true()  { assert_eq!(eval("0 || 1"), 1); }
    #[test]
    fn logical_or_both_false(){ assert_eq!(eval("0 || 0"), 0); }
    #[test]
    fn logical_not_true()     { assert_eq!(eval("!0"), 1); }
    #[test]
    fn logical_not_false()    { assert_eq!(eval("!1"), 0); }

    #[test]
    fn logical_and_short_circuits() {
        // Right side would divide by zero; should never evaluate due to short-circuit.
        // We can't fully test short-circuit without a side-effect, but we can
        // confirm 0 && anything == 0 with a field ref that's not in scope.
        let c = ctx();
        // "0 && missing" — rhs not evaluated when lhs is 0
        let ast = parse("0 && missing").unwrap();
        assert_eq!(super::eval(&ast, &c, "0 && missing").unwrap(), 0);
    }

    #[test]
    fn logical_or_short_circuits() {
        let c = ctx();
        let ast = parse("1 || missing").unwrap();
        assert_eq!(super::eval(&ast, &c, "1 || missing").unwrap(), 1);
    }

    // ── Evaluation: field references ─────────────────────────────────────────

    #[test]
    fn simple_field_ref() {
        assert_eq!(eval_with(&[("length", 10)], "length"), 10);
    }

    #[test]
    fn field_in_arithmetic() {
        assert_eq!(eval_with(&[("n", 5)], "n * 2 + 1"), 11);
    }

    #[test]
    fn field_comparison() {
        assert_eq!(eval_with(&[("version", 3)], "version > 2"), 1);
        assert_eq!(eval_with(&[("version", 1)], "version > 2"), 0);
    }

    #[test]
    fn unknown_field_is_error() {
        assert!(eval_err("unknown_field"));
    }

    // ── Evaluation: parent scope ──────────────────────────────────────────────

    #[test]
    fn parent_field_ref() {
        let mut c = ctx();
        c.bind("outer_count", 7);
        c.push_frame();
        c.bind("inner_val", 3);
        let ast = parse("_parent.outer_count").unwrap();
        assert_eq!(super::eval(&ast, &c, "_parent.outer_count").unwrap(), 7);
    }

    #[test]
    fn parent_field_in_arithmetic() {
        let mut c = ctx();
        c.bind("base", 100);
        c.push_frame();
        c.bind("offset", 5);
        assert_eq!(eval_str("_parent.base + offset", &c).unwrap(), 105);
    }

    #[test]
    fn too_many_parent_hops_is_error() {
        let mut c = ctx();
        c.push_frame();
        let ast = parse("_parent._parent.x").unwrap();
        assert!(super::eval(&ast, &c, "_parent._parent.x").is_err());
    }

    // ── Evaluation: division by zero ─────────────────────────────────────────

    #[test]
    fn division_by_zero_is_error() { assert!(eval_err("1 / 0")); }

    #[test]
    fn modulo_by_zero_is_error()   { assert!(eval_err("1 % 0")); }

    // ── Parser: error cases ───────────────────────────────────────────────────

    #[test]
    fn empty_expression_is_error() { assert!(parse_err("")); }

    #[test]
    fn trailing_operator_is_error() { assert!(parse_err("1 +")); }

    #[test]
    fn unmatched_paren_is_error() { assert!(parse_err("(1 + 2")); }

    #[test]
    fn dangling_dot_is_error() { assert!(parse_err("foo.")); }

    // ── Complex expressions ───────────────────────────────────────────────────

    #[test]
    fn complex_size_expr() {
        // (n * 4) + 8 — a plausible "size = count * 4 + header" pattern
        assert_eq!(eval_with(&[("n", 3)], "(n * 4) + 8"), 20);
    }

    #[test]
    fn bitfield_mask_expr() {
        // (flags >> 4) & 0x0f
        assert_eq!(eval_with(&[("flags", 0xab)], "(flags >> 4) & 0x0f"), 0x0a);
    }

    #[test]
    fn nested_conditional() {
        // (a > 0) && (b < 10)
        assert_eq!(eval_with(&[("a", 5), ("b", 3)], "(a > 0) && (b < 10)"), 1);
        assert_eq!(eval_with(&[("a", 5), ("b", 15)], "(a > 0) && (b < 10)"), 0);
    }

    // ── EvalContext: frame management ────────────────────────────────────────

    #[test]
    fn bind_and_lookup_current_frame() {
        let mut c = ctx();
        c.bind("x", 42);
        assert_eq!(c.lookup_current("x"), Some(42));
    }

    #[test]
    fn push_frame_shadows_outer() {
        let mut c = ctx();
        c.bind("x", 1);
        c.push_frame();
        c.bind("x", 2);
        assert_eq!(c.lookup_current("x"), Some(2));
    }

    #[test]
    fn pop_frame_restores_outer() {
        let mut c = ctx();
        c.bind("x", 1);
        c.push_frame();
        c.bind("x", 2);
        c.pop_frame();
        assert_eq!(c.lookup_current("x"), Some(1));
    }

    #[test]
    fn pop_below_root_is_safe() {
        let mut c = ctx();
        c.pop_frame(); // should not panic
        c.pop_frame();
    }
}
