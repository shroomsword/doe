use doe::DoeError;
use doe::schema::expr::*;

fn ctx() -> EvalContext {
    EvalContext::new()
}

/// An integer or boolean result as an `i64`, which is what most of these
/// tests compare with.
fn as_i64(v: ExprValue) -> i64 {
    match v {
        ExprValue::Int(n) => i64::try_from(n).unwrap(),
        ExprValue::Bool(b) => i64::from(b),
        other => panic!("expected an integer, got {other:?}"),
    }
}

fn eval(s: &str) -> i64 {
    as_i64(eval_str(s, &ctx()).expect(s))
}

fn eval_with(bindings: &[(&str, i64)], s: &str) -> i64 {
    let mut c = ctx();
    for (k, v) in bindings {
        c.bind(k, *v);
    }
    as_i64(eval_str(s, &c).expect(s))
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
        ExprValue::Bool(false)
    );
}

#[test]
fn logical_or_short_circuits() {
    let c = ctx();
    let ast = parse("1 || missing").unwrap();
    assert_eq!(
        doe::schema::expr::eval(&ast, &c, "1 || missing").unwrap(),
        ExprValue::Bool(true)
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
        ExprValue::Int(7)
    );
}

#[test]
fn parent_field_in_arithmetic() {
    let mut c = ctx();
    c.bind("base", 100);
    c.push_frame();
    c.bind("offset", 5);
    assert_eq!(
        eval_str("_parent.base + offset", &c).unwrap(),
        ExprValue::Int(105)
    );
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
    assert_eq!(eval_str("x", &c).ok(), Some(ExprValue::Int(42)));
}

#[test]
fn push_frame_shadows_outer() {
    let mut c = ctx();
    c.bind("x", 1);
    c.push_frame();
    c.bind("x", 2);
    assert_eq!(eval_str("x", &c).ok(), Some(ExprValue::Int(2)));
}

#[test]
fn pop_frame_restores_outer() {
    let mut c = ctx();
    c.bind("x", 1);
    c.push_frame();
    c.bind("x", 2);
    c.pop_frame();
    assert_eq!(eval_str("x", &c).ok(), Some(ExprValue::Int(1)));
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
    assert_eq!(
        doe::schema::expr::eval(&neg, &c, "-5").unwrap(),
        ExprValue::Int(-5)
    );
    assert_eq!(
        doe::schema::expr::eval(&not_zero, &c, "!0").unwrap(),
        ExprValue::Bool(true)
    );
    assert_eq!(
        doe::schema::expr::eval(&not_one, &c, "!1").unwrap(),
        ExprValue::Bool(false)
    );
}

#[test]
fn bool_literals_evaluate_to_booleans() {
    let c = ctx();
    assert_eq!(
        doe::schema::expr::eval(&Expr::Bool(true), &c, "true").unwrap(),
        ExprValue::Bool(true)
    );
    assert_eq!(
        doe::schema::expr::eval(&Expr::Bool(false), &c, "false").unwrap(),
        ExprValue::Bool(false)
    );
    assert_eq!(parse("true").unwrap(), Expr::Bool(true));
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
    assert_eq!(eval_str("1 + 1", &c).unwrap(), ExprValue::Int(2));
}

#[test]
fn i64_min_arithmetic_is_exact() {
    let mut c = ctx();
    c.bind("m", i64::MIN);
    let max_plus_one = ExprValue::Int(i128::from(i64::MAX) + 1);
    assert_eq!(eval_str("-m", &c).unwrap(), max_plus_one);
    assert_eq!(eval_str("m / -1", &c).unwrap(), max_plus_one);
    assert_eq!(eval_str("m % -1", &c).unwrap(), ExprValue::Int(0));
}

