# Contributing

## Wording of messages

Everything a person reads (error and warning texts, notices and prompts of the
terminal viewer, labels and status lines of the web IDE, `--help`, the manual
pages and the documentation) is in the neutral voice: it states what happened
and what to do. It does not speak in the first person ("I", "we", "my",
"our", "let's"), because the software is a tool, not a speaker.

- "no type paths: give one with `-I`", not "I found no type paths";
- "the file does not fit the schema", not "we could not parse the file";
- placeholders in examples are not "my" anything (`custom_format.yaml`).

The second person ("you") is fine where an instruction needs it. Comments that
explain the code are not user-facing and may say "we".

`crates/doe-cli/tests/integration/voice.rs` scans the strings and doc comments
of the Rust sources, the strings of the web page's sources, its HTML and the
documentation for these words. If it finds one, reword the text; a flag or a
Roman numeral that looks like a pronoun goes in `NOT_A_PRONOUN` there.

## Before a pull request

Run the tests of the crate you changed (`cargo test -p CRATE`; one crate at a
time on a small machine), `cargo fmt --all` and `cargo clippy --workspace
--all-targets`. For the web IDE, `npm run typecheck` and `npx playwright test`
in `web/`. Pull requests that change the public API of a library crate take the
`breaking` label.
