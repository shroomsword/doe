//! The schema that a viewer builds from the bytes of a file (`doe --tui FILE --build OUT`, and the web IDE's builder):
//! the model ([`draft`]: the fields, the edits that can be undone, the YAML that they come to and that is read back) and
//! the rules for what a dialog proposes, accepts and asks next ([`field_dialog`]).
//!
//! Nothing here touches a terminal, a file or a page, so that both viewers (and the WebAssembly module) use one
//! implementation with one set of tests.

pub mod draft;
pub mod field_dialog;