#[test]
fn shifts_are_exact_and_out_of_range_amounts_are_errors() {
    assert_eq!(eval("1 << 40"), 1 << 40);
    assert_eq!(eval("1024 >> 3"), 128);
    assert_eq!(eval("-8 >> 1"), -4);
    assert_eq!(
        eval_str("1 << 100", &ctx()).unwrap(),
        ExprValue::Int(1 << 100)
    );
    for src in ["1 << 127", "1 << 128", "1 << -1", "1 >> -1", "1 >> 200"] {
        assert!(eval_err(src), "{src}");
    }
}

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
    // The operators of a chain are limited, not the length.
    let at_limit = format!("1{}", "+1".repeat(MAX_EXPR_LINKS));
    assert_eq!(eval(&at_limit), i64::try_from(MAX_EXPR_LINKS + 1).unwrap());
    assert!(parse(&format!("{at_limit}+1")).is_err());
    // Suffixes count too.
    let suffixes = format!("a{}", ".reverse".repeat(MAX_EXPR_LINKS + 1));
    assert!(parse(&suffixes).is_err());
    // A long expression without a chain is fine, up to the length limit.
    let table = format!("[{}0]", "0,".repeat(MAX_EXPR_LEN / 4));
    assert!(parse(&table).is_ok());
    let too_long = format!("[{}0]", "0,".repeat(MAX_EXPR_LEN));
    assert!(parse(&too_long).is_err());
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

#[test]
fn integer_literals_that_do_not_fit_are_parse_errors() {
    for src in [
        "0x80000000000000000000000000000000",
        "170141183460469231731687303715884105728",
        "99999999999999999999999999999999999999999",
        "0x",
        "1 + 0x",
        "x == 99999999999999999999999999999999999999999",
    ] {
        let err = parse(src).unwrap_err();
        assert!(
            matches!(&err, DoeError::ExprParse { message, .. } if message.contains("does not fit")),
            "{src}: {err:?}"
        );
    }
}

#[test]
fn integer_literals_hold_every_u64_and_up_to_i128_max() {
    let int = |src| eval_str(src, &ctx()).unwrap();
    assert_eq!(
        int("0xffffffffffffffff"),
        ExprValue::Int(i128::from(u64::MAX))
    );
    assert_eq!(
        int("18446744073709551615"),
        ExprValue::Int(i128::from(u64::MAX))
    );
    assert_eq!(int("9223372036854775808"), ExprValue::Int(1 << 63));
    assert_eq!(
        int("0x7fffffffffffffffffffffffffffffff"),
        ExprValue::Int(i128::MAX)
    );
}

// ── Typed values ─────────────────────────────────────────────────────────

fn value(src: &str) -> ExprValue {
    eval_str(src, &ctx()).expect(src)
}

fn text(s: &str) -> ExprValue {
    ExprValue::Str(s.to_owned())
}

#[test]
fn float_literals_and_arithmetic() {
    assert_eq!(value("1.5"), ExprValue::Float(1.5));
    assert_eq!(value("1.5 + 1"), ExprValue::Float(2.5));
    assert_eq!(value("3 * 0.5"), ExprValue::Float(1.5));
    assert_eq!(value("1 / 4.0"), ExprValue::Float(0.25));
    assert_eq!(value("-2.5"), ExprValue::Float(-2.5));
    assert_eq!(value("1.5 < 2"), ExprValue::Bool(true));
    assert_eq!(value("2 == 2.0"), ExprValue::Bool(true));
    // A dot without a digit after it is a member access, not a fraction.
    assert_eq!(value("10.to_s"), text("10"));
}

#[test]
fn float_errors() {
    assert!(eval_err("1.5 % 2"));
    assert!(eval_err("1.5 & 1"));
    assert!(eval_err("1.5 << 1"));
}

#[test]
fn nan_compares_false_except_not_equal() {
    let mut c = ctx();
    c.bind("nan", f64::NAN);
    let b = |src| eval_str(src, &c).unwrap();
    assert_eq!(b("nan == nan"), ExprValue::Bool(false));
    assert_eq!(b("nan != nan"), ExprValue::Bool(true));
    assert_eq!(b("nan < 1"), ExprValue::Bool(false));
    assert_eq!(b("nan >= 1"), ExprValue::Bool(false));
}

#[test]
fn string_literals_and_escapes() {
    assert_eq!(value("'abc'"), text("abc"));
    assert_eq!(value("\"abc\""), text("abc"));
    assert_eq!(value("'a\\nb'"), text("a\\nb"));
    assert_eq!(value("\"a\\nb\""), text("a\nb"));
    assert_eq!(value("\"\\t\\r\\0\\\\\\\"\\'\""), text("\t\r\0\\\"'"));
    assert_eq!(value("\"\\x41\\u00e9\""), text("A\u{e9}"));
    assert_eq!(value("'héllo'"), text("héllo"));
    assert_eq!(value("''"), text(""));
}

