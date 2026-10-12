use doe::Value;
use doe::parser::context::*;

#[test]
fn new_context_is_empty() {
    let ctx = ParseContext::new();
    assert_eq!(ctx.current_fields(), []);
    assert!(ctx.lookup("x").is_none());
}

#[test]
fn bind_uint_is_accessible() {
    let mut ctx = ParseContext::new();
    ctx.bind("count", Value::UInt(5));
    assert_eq!(ctx.lookup("count"), Some(&Value::UInt(5)));
}

#[test]
fn bind_uint_registers_in_eval_ctx() {
    let mut ctx = ParseContext::new();
    ctx.bind("n", Value::UInt(42));
    // Expression evaluator should see it
    let result = doe::schema::expr::eval_str("n + 1", &ctx.eval_ctx).unwrap();
    assert_eq!(result, 43);
}

#[test]
fn bind_sint_registers_in_eval_ctx() {
    let mut ctx = ParseContext::new();
    ctx.bind("offset", Value::SInt(-8));
    let result = doe::schema::expr::eval_str("offset + 10", &ctx.eval_ctx).unwrap();
    assert_eq!(result, 2);
}

#[test]
fn bind_bytes_does_not_register_in_eval_ctx() {
    let mut ctx = ParseContext::new();
    ctx.bind("data", Value::Bytes(vec![1, 2, 3]));
    // Bytes have no numeric value, so eval should fail
    assert!(doe::schema::expr::eval_str("data", &ctx.eval_ctx).is_err());
}

#[test]
fn bind_enum_registers_discriminant() {
    let mut ctx = ParseContext::new();
    ctx.bind(
        "kind",
        Value::Enum {
            value: 2,
            name: Some("data".into()),
        },
    );
    let result = doe::schema::expr::eval_str("kind", &ctx.eval_ctx).unwrap();
    assert_eq!(result, 2);
}

#[test]
fn current_fields_returns_all_bound() {
    let mut ctx = ParseContext::new();
    ctx.bind("a", Value::UInt(1));
    ctx.bind("b", Value::UInt(2));
    let fields = ctx.current_fields();
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].0, "a");
    assert_eq!(fields[1].0, "b");
}

#[test]
fn push_pop_scope_isolates_bindings() {
    let mut ctx = ParseContext::new();
    ctx.bind("outer", Value::UInt(10));

    ctx.push_scope();
    ctx.bind("inner", Value::UInt(20));

    // Inner scope sees its own field
    assert!(ctx.lookup("inner").is_some());

    // Pop returns the inner fields
    let fields = ctx.pop_scope();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].0, "inner");

    // After pop, outer is accessible again, inner is not
    assert!(ctx.lookup("outer").is_some());
    assert!(ctx.lookup("inner").is_none());
}

#[test]
fn parent_field_accessible_in_eval_after_push() {
    let mut ctx = ParseContext::new();
    ctx.bind("count", Value::UInt(7));

    ctx.push_scope();
    ctx.bind("offset", Value::UInt(3));

    // _parent.count should be accessible through the eval context
    let result = doe::schema::expr::eval_str("_parent.count + offset", &ctx.eval_ctx);
    assert_eq!(result.unwrap(), 10);

    ctx.pop_scope();
}

#[test]
fn lookup_returns_most_recent_binding() {
    // If somehow a field is bound twice (shouldn't happen in normal use
    // but worth checking), the most recent wins.
    let mut ctx = ParseContext::new();
    ctx.bind("x", Value::UInt(1));
    ctx.bind("x", Value::UInt(2));
    assert_eq!(ctx.lookup("x"), Some(&Value::UInt(2)));
}

#[test]
fn multiple_nested_scopes() {
    let mut ctx = ParseContext::new();
    ctx.bind("a", Value::UInt(1));
    ctx.push_scope();
    ctx.bind("b", Value::UInt(2));
    ctx.push_scope();
    ctx.bind("c", Value::UInt(3));

    // Innermost scope
    assert!(ctx.lookup("c").is_some());
    assert!(ctx.lookup("b").is_none()); // not in current scope
    assert!(ctx.lookup("a").is_none());

    let inner = ctx.pop_scope();
    assert_eq!(inner.len(), 1);

    let mid = ctx.pop_scope();
    assert_eq!(mid.len(), 1);

    assert!(ctx.lookup("a").is_some());
}

#[test]
fn default_matches_new() {
    let mut a = ParseContext::default();
    a.bind("x", Value::UInt(1));
    assert_eq!(a.lookup("x"), Some(&Value::UInt(1)));
    assert_eq!(a.current_fields().len(), 1);
}
