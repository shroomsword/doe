//! The places the person jumped from, to walk back and forward as an editor does.
//!
//! A jump (`g`, and later a followed pointer) records where it came from; moving
//! by arrows or pages does not. The list is a line with the present place on it:
//! going back and then jumping somewhere new drops the places that were ahead.

/// The most places kept; the oldest go first.
pub const LIMIT: usize = 100;

/// A place in the file: the byte the cursor was on and the field that was selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    /// The byte under the cursor.
    pub offset: u64,
    /// The path of the selected field in the tree.
    pub path: Vec<usize>,
}

/// The places, with the one the person is at.
#[derive(Debug, Default)]
pub struct History {
    places: Vec<Place>,
    at: usize,
}

impl History {
    /// How many places are kept.
    #[must_use]
    pub fn len(&self) -> usize {
        self.places.len()
    }

    /// Whether no place is kept.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.places.is_empty()
    }

    /// A jump is about to leave `here`: it becomes the present place, and the places ahead are dropped.
    pub fn leave(&mut self, here: Place) {
        if self.places.is_empty() {
            self.places.push(here);
            self.at = 0;
            return;
        }
        if let Some(slot) = self.places.get_mut(self.at) {
            *slot = here;
        }
        self.places.truncate(self.at + 1);
    }

    /// The jump landed on `there`. A place within `screen` bytes of the one left replaces it, so that
    /// a short hop does not take a step of the history.
    pub fn arrive(&mut self, there: Place, screen: u64) {
        let near = self
            .places
            .get(self.at)
            .is_some_and(|last| last.offset.abs_diff(there.offset) < screen);
        if near && self.at > 0 {
            if let Some(slot) = self.places.get_mut(self.at) {
                *slot = there;
            }
            return;
        }
        if near {
            // nothing was before it: the place left is the same one, so there is no jump to remember
            self.places.clear();
            self.at = 0;
            return;
        }
        self.places.push(there);
        if self.places.len() > LIMIT {
            self.places.remove(0);
        }
        self.at = self.places.len() - 1;
    }

    /// Note where the person is before walking, so that coming back lands here.
    pub fn remember(&mut self, here: Place) {
        if let Some(slot) = self.places.get_mut(self.at) {
            *slot = here;
        }
    }

    /// Move one place back (or forward) and give it; `None` at the oldest (or the newest).
    pub fn walk(&mut self, forward: bool) -> Option<Place> {
        let next = if forward {
            self.at.checked_add(1).filter(|&n| n < self.places.len())?
        } else {
            self.at.checked_sub(1)?
        };
        self.at = next;
        self.places.get(next).cloned()
    }

    /// Drop the place that is current (it no longer exists), staying on the one before it.
    pub fn drop_current(&mut self) {
        if self.at < self.places.len() {
            self.places.remove(self.at);
            self.at = self
                .at
                .saturating_sub(1)
                .min(self.places.len().saturating_sub(1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(offset: u64) -> Place {
        Place {
            offset,
            path: vec![],
        }
    }

    #[test]
    fn jumps_are_walked_back_and_forward() {
        let mut h = History::default();
        h.leave(at(0));
        h.arrive(at(1000), 100);
        h.leave(at(1000));
        h.arrive(at(5000), 100);
        assert_eq!(h.len(), 3);
        h.remember(at(5000));
        assert_eq!(h.walk(false), Some(at(1000)));
        assert_eq!(h.walk(false), Some(at(0)));
        assert_eq!(h.walk(false), None);
        assert_eq!(h.walk(true), Some(at(1000)));
        assert_eq!(h.walk(true), Some(at(5000)));
        assert_eq!(h.walk(true), None);
    }

    #[test]
    fn a_jump_after_going_back_drops_the_places_ahead() {
        let mut h = History::default();
        h.leave(at(0));
        h.arrive(at(1000), 100);
        h.leave(at(1000));
        h.arrive(at(5000), 100);
        h.remember(at(5000));
        assert_eq!(h.walk(false), Some(at(1000)));
        h.leave(at(1000));
        h.arrive(at(9000), 100);
        assert_eq!(h.walk(true), None);
        assert_eq!(h.walk(false), Some(at(1000)));
    }

    #[test]
    fn a_hop_within_a_screen_replaces_the_place_and_the_list_is_limited() {
        let mut h = History::default();
        h.leave(at(0));
        h.arrive(at(1000), 100);
        h.leave(at(1000));
        h.arrive(at(1050), 100);
        assert_eq!(h.len(), 2);
        // a hop from the very first place that stays on the screen is no jump
        let mut first = History::default();
        first.leave(at(10));
        first.arrive(at(20), 100);
        assert!(first.is_empty());
        let mut long = History::default();
        long.leave(at(0));
        for n in 1..=150_u64 {
            long.arrive(at(n * 1000), 100);
            long.leave(at(n * 1000));
        }
        assert_eq!(long.len(), LIMIT);
    }
}