#[test]
fn bad_string_literals_are_parse_errors() {
    for src in [
        "'abc",
        "\"abc",
        "\"abc\\",
        "\"\\q\"",
        "\"\\x4\"",
        "\"\\ud800\"",
        "\"\\u12\"",
    ] {
        assert!(parse_err(src), "{src}");
    }
}

#[test]
fn strings_concatenate_and_compare() {
    assert_eq!(value("'ab' + 'cd'"), text("abcd"));
    assert_eq!(value("'ab' == 'ab'"), ExprValue::Bool(true));
    assert_eq!(value("'ab' != 'ab'"), ExprValue::Bool(false));
    assert_eq!(value("'ab' < 'b'"), ExprValue::Bool(true));
    assert_eq!(value("'b' >= 'ab'"), ExprValue::Bool(true));
    assert!(eval_err("'a' - 'b'"));
    assert!(eval_err("'a' + 1"));
    assert!(eval_err("'a' == 1"));
}

#[test]
fn string_concatenation_is_bounded() {
    // `s + s` doubles a string, so without a limit a short expression over a
    // large field could ask for as much memory as it likes.
    let mut c = ctx();
    c.bind("big", "x".repeat(MAX_VALUE_LEN));
    assert!(eval_str("big + 'y'", &c).is_err());
    assert!(eval_str("big + big", &c).is_err());
    assert!(eval_str("big + ''", &c).is_ok());
    c.bind("half", "x".repeat(MAX_VALUE_LEN / 2));
    assert!(eval_str("half + half", &c).is_ok());
    assert!(eval_str("half + half + 'y'", &c).is_err());
}

#[test]
fn booleans() {
    assert_eq!(value("true"), ExprValue::Bool(true));
    assert_eq!(value("true && false"), ExprValue::Bool(false));
    assert_eq!(value("true || false"), ExprValue::Bool(true));
    assert_eq!(value("!true"), ExprValue::Bool(false));
    assert_eq!(value("true == true"), ExprValue::Bool(true));
    assert_eq!(value("true != false"), ExprValue::Bool(true));
    // Integers are conditions too: non-zero is true.
    assert_eq!(value("3 && 4"), ExprValue::Bool(true));
    assert_eq!(value("0 || 0"), ExprValue::Bool(false));
    assert!(eval_err("true + 1"));
    assert!(eval_err("true == 1"));
    assert!(eval_err("'a' && true"));
    assert!(eval_err("!'a'"));
}

#[test]
fn logical_operators_evaluate_the_right_side_only_when_needed() {
    assert_eq!(value("true || missing"), ExprValue::Bool(true));
    assert_eq!(value("false && missing"), ExprValue::Bool(false));
    assert!(eval_err("true && missing"));
    assert!(eval_err("false || missing"));
    assert_eq!(value("false && (1 / 0 == 0)"), ExprValue::Bool(false));
}

#[test]
fn ternary() {
    assert_eq!(value("true ? 1 : 2"), ExprValue::Int(1));
    assert_eq!(value("false ? 1 : 2"), ExprValue::Int(2));
    assert_eq!(value("1 < 2 ? 'yes' : 'no'"), text("yes"));
    assert_eq!(value("0 ? 1 : 2"), ExprValue::Int(2));
    // Right associative, and only the chosen branch is evaluated.
    assert_eq!(value("false ? 1 : false ? 2 : 3"), ExprValue::Int(3));
    assert_eq!(value("true ? 1 : missing"), ExprValue::Int(1));
    assert_eq!(value("false ? missing : 2"), ExprValue::Int(2));
    assert_eq!(value("(true ? 1 : 2) + 10"), ExprValue::Int(11));
    assert!(eval_err("'a' ? 1 : 2"));
    assert!(parse_err("true ? 1"));
    assert!(parse_err("true ? 1 :"));
    assert!(parse_err("? 1 : 2"));
}

