//! Crafted input that nests or repeats without bound is an error, not a stack
//! overflow or an endless loop.
//!
//! These tests need no files, and run on the main thread with the stack it
//! has, so they also run in a WebAssembly runtime (see `scripts/wasm-test`),
//! where the stack is 1 MiB and an overflow cannot be caught.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "Panicking is permitted in tests (rust-practices)."
)]

use std::fmt::Write as _;

use doe::parser::cursor::Cursor;
use doe::parser::engine::{Engine, MAX_TYPE_DEPTH};
use doe::schema::compiler::compile_schema;
use doe::schema::expr::{
    EvalContext, ExprValue, MAX_EXPR_DEPTH, MAX_EXPR_LEN, MAX_EXPR_LINKS, eval_str, parse,
};
use doe::schema::raw::RawSchema;

#[test]
fn deeply_nested_expressions_are_errors() {
    assert!(parse(&format!("{}1{}", "(".repeat(100_000), ")".repeat(100_000))).is_err());
    let over = MAX_EXPR_DEPTH + 1;
    assert!(parse(&format!("{}1{}", "(".repeat(over), ")".repeat(over))).is_err());
    assert!(parse(&format!("{}1", "-".repeat(100_000))).is_err());
    assert!(parse(&format!("{}1", "!".repeat(over))).is_err());
}

#[test]
fn the_deepest_legal_expressions_work_on_this_stack() {
    let n = MAX_EXPR_DEPTH;
    let nested = format!("{}7{}", "(".repeat(n), ")".repeat(n));
    assert_eq!(
        eval_str(&nested, &EvalContext::new()).unwrap(),
        ExprValue::Int(7)
    );
    let chain = format!("1{}", "+1".repeat(MAX_EXPR_LINKS));
    assert_eq!(
        eval_str(&chain, &EvalContext::new()).unwrap(),
        ExprValue::Int(i128::try_from(MAX_EXPR_LINKS + 1).unwrap())
    );
}

#[test]
fn very_long_expressions_are_errors() {
    assert!(parse(&format!("1{}", "+1".repeat(100_000))).is_err());
    assert!(parse(&format!("a{}", ".reverse".repeat(MAX_EXPR_LINKS + 1))).is_err());
    assert!(parse(&format!("[{}0]", "0,".repeat(MAX_EXPR_LEN))).is_err());
}

/// A chain of `depth` types, each holding the next, ending in a byte.
fn chain_of_types(depth: usize) -> String {
    let mut yaml = String::from("id: t0\nseq:\n  - id: next\n    type: t1\ntypes:\n");
    for i in 1..depth {
        write!(
            yaml,
            "  t{i}:\n    seq:\n      - id: next\n        type: t{}\n",
            i + 1
        )
        .unwrap();
    }
    write!(
        yaml,
        "  t{depth}:\n    seq:\n      - id: leaf\n        type: u1\n"
    )
    .unwrap();
    yaml
}

fn parse_chain(depth: usize) -> bool {
    let schema = RawSchema::from_yaml_str(&chain_of_types(depth)).unwrap();
    let registry = compile_schema(&schema).unwrap();
    Engine::new(&registry)
        .parse_type("t0", &mut Cursor::new(&[9]))
        .is_ok()
}

#[test]
fn types_nested_up_to_the_limit_parse_and_deeper_ones_are_errors() {
    assert!(parse_chain(MAX_TYPE_DEPTH - 1));
    assert!(!parse_chain(MAX_TYPE_DEPTH + 10));
    assert!(!parse_chain(1_000));
}

#[test]
fn types_that_contain_themselves_are_errors() {
    let yaml = "id: t\nseq:\n  - id: again\n    type: t\n";
    let schema = RawSchema::from_yaml_str(yaml).unwrap();
    let registry = compile_schema(&schema).unwrap();
    assert!(
        Engine::new(&registry)
            .parse_type("t", &mut Cursor::new(&[1, 2, 3]))
            .is_err()
    );
}

#[test]
fn a_schema_nested_without_bound_is_an_error() {
    // Flow sequences and mappings nested far deeper than a schema ever is:
    // the YAML reader refuses them, without a stack that grows with the input.
    let flow = format!("id: t\nseq: {}{}\n", "[".repeat(50_000), "]".repeat(50_000));
    assert!(RawSchema::from_yaml_str(&flow).is_err());
    let map = format!(
        "id: t\nseq: {}{}\n",
        "{a: ".repeat(50_000),
        "}".repeat(50_000)
    );
    assert!(RawSchema::from_yaml_str(&map).is_err());
}

/// Type declarations inside type declarations, `depth` deep: `types: {a: {types:
/// {a: ...}}}`. The text is small, however deep it goes.
fn nested_declarations(depth: usize) -> String {
    format!(
        "id: t\ntypes: {}{}\n",
        "{a: {types: ".repeat(depth),
        format!("{{}}{}", "}}".repeat(depth))
    )
}

#[test]
fn type_declarations_nest_as_far_as_the_reader_allows_and_no_further() {
    // A declaration takes two levels of the YAML (a mapping in a mapping), and
    // the reader refuses more than 64: the compiler recurses once per
    // declaration, so its depth is bounded by that.
    let schema = RawSchema::from_yaml_str(&nested_declarations(30)).unwrap();
    compile_schema(&schema).unwrap();
    assert!(RawSchema::from_yaml_str(&nested_declarations(1_000)).is_err());
    assert!(RawSchema::from_yaml_str(&nested_declarations(100_000)).is_err());
}
