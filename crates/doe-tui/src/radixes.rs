//! The radix in which the integers of the tree are written: one for all of
//! them (`x`), and a radix of its own for a field (`X`), kept by the field's
//! type and name, so that it holds for every element of a list and after the
//! schema is read again, and not for a place in the tree.

use std::collections::HashMap;

use doe::radix::Radix;

/// What the viewer has chosen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Radixes {
    /// The radix of every integer that has none of its own.
    pub global: Radix,
    fields: HashMap<String, Radix>,
}

impl Radixes {
    /// Start with `global` and no choice of a field.
    #[must_use]
    pub fn new(global: Radix) -> Self {
        Radixes {
            global,
            fields: HashMap::new(),
        }
    }

    /// The radix of a field: its own, or the global one.
    #[must_use]
    pub fn of(&self, field: &str) -> Radix {
        self.fields.get(field).copied().unwrap_or(self.global)
    }

    /// The radix that the field has chosen for itself, if it has.
    #[must_use]
    pub fn own(&self, field: &str) -> Option<Radix> {
        self.fields.get(field).copied()
    }

    /// The next radix for all integers: decimal, hex, binary, octal.
    pub fn cycle_global(&mut self) {
        self.global = self.global.next();
    }

    /// The next choice for a field, going through the radixes and then back to
    /// none (which is the global one): returns what it is now.
    pub fn cycle_field(&mut self, field: &str) -> Option<Radix> {
        let next = match self.own(field) {
            None => Some(Radix::Decimal),
            Some(Radix::Octal) => None,
            Some(radix) => Some(radix.next()),
        };
        match next {
            Some(radix) => {
                self.fields.insert(field.to_owned(), radix);
            }
            None => {
                self.fields.remove(field);
            }
        }
        next
    }

    /// How many fields have a radix of their own.
    #[must_use]
    pub fn fields(&self) -> usize {
        self.fields.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_has_the_global_radix_until_it_chooses_its_own() {
        let mut radixes = Radixes::new(Radix::Hex);
        assert_eq!(radixes.of("t.a"), Radix::Hex);
        assert_eq!(radixes.cycle_field("t.a"), Some(Radix::Decimal));
        assert_eq!(radixes.of("t.a"), Radix::Decimal);
        assert_eq!(radixes.of("t.b"), Radix::Hex);
        radixes.cycle_global();
        assert_eq!(radixes.global, Radix::Binary);
        // its own choice stays when the global one changes
        assert_eq!(radixes.of("t.a"), Radix::Decimal);
        assert_eq!(radixes.fields(), 1);
    }

    #[test]
    fn the_choices_of_a_field_go_round_and_end_with_none() {
        let mut radixes = Radixes::default();
        let seen: Vec<Option<Radix>> = (0..5).map(|_| radixes.cycle_field("t.a")).collect();
        assert_eq!(
            seen,
            [
                Some(Radix::Decimal),
                Some(Radix::Hex),
                Some(Radix::Binary),
                Some(Radix::Octal),
                None
            ]
        );
        assert_eq!(radixes.fields(), 0);
    }
}