#[test]
fn division_and_remainder_round_towards_negative_infinity() {
    for (src, expected) in [
        ("7 / 2", 3),
        ("-7 / 2", -4),
        ("7 / -2", -4),
        ("-7 / -2", 3),
        ("6 / -2", -3),
        ("7 % 3", 1),
        ("-7 % 3", 2),
        ("7 % -3", -2),
        ("-7 % -3", -1),
        ("-6 % 3", 0),
    ] {
        assert_eq!(value(src), ExprValue::Int(expected), "{src}");
    }
}

#[test]
fn integer_overflow_is_an_error() {
    let mut c = ctx();
    c.bind("max", i128::MAX);
    c.bind("min", i128::MIN);
    for src in [
        "max + 1",
        "min - 1",
        "max * 2",
        "-min",
        "min / -1",
        "min % -1",
        "1 << 126 << 1",
    ] {
        assert!(eval_str(src, &c).is_err(), "{src}");
    }
    assert_eq!(
        eval_str("max - 1 + 1", &c).unwrap(),
        ExprValue::Int(i128::MAX)
    );
}

#[test]
fn bitwise_complement() {
    assert_eq!(value("~0"), ExprValue::Int(-1));
    assert_eq!(value("~5 & 0xff"), ExprValue::Int(250));
    assert!(eval_err("~1.5"));
    assert!(eval_err("~'a'"));
}

#[test]
fn array_literals_and_indexing() {
    assert_eq!(
        value("[1, 'a', true]"),
        ExprValue::Array(vec![ExprValue::Int(1), text("a"), ExprValue::Bool(true)])
    );
    assert_eq!(value("[]"), ExprValue::Array(vec![]));
    assert_eq!(value("[10, 20, 30][1]"), ExprValue::Int(20));
    assert_eq!(value("[[1, 2], [3]][0][1]"), ExprValue::Int(2));
    assert_eq!(value("[1, 2, 3].size"), ExprValue::Int(3));
    assert!(eval_err("[1, 2][2]"));
    assert!(eval_err("[1, 2][-1]"));
    assert!(eval_err("[1, 2]['a']"));
    assert!(eval_err("5[0]"));
    assert!(parse_err("[1, 2"));
    assert!(parse_err("[1 2]"));
    assert!(parse_err("[1, 2][0"));
}

#[test]
fn array_members() {
    assert_eq!(value("[3, 9, 1].first"), ExprValue::Int(3));
    assert_eq!(value("[3, 9, 1].last"), ExprValue::Int(1));
    assert_eq!(value("[3, 9, 1].min"), ExprValue::Int(1));
    assert_eq!(value("[3, 9, 1].max"), ExprValue::Int(9));
    assert_eq!(value("['b', 'a'].min"), text("a"));
    assert_eq!(value("[1, 2, 3].length"), ExprValue::Int(3));
    for src in ["[].first", "[].last", "[].min", "[].max", "[1, 'a'].max"] {
        assert!(eval_err(src), "{src}");
    }
}

#[test]
fn string_members_and_methods() {
    assert_eq!(value("'héllo'.length"), ExprValue::Int(5));
    assert_eq!(value("'abc'.size"), ExprValue::Int(3));
    assert_eq!(value("'abc'.reverse"), text("cba"));
    assert_eq!(value("'abcdef'.substring(1, 4)"), text("bcd"));
    assert_eq!(value("'abc'.substring(0, 0)"), text(""));
    assert_eq!(value("'héllo'.substring(1, 3)"), text("él"));
    assert_eq!(value("'123'.to_i"), ExprValue::Int(123));
    assert_eq!(value("'-123'.to_i"), ExprValue::Int(-123));
    assert_eq!(value("'ff'.to_i(16)"), ExprValue::Int(255));
    assert_eq!(value("'101'.to_i(2)"), ExprValue::Int(5));
    assert_eq!(value("'abc'.to_s"), text("abc"));
    for src in [
        "'abc'.substring(2, 1)",
        "'abc'.substring(0, 4)",
        "'abc'.substring(-1, 2)",
        "'abc'.substring(1)",
        "'abc'.substring('a', 2)",
        "'abc'.to_i",
        "''.to_i",
        "'12'.to_i(1)",
        "'12'.to_i(37)",
        "'12'.to_i(2)",
        "'abc'.nope",
        "'abc'.length(1)",
    ] {
        assert!(eval_err(src), "{src}");
    }
}

