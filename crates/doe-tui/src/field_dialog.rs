//! What is asked when a field is added to the schema that is being built (`a`). The rules (what is proposed, accepted and
//! asked next) are in `doe_draft`, which the web IDE uses too; this module adds the way that the prompt of the viewer is
//! asked.

pub use doe_draft::field_dialog::*;

use crate::prompt::Kind as Prompt;

/// What is asked after `after` (a question about the kind), or none when only the name is left.
#[must_use]
pub fn next(kind: &str, after: Prompt) -> Option<Prompt> {
    let step = match after {
        Prompt::FieldKind => Step::FieldKind,
        Prompt::FieldLength => Step::FieldLength,
        Prompt::FieldSize => Step::FieldSize,
        Prompt::FieldEncoding => Step::FieldEncoding,
        Prompt::FieldTerminator => Step::FieldTerminator,
        Prompt::FieldMagic => Step::FieldMagic,
        Prompt::FieldRepeat => Step::FieldRepeat,
        _ => return None,
    };
    doe_draft::field_dialog::next(kind, step).map(|step| match step {
        Step::FieldKind => Prompt::FieldKind,
        Step::FieldLength => Prompt::FieldLength,
        Step::FieldSize => Prompt::FieldSize,
        Step::FieldEncoding => Prompt::FieldEncoding,
        Step::FieldTerminator => Prompt::FieldTerminator,
        Step::FieldMagic => Prompt::FieldMagic,
        Step::FieldRepeat => Prompt::FieldRepeat,
    })
}
