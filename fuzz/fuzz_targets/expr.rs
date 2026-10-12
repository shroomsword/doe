//! Parses arbitrary text as an expression and evaluates what parses, in a
//! context with a few bindings and a nested scope. Errors are fine; panics
//! and hangs are not.

#![no_main]

use doe::schema::expr::{EvalContext, eval, parse};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| {
    let Ok(ast) = parse(text) else {
        return;
    };
    let mut ctx = EvalContext::new();
    ctx.bind("a", i64::MIN);
    ctx.bind("b", -1);
    ctx.bind("c", 0);
    ctx.bind("x", 7);
    ctx.push_frame();
    ctx.bind("y", i64::MAX);
    drop(eval(&ast, &ctx, text));
    ctx.pop_frame();
    drop(eval(&ast, &ctx, text));
});