#[test]
fn conversions() {
    assert_eq!(value("123.to_s"), text("123"));
    assert_eq!(value("(0 - 5).to_s"), text("-5"));
    assert_eq!(value("1.5.to_s"), text("1.5"));
    assert_eq!(value("2.9.to_i"), ExprValue::Int(2));
    assert_eq!(value("(0 - 2.9).to_i"), ExprValue::Int(-2));
    assert_eq!(value("true.to_i"), ExprValue::Int(1));
    assert_eq!(value("false.to_i"), ExprValue::Int(0));
    assert_eq!(value("true.to_s"), text("true"));
    let mut c = ctx();
    c.bind("inf", f64::INFINITY);
    c.bind("huge", 1e300);
    c.bind("nan", f64::NAN);
    for src in ["inf.to_i", "huge.to_i", "nan.to_i"] {
        assert!(eval_str(src, &c).is_err(), "{src}");
    }
}

#[test]
fn bytes_members_and_indexing() {
    let mut c = ctx();
    c.bind("raw", vec![5u8, 7, 6]);
    let v = |src| eval_str(src, &c).unwrap();
    assert_eq!(v("raw.size"), ExprValue::Int(3));
    assert_eq!(v("raw.length"), ExprValue::Int(3));
    assert_eq!(v("raw[1]"), ExprValue::Int(7));
    assert_eq!(v("raw.first"), ExprValue::Int(5));
    assert_eq!(v("raw.last"), ExprValue::Int(6));
    assert_eq!(v("raw.min"), ExprValue::Int(5));
    assert_eq!(v("raw.max"), ExprValue::Int(7));
    assert_eq!(v("raw == raw"), ExprValue::Bool(true));
    assert!(eval_str("raw[3]", &c).is_err());
    assert!(eval_str("raw + raw", &c).is_err());
}

#[test]
fn members_of_a_bound_field_use_the_longest_bound_prefix() {
    let mut c = ctx();
    c.bind("header", "outer");
    c.bind("header.name", "inner");
    assert_eq!(eval_str("header.name", &c).unwrap(), text("inner"));
    assert_eq!(
        eval_str("header.name.length", &c).unwrap(),
        ExprValue::Int(5)
    );
    assert_eq!(eval_str("header.length", &c).unwrap(), ExprValue::Int(5));
    assert!(eval_str("header.nope.length", &c).is_err());
    c.push_frame();
    c.bind("x", "abc");
    assert_eq!(
        eval_str("_parent.header.length", &c).unwrap(),
        ExprValue::Int(5)
    );
    assert_eq!(eval_str("x.length", &c).unwrap(), ExprValue::Int(3));
}

#[test]
fn members_and_calls_on_expressions() {
    assert_eq!(value("('a' + 'bc').length"), ExprValue::Int(3));
    assert_eq!(value("'abc'.substring(1, 3).length"), ExprValue::Int(2));
    assert_eq!(value("'abc'.reverse.reverse"), text("abc"));
    assert_eq!(value("[1, 2][1].to_s"), text("2"));
    assert_eq!(value("(1 + 2).to_s + 'x'"), text("3x"));
    assert!(parse_err("'abc'."));
    assert!(parse_err("'abc'.1"));
    assert!(parse_err("'abc'.substring(1,"));
    assert!(parse_err("'abc'.substring(1 2)"));
    // A call needs a receiver.
    assert!(parse_err("substring(1, 2)"));
}

#[test]
fn eval_bool_and_eval_int() {
    let c = ctx();
    let parsed = |src| parse(src).unwrap();
    assert!(eval_bool(&parsed("1 < 2"), &c, "").unwrap());
    assert!(eval_bool(&parsed("5"), &c, "").unwrap());
    assert!(!eval_bool(&parsed("0"), &c, "").unwrap());
    assert!(eval_bool(&parsed("'a'"), &c, "").is_err());
    assert_eq!(eval_int(&parsed("2 + 3"), &c, "").unwrap(), 5);
    assert!(eval_int(&parsed("true"), &c, "").is_err());
    assert!(eval_int(&parsed("1.5"), &c, "").is_err());
    assert!(eval_int(&parsed("'a'"), &c, "").is_err());
}

