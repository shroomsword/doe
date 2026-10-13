//! Deserializes arbitrary text as a schema and compiles what deserializes,
//! then parses a few bytes with every compiled type. Errors are fine; panics
//! and hangs are not.

#![no_main]

use doe::parser::cursor::Cursor;
use doe::parser::engine::Engine;
use doe::schema::compiler::compile_schema;
use doe::schema::raw::RawSchema;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|text: &str| {
    let Ok(schema) = RawSchema::from_yaml_str(text) else {
        return;
    };
    let Ok(registry) = compile_schema(&schema) else {
        return;
    };
    let engine = Engine::new(&registry);
    for name in registry.types.keys() {
        let mut cursor = Cursor::new(&[1, 2, 3, 4, 5, 6, 7, 8, 0, 255, 128, 64]);
        drop(engine.parse_type(name, &mut cursor));
    }
});
