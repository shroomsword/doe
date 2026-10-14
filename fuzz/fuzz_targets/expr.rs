//! Parses arbitrary text as an expression and evaluates what parses, in a
//! context with a few bindings and a nested scope. Errors are fine; panics
//! and hangs are not.

#![no_main]

use doe::schema::expr::{EvalContext, ExprValue, MAX_VALUE_LEN, eval, parse};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| {
    let Ok(ast) = parse(text) else {
        return;
    };
    let mut ctx = EvalContext::new();
    ctx.bind("a", i128::MIN);
    ctx.bind("b", -1);
    ctx.bind("c", 0);
    ctx.bind("x", 7);
    ctx.bind("f", 2.5);
    ctx.bind("s", "héllo");
    ctx.bind("raw", vec![1u8, 2, 3]);
    ctx.bind("list", vec![ExprValue::Int(4), ExprValue::Int(9)]);
    ctx.bind("big", "x".repeat(MAX_VALUE_LEN));
    ctx.push_frame();
    ctx.bind("y", i128::MAX);
    drop(eval(&ast, &ctx, text));
    ctx.pop_frame();
    drop(eval(&ast, &ctx, text));
});