#[test]
fn expression_values_convert_from_rust_values_and_display() {
    assert_eq!(ExprValue::from(7u8), ExprValue::Int(7));
    assert_eq!(ExprValue::from(-7i32), ExprValue::Int(-7));
    assert_eq!(
        ExprValue::from(u64::MAX),
        ExprValue::Int(i128::from(u64::MAX))
    );
    assert_eq!(ExprValue::from(true), ExprValue::Bool(true));
    assert_eq!(ExprValue::from("s"), text("s"));
    assert_eq!(ExprValue::from(vec![1u8]), ExprValue::Bytes(vec![1]));
    assert_eq!(
        ExprValue::from(vec![ExprValue::Int(1)]),
        ExprValue::Array(vec![ExprValue::Int(1)])
    );
    assert_eq!(ExprValue::Int(1).as_int(), Some(1));
    assert_eq!(ExprValue::Bool(true).as_int(), None);
    assert_eq!(ExprValue::Int(42).to_string(), "42");
    assert_eq!(ExprValue::Float(1.5).to_string(), "1.5");
    assert_eq!(ExprValue::Bool(true).to_string(), "true");
    assert_eq!(text("a").to_string(), "\"a\"");
    assert_eq!(ExprValue::Bytes(vec![1, 2]).to_string(), "<2 bytes>");
    assert_eq!(ExprValue::Array(vec![]).to_string(), "[0 items]");
    for (v, name) in [
        (ExprValue::Int(0), "integer"),
        (ExprValue::Float(0.0), "float"),
        (ExprValue::Bool(true), "boolean"),
        (text(""), "string"),
        (ExprValue::Bytes(vec![]), "bytes"),
        (ExprValue::Array(vec![]), "array"),
    ] {
        assert_eq!(v.type_name(), name);
    }
}

#[test]
fn unbind_prefix_removes_a_value_and_its_fields() {
    let mut c = ctx();
    c.bind("_", 1);
    c.bind("_.a", 2);
    c.bind("_x", 3);
    c.unbind_prefix("_");
    assert!(eval_str("_", &c).is_err());
    assert!(eval_str("_.a", &c).is_err());
    assert_eq!(eval_str("_x", &c).unwrap(), ExprValue::Int(3));
}

#[test]
fn deep_but_legal_expressions_do_not_depend_on_the_stack_size() {
    // Every operator level adds a level to the tree; evaluating it must not
    // recurse once per level.
    let src = format!("1{}", "+1".repeat(MAX_EXPR_LINKS));
    let handle = std::thread::Builder::new()
        .stack_size(128 * 1024)
        .spawn(move || eval_str(&src, &EvalContext::new()).unwrap())
        .unwrap();
    assert_eq!(handle.join().unwrap(), ExprValue::Int(513));
}

// ── Enum literals, _root, _io, structs ───────────────────────────────────

#[test]
fn enum_members_parse_as_paths() {
    assert_eq!(
        parse("kind_t::data").unwrap(),
        Expr::Enum(vec!["kind_t".into(), "data".into()])
    );
    assert_eq!(
        parse("a::b::c").unwrap(),
        Expr::Enum(vec!["a".into(), "b".into(), "c".into()])
    );
    assert!(matches!(
        parse("kind == kind_t::data").unwrap(),
        Expr::Binary(BinaryOp::Eq, ..)
    ));
    // A single colon is still the conditional.
    assert!(matches!(parse("x ? a : b").unwrap(), Expr::Ternary(..)));
    assert!(matches!(
        parse("x ? kind_t::data : kind_t::other").unwrap(),
        Expr::Ternary(..)
    ));
    for bad in ["a::", "::a", "a::1", "a:: ::b", "a::b::"] {
        assert!(parse_err(bad), "{bad}");
    }
}

#[test]
fn an_enum_member_that_was_not_resolved_is_an_error_when_evaluated() {
    let err = eval_str("kind_t::data", &ctx()).unwrap_err();
    assert!(err.to_string().contains("not resolved"), "{err}");
}

