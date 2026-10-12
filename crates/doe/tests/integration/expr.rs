use doe::schema::expr::*;

fn ctx() -> EvalContext {
    EvalContext::new()
}

fn eval(s: &str) -> i64 {
    eval_str(s, &ctx()).expect(s)
}

fn eval_with(bindings: &[(&str, i64)], s: &str) -> i64 {
    let mut c = ctx();
    for (k, v) in bindings {
        c.bind(k, *v);
    }
    eval_str(s, &c).expect(s)
}

fn parse_err(s: &str) -> bool {
    parse(s).is_err()
}
fn eval_err(s: &str) -> bool {
    eval_str(s, &ctx()).is_err()
}

// ── Parser: literals ─────────────────────────────────────────────────────

#[test]
fn integer_literal() {
    assert_eq!(eval("42"), 42);
}

#[test]
fn zero_literal() {
    assert_eq!(eval("0"), 0);
}

#[test]
fn hex_literal() {
    assert_eq!(eval("0xff"), 255);
}

#[test]
fn hex_literal_upper() {
    assert_eq!(eval("0xFF"), 255);
}

#[test]
fn true_literal() {
    assert_eq!(eval("true"), 1);
}

#[test]
fn false_literal() {
    assert_eq!(eval("false"), 0);
}

// ── Parser: arithmetic ───────────────────────────────────────────────────

#[test]
fn addition() {
    assert_eq!(eval("1 + 2"), 3);
}

#[test]
fn subtraction() {
    assert_eq!(eval("10 - 3"), 7);
}

#[test]
fn multiplication() {
    assert_eq!(eval("3 * 4"), 12);
}

#[test]
fn division() {
    assert_eq!(eval("10 / 2"), 5);
}

#[test]
fn remainder() {
    assert_eq!(eval("10 % 3"), 1);
}

#[test]
fn precedence_mul_before_add() {
    assert_eq!(eval("2 + 3 * 4"), 14);
}

#[test]
fn parentheses_override_precedence() {
    assert_eq!(eval("(2 + 3) * 4"), 20);
}

#[test]
fn unary_negation() {
    assert_eq!(eval("-5"), -5);
}

#[test]
fn double_negation() {
    assert_eq!(eval("--5"), 5);
}

// ── Parser: bitwise ──────────────────────────────────────────────────────

#[test]
fn bitwise_and() {
    assert_eq!(eval("0xff & 0x0f"), 0x0f);
}

#[test]
fn bitwise_or() {
    assert_eq!(eval("0x0f | 0xf0"), 0xff);
}

#[test]
fn bitwise_xor() {
    assert_eq!(eval("0xff ^ 0x0f"), 0xf0);
}

#[test]
fn shift_left() {
    assert_eq!(eval("1 << 4"), 16);
}

#[test]
fn shift_right() {
    assert_eq!(eval("0x10 >> 2"), 4);
}

// ── Parser: comparison ───────────────────────────────────────────────────

#[test]
fn eq_true() {
    assert_eq!(eval("5 == 5"), 1);
}
#[test]
fn eq_false() {
    assert_eq!(eval("5 == 6"), 0);
}
#[test]
fn ne_true() {
    assert_eq!(eval("5 != 6"), 1);
}
#[test]
fn lt_true() {
    assert_eq!(eval("3 < 5"), 1);
}
#[test]
fn lt_false() {
    assert_eq!(eval("5 < 3"), 0);
}
#[test]
fn le_eq() {
    assert_eq!(eval("5 <= 5"), 1);
}
#[test]
fn gt_true() {
    assert_eq!(eval("5 > 3"), 1);
}
#[test]
fn ge_eq() {
    assert_eq!(eval("5 >= 5"), 1);
}

// ── Parser: logical ──────────────────────────────────────────────────────

#[test]
fn logical_and_both_true() {
    assert_eq!(eval("1 && 1"), 1);
}
#[test]
fn logical_and_one_false() {
    assert_eq!(eval("1 && 0"), 0);
}
#[test]
fn logical_or_one_true() {
    assert_eq!(eval("0 || 1"), 1);
}
#[test]
fn logical_or_both_false() {
    assert_eq!(eval("0 || 0"), 0);
}
#[test]
fn logical_not_true() {
    assert_eq!(eval("!0"), 1);
}
#[test]
fn logical_not_false() {
    assert_eq!(eval("!1"), 0);
}

