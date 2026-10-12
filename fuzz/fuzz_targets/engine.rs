//! Parses arbitrary bytes with a fixed schema that exercises every field kind:
//! integers, floats, bits, bytes, strings, enums, nested types, conditionals,
//! and all repeat modes with sizes and counts taken from earlier fields.

#![no_main]

use std::sync::OnceLock;

use doe::parser::cursor::Cursor;
use doe::parser::engine::Engine;
use doe::schema::compiler::compile_schema;
use doe::schema::ir::TypeRegistry;
use doe::schema::raw::RawSchema;
use libfuzzer_sys::fuzz_target;

const SCHEMA: &str = include_str!("../seeds/engine/everything.yaml");
const MAGIC_SCHEMA: &str = include_str!("../seeds/engine/magic.yaml");

fn registry(source: &'static str, cell: &'static OnceLock<TypeRegistry>) -> &'static TypeRegistry {
    cell.get_or_init(|| {
        let schema: RawSchema = serde_yaml::from_str(source).expect("fuzz schema is valid YAML");
        compile_schema(&schema).expect("fuzz schema compiles")
    })
}

fuzz_target!(|data: &[u8]| {
    static EVERYTHING: OnceLock<TypeRegistry> = OnceLock::new();
    static MAGIC: OnceLock<TypeRegistry> = OnceLock::new();

    let mut cursor = Cursor::new(data);
    drop(Engine::new(registry(SCHEMA, &EVERYTHING)).parse_type("everything", &mut cursor));

    let mut cursor = Cursor::new(data);
    drop(Engine::new(registry(MAGIC_SCHEMA, &MAGIC)).parse_type("magic", &mut cursor));
});