#[test]
fn root_reads_the_outermost_scope() {
    let mut c = ctx();
    c.bind("count", 3);
    c.bind(
        "header",
        ExprValue::Struct(vec![("n".into(), ExprValue::Int(9))]),
    );
    c.push_frame();
    c.bind("count", 100);
    c.push_frame();
    c.bind("count", 200);
    let v = |src| eval_str(src, &c).unwrap();
    assert_eq!(v("count"), ExprValue::Int(200));
    assert_eq!(v("_root.count"), ExprValue::Int(3));
    assert_eq!(v("_root.header.n"), ExprValue::Int(9));
    assert_eq!(v("_parent.count"), ExprValue::Int(100));
    assert_eq!(v("_parent._parent.count"), ExprValue::Int(3));
    assert!(eval_str("_root", &c).is_err());
    assert!(eval_str("_root.nope", &c).is_err());
}

#[test]
fn io_reports_the_stream_the_expression_is_in() {
    let mut c = ctx();
    assert!(eval_str("_io.pos", &c).is_err(), "_io needs a parse");
    c.set_io(3, 10, false);
    let v = |src| eval_str(src, &c).unwrap();
    assert_eq!(v("_io.pos"), ExprValue::Int(3));
    assert_eq!(v("_io.size"), ExprValue::Int(10));
    assert_eq!(v("_io.eof"), ExprValue::Bool(false));
    assert_eq!(v("_io.size - _io.pos"), ExprValue::Int(7));
    assert_eq!(v("_io.eof ? 1 : 2"), ExprValue::Int(2));
    assert!(eval_str("_io.nope", &c).is_err());
    c.set_io(10, 10, true);
    assert_eq!(eval_str("_io.eof", &c).unwrap(), ExprValue::Bool(true));
}

#[test]
fn struct_fields_come_before_methods() {
    let mut c = ctx();
    c.bind(
        "rec",
        ExprValue::Struct(vec![
            ("size".into(), ExprValue::Int(42)),
            ("name".into(), ExprValue::Str("abc".into())),
        ]),
    );
    let v = |src| eval_str(src, &c).unwrap();
    assert_eq!(v("rec.size"), ExprValue::Int(42));
    assert_eq!(v("rec.name.length"), ExprValue::Int(3));
    assert!(eval_str("rec.nope", &c).is_err());
    assert!(eval_str("rec.length", &c).is_err());
    assert!(eval_str("rec + 1", &c).is_err());
}

#[test]
fn arrays_of_structs_are_indexed_and_navigated() {
    let entry = |n: i128| ExprValue::Struct(vec![("n".into(), ExprValue::Int(n))]);
    let mut c = ctx();
    c.bind(
        "entries",
        ExprValue::Array(vec![entry(5), entry(6), entry(7)]),
    );
    let v = |src| eval_str(src, &c).unwrap();
    assert_eq!(v("entries[0].n"), ExprValue::Int(5));
    assert_eq!(v("entries[2].n + entries[1].n"), ExprValue::Int(13));
    assert_eq!(v("entries[entries.size - 1].n"), ExprValue::Int(7));
    assert_eq!(v("entries.first.n"), ExprValue::Int(5));
    assert_eq!(v("entries.last.n"), ExprValue::Int(7));
    assert_eq!(v("entries.size"), ExprValue::Int(3));
    assert!(eval_str("entries[3].n", &c).is_err());
    assert!(eval_str("entries[-1].n", &c).is_err());
    assert!(eval_str("entries.min", &c).is_err());
}

#[test]
fn an_omitted_value_knows_only_its_size() {
    let mut c = ctx();
    c.bind("big", ExprValue::Omitted { len: 5_000_000 });
    let v = |src| eval_str(src, &c);
    assert_eq!(v("big.size").unwrap(), ExprValue::Int(5_000_000));
    assert_eq!(v("big.length").unwrap(), ExprValue::Int(5_000_000));
    assert_eq!(v("big.size > 1000").unwrap(), ExprValue::Bool(true));
    for src in [
        "big[0]",
        "big.first",
        "big.to_s",
        "big == big",
        "big + 1",
        "!big",
    ] {
        assert!(v(src).is_err(), "{src}");
    }
    let err = v("big[0]").unwrap_err().to_string();
    assert!(err.contains("too large"), "{err}");
}