#[test]
fn logical_and_short_circuits() {
    // Right side would divide by zero; should never evaluate due to short-circuit.
    // We can't fully test short-circuit without a side-effect, but we can
    // confirm 0 && anything == 0 with a field ref that's not in scope.
    let c = ctx();
    // "0 && missing" — rhs not evaluated when lhs is 0
    let ast = parse("0 && missing").unwrap();
    assert_eq!(
        doe::schema::expr::eval(&ast, &c, "0 && missing").unwrap(),
        0
    );
}

#[test]
fn logical_or_short_circuits() {
    let c = ctx();
    let ast = parse("1 || missing").unwrap();
    assert_eq!(
        doe::schema::expr::eval(&ast, &c, "1 || missing").unwrap(),
        1
    );
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
    assert_eq!(
        doe::schema::expr::eval(&ast, &c, "_parent.outer_count").unwrap(),
        7
    );
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
    assert!(doe::schema::expr::eval(&ast, &c, "_parent._parent.x").is_err());
}

// ── Evaluation: division by zero ─────────────────────────────────────────

#[test]
fn division_by_zero_is_error() {
    assert!(eval_err("1 / 0"));
}

#[test]
fn modulo_by_zero_is_error() {
    assert!(eval_err("1 % 0"));
}

// ── Parser: error cases ───────────────────────────────────────────────────

#[test]
fn empty_expression_is_error() {
    assert!(parse_err(""));
}

#[test]
fn trailing_operator_is_error() {
    assert!(parse_err("1 +"));
}

#[test]
fn unmatched_paren_is_error() {
    assert!(parse_err("(1 + 2"));
}

#[test]
fn dangling_dot_is_error() {
    assert!(parse_err("foo."));
}

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
    assert_eq!(eval_str("x", &c).ok(), Some(42));
}

#[test]
fn push_frame_shadows_outer() {
    let mut c = ctx();
    c.bind("x", 1);
    c.push_frame();
    c.bind("x", 2);
    assert_eq!(eval_str("x", &c).ok(), Some(2));
}

#[test]
fn pop_frame_restores_outer() {
    let mut c = ctx();
    c.bind("x", 1);
    c.push_frame();
    c.bind("x", 2);
    c.pop_frame();
    assert_eq!(eval_str("x", &c).ok(), Some(1));
}

#[test]
fn pop_below_root_is_safe() {
    let mut c = ctx();
    c.pop_frame(); // should not panic
    c.pop_frame();
}

// ── Operators and display ─────────────────────────────────────────────────

#[test]
fn binary_op_display_matches_source_syntax() {
    let cases = [
        (BinaryOp::Add, "+"),
        (BinaryOp::Sub, "-"),
        (BinaryOp::Mul, "*"),
        (BinaryOp::Div, "/"),
        (BinaryOp::Rem, "%"),
        (BinaryOp::Shl, "<<"),
        (BinaryOp::Shr, ">>"),
        (BinaryOp::BitAnd, "&"),
        (BinaryOp::BitOr, "|"),
        (BinaryOp::BitXor, "^"),
        (BinaryOp::Eq, "=="),
        (BinaryOp::Ne, "!="),
        (BinaryOp::Lt, "<"),
        (BinaryOp::Gt, ">"),
        (BinaryOp::Le, "<="),
        (BinaryOp::Ge, ">="),
        (BinaryOp::And, "&&"),
        (BinaryOp::Or, "||"),
    ];
    for (op, text) in cases {
        assert_eq!(op.to_string(), text);
    }
}

#[test]
fn parser_produces_unary_nodes() {
    assert!(matches!(parse("-x").unwrap(), Expr::Unary(UnaryOp::Neg, _)));
    assert!(matches!(parse("!x").unwrap(), Expr::Unary(UnaryOp::Not, _)));
}