#[test]
fn struct_and_omitted_values_display_and_name_their_type() {
    let st = ExprValue::Struct(vec![("a".into(), ExprValue::Int(1))]);
    assert_eq!(st.type_name(), "struct");
    assert_eq!(st.to_string(), "<struct with 1 fields>");
    let om = ExprValue::Omitted { len: 9 };
    assert_eq!(om.type_name(), "omitted value");
    assert!(om.to_string().contains('9'));
}

#[test]
fn kaitai_word_operators_are_the_logical_operators() {
    let c = ctx();
    let v = |src| eval_str(src, &c).unwrap();
    assert_eq!(v("true and false"), ExprValue::Bool(false));
    assert_eq!(v("true and true"), ExprValue::Bool(true));
    assert_eq!(v("false or true"), ExprValue::Bool(true));
    assert_eq!(v("not true"), ExprValue::Bool(false));
    assert_eq!(v("not (1 > 2) and 2 > 1"), ExprValue::Bool(true));
    assert_eq!(v("1 > 2 or 3 > 2 and not false"), ExprValue::Bool(true));
    // They short-circuit like the symbols, and the symbols still work.
    assert_eq!(v("true or missing"), ExprValue::Bool(true));
    assert_eq!(v("false and missing"), ExprValue::Bool(false));
    assert_eq!(v("true && !false"), ExprValue::Bool(true));
    // Only whole words are operators.
    let mut c = ctx();
    c.bind("order", 5);
    c.bind("android", 6);
    c.bind("notes", 7);
    assert_eq!(
        eval_str("order + android + notes", &c).unwrap(),
        ExprValue::Int(18)
    );
}

// ── Literals ─────────────────────────────────────────────────────────────

#[test]
fn binary_octal_and_separated_literals() {
    assert_eq!(eval("0b1000_0000"), 128);
    assert_eq!(eval("0B101"), 5);
    assert_eq!(eval("0o0170000"), 0o017_0000);
    assert_eq!(eval("0xffff_ffff"), 0xffff_ffff);
    assert_eq!(eval("1_000"), 1000);
    assert_eq!(eval("0x1F & 0b1111_0000"), 0x10);
    for bad in ["0b", "0b2", "0o8", "0x", "0b1a"] {
        assert!(parse_err(bad), "{bad}");
    }
}

#[test]
fn a_float_may_end_in_a_point_and_a_number_may_have_members() {
    assert_eq!(
        eval_str("24000. * 2", &EvalContext::new()).unwrap(),
        ExprValue::Float(48000.0)
    );
    assert_eq!(
        eval_str("1.to_s", &EvalContext::new()).unwrap(),
        ExprValue::Str("1".to_owned())
    );
    assert_eq!(
        eval_str("1.5", &EvalContext::new()).unwrap(),
        ExprValue::Float(1.5)
    );
}

#[test]
fn unary_plus_is_a_no_op() {
    assert_eq!(eval("+3"), 3);
    assert_eq!(eval("(1 < 0 ? -1 : +1) * -4"), -4);
    assert_eq!(eval("2 - +1"), 1);
}

#[test]
fn a_list_may_end_in_a_comma() {
    assert_eq!(eval("[1, 2, 3, ].size"), 3);
    assert_eq!(eval("[[1, 2,], [3,],].size"), 2);
    for bad in ["[,]", "[1,,2]", "[1 2]"] {
        assert!(parse_err(bad), "{bad}");
    }
}

#[test]
fn as_and_sizeof_parse() {
    assert_eq!(
        parse("a.b.as<c>.d").unwrap(),
        Expr::Field(vec!["a".into(), "b".into(), "d".into()])
    );
    assert!(parse("a.as<c::d>.e").is_ok());
    assert_eq!(
        parse("sizeof<s4>").unwrap(),
        Expr::Field(vec!["sizeof<s4>".into()])
    );
    assert!(parse("bitsizeof<a::b> * 2").is_ok());
    for bad in [
        "a.as<>", "a.as<1>", "a.as<b", "sizeof<>", "sizeof<a", "a.as < b",
    ] {
        assert!(parse_err(bad), "{bad}");
    }
}