#[test]
fn unary_ops_evaluate() {
    let c = ctx();
    let neg = Expr::Unary(UnaryOp::Neg, Box::new(Expr::Int(5)));
    let not_zero = Expr::Unary(UnaryOp::Not, Box::new(Expr::Int(0)));
    let not_one = Expr::Unary(UnaryOp::Not, Box::new(Expr::Int(1)));
    assert_eq!(doe::schema::expr::eval(&neg, &c, "-5").unwrap(), -5);
    assert_eq!(doe::schema::expr::eval(&not_zero, &c, "!0").unwrap(), 1);
    assert_eq!(doe::schema::expr::eval(&not_one, &c, "!1").unwrap(), 0);
}

#[test]
fn bool_literal_evaluates_to_zero_or_one() {
    let c = ctx();
    assert_eq!(
        doe::schema::expr::eval(&Expr::Bool(true), &c, "true").unwrap(),
        1
    );
    assert_eq!(
        doe::schema::expr::eval(&Expr::Bool(false), &c, "false").unwrap(),
        0
    );
}

#[test]
fn trailing_tokens_after_expression_are_an_error() {
    assert!(matches!(
        parse("1 2").unwrap_err(),
        doe::DoeError::ExprParse { .. }
    ));
    assert!(parse_err("1 )"));
}

#[test]
fn lone_equals_and_unknown_characters_are_errors() {
    assert!(parse_err("1 = 1"));
    assert!(parse_err("1 @ 2"));
    assert!(parse_err("$"));
}

#[test]
fn eval_context_default_is_empty_root_scope() {
    let c = EvalContext::default();
    assert!(eval_str("x", &c).is_err());
    assert_eq!(eval_str("1 + 1", &c).unwrap(), 2);
}

#[test]
fn i64_min_arithmetic_wraps_instead_of_panicking() {
    let mut c = ctx();
    c.bind("m", i64::MIN);
    assert_eq!(eval_str("-m", &c).unwrap(), i64::MIN);
    assert_eq!(eval_str("m / -1", &c).unwrap(), i64::MIN);
    assert_eq!(eval_str("m % -1", &c).unwrap(), 0);
}

#[test]
fn shift_amount_uses_low_six_bits() {
    assert_eq!(eval("1 << 64"), 1);
    assert_eq!(eval("1 << 65"), 2);
    assert_eq!(eval("8 >> 67"), 1);
    assert_eq!(eval("1 << -63"), 2);
}

// ── Resource limits (regressions for stack overflows found by fuzzing) ────

#[test]
fn deeply_nested_parentheses_are_rejected_not_a_stack_overflow() {
    let text = format!("{}1{}", "(".repeat(100_000), ")".repeat(100_000));
    assert!(parse(&text).is_err());
    let text = format!(
        "{}1{}",
        "(".repeat(MAX_EXPR_DEPTH + 1),
        ")".repeat(MAX_EXPR_DEPTH + 1)
    );
    assert!(matches!(parse(&text), Err(doe::DoeError::ExprParse { .. })));
}

#[test]
fn long_unary_chains_are_rejected() {
    assert!(parse(&format!("{}1", "-".repeat(100_000))).is_err());
    assert!(parse(&format!("{}1", "!".repeat(MAX_EXPR_DEPTH + 1))).is_err());
}

#[test]
fn nesting_up_to_the_limit_is_accepted() {
    let n = MAX_EXPR_DEPTH;
    let text = format!("{}7{}", "(".repeat(n), ")".repeat(n));
    assert_eq!(eval(&text), 7);
    // An even number of negations cancels out.
    assert_eq!(eval(&format!("{}1", "-".repeat(n))), 1);
}

#[test]
fn very_long_expressions_are_rejected_not_a_stack_overflow() {
    let text = format!("1{}", "+1".repeat(100_000));
    assert!(parse(&text).is_err());
    let at_limit = format!("1{}", "+1".repeat((MAX_EXPR_LEN - 1) / 2));
    assert!(at_limit.len() <= MAX_EXPR_LEN);
    assert_eq!(
        eval(&at_limit),
        i64::try_from((MAX_EXPR_LEN - 1) / 2 + 1).unwrap()
    );
    assert!(parse(&format!("{at_limit}+1+1")).is_err());
}

#[test]
fn depth_resets_between_sibling_groups() {
    let group = format!(
        "{}1{}",
        "(".repeat(MAX_EXPR_DEPTH),
        ")".repeat(MAX_EXPR_DEPTH)
    );
    assert_eq!(eval(&format!("{group} + {group}")), 2);
}
