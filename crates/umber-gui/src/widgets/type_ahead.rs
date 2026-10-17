//! Dropdown type-ahead (GUI plan g7-polish-batch; photocraft #13).
//!
//! In an OPEN dropdown, typing a letter jumps to the next option whose
//! label starts with (or contains, case-insensitively) that letter; the
//! same letter again cycles through the matches, and quick typing builds
//! a prefix. A pause resets the prefix (photocraft's `PAUSE = 1.0`).
//!
//! Pure state — the consuming dropdown (a `ComboBox` or custom list)
//! feeds it each frame:
//! * [`TypeAhead::frame`] with the timestamp delta and whether the list
//!   is open, then the keys typed this frame;
//! * it answers the index the selection should move to, if any.
//!
//! No egui dependencies beyond `Key` — the shell maps events to
//! characters its way.

use std::time::Duration;

/// How long after the last keypress the prefix resets (photocraft: 1 s).
pub const PAUSE: Duration = Duration::from_secs(1);

/// The type-ahead state for one open dropdown.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TypeAhead {
    /// The accumulated prefix (lowercased).
    prefix: String,
    /// Time since the last keypress (accumulated frame deltas).
    since_last: Duration,
    /// The last match index the cycle visited.
    last_match: Option<usize>,
}

impl TypeAhead {
    /// Feed one frame. `delta` is the time since the previous frame (the
    /// caller's clock); `keys` are the characters typed THIS frame
    /// (already lowercased by the caller or here). Returns the option
    /// index to select, if the prefix matched.
    ///
    /// When the list closes the caller drops this — the state is
    /// per-open-list.
    pub fn frame(
        &mut self,
        delta: Duration,
        keys: impl IntoIterator<Item = char>,
        labels: &[impl AsRef<str>],
    ) -> Option<usize> {
        if self.since_last + delta > PAUSE {
            self.prefix.clear();
            self.last_match = None;
        }
        self.since_last += delta;
        let mut had_keys = false;
        for c in keys {
            had_keys = true;
            let c = c.to_ascii_lowercase();
            // The same letter again (single-char prefix, same key) is a
            // CYCLE press, not an extension: "pp" is two taps of "p",
            // never the prefix "pp" (photocraft #13's "the same letter
            // again cycles through matches").
            if self.prefix.chars().count() != 1 || self.prefix != c.to_string() {
                self.prefix.push(c);
            }
            self.since_last = Duration::ZERO;
        }
        if self.prefix.is_empty() {
            return None;
        }
        // Match: labels that CONTAIN the prefix as a substring
        // (case-insensitive). A KEY frame cycles: first match AFTER
        // `last_match` wins (the same letter again cycles through
        // matches). A KEYLESS frame re-affirms: it matches from
        // `last_match` INCLUSIVE, so a preserved prefix (the
        // miss-preserving rule below) does not silently jump to a
        // different option without a keypress.
        let hay: Vec<String> = labels
            .iter()
            .map(|l| l.as_ref().to_ascii_lowercase())
            .collect();
        let n = hay.len();
        let start = match (self.last_match, had_keys) {
            (Some(m), true) => (m + 1) % n.max(1),
            (Some(m), false) => m % n.max(1),
            (None, _) => 0,
        };
        for off in 0..n {
            let i = (start + off) % n;
            if hay[i].contains(&self.prefix) {
                self.last_match = Some(i);
                return Some(i);
            }
        }
        // No match for the extended prefix: keep the old prefix working
        // (drop the new key) rather than locking the selection dead.
        self.prefix.pop();
        self.since_last = Duration::ZERO;
        None
    }

    /// The current prefix (for tests + a possible "search: x" readout).
    pub fn prefix(&self) -> &str {
        &self.prefix
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn labels() -> Vec<&'static str> {
        vec!["Paint", "Smudge", "Clone", "Erase", "Pick"]
    }

    #[test]
    fn a_letter_jumps_to_the_next_matching_option() {
        let mut t = TypeAhead::default();
        // 'c' matches "Clone" (index 2).
        assert_eq!(
            t.frame(Duration::ZERO, ['c'], &labels()),
            Some(2),
            "contains-match jumps to Clone"
        );
    }

    #[test]
    fn the_same_letter_cycles_through_matches() {
        let mut t = TypeAhead::default();
        // 'p' matches "Paint" (0) then "Pick" (4), then wraps to Paint.
        assert_eq!(t.frame(Duration::ZERO, ['p'], &labels()), Some(0));
        assert_eq!(
            t.frame(Duration::from_millis(100), ['p'], &labels()),
            Some(4),
            "the same letter again cycles — 'pp' is two taps, never a prefix"
        );
        assert_eq!(
            t.frame(Duration::from_millis(100), ['p'], &labels()),
            Some(0),
            "the cycle wraps"
        );
        assert_eq!(t.prefix(), "p", "a cycle press never extends the prefix");
    }

    #[test]
    fn quick_typing_builds_a_prefix() {
        let mut t = TypeAhead::default();
        // "sm" builds to "Smudge".
        assert_eq!(t.frame(Duration::ZERO, ['s'], &labels()), Some(1));
        assert_eq!(t.frame(Duration::ZERO, ['m'], &labels()), Some(1));
        assert_eq!(t.prefix(), "sm");
    }

    #[test]
    fn a_pause_resets_the_prefix() {
        let mut t = TypeAhead::default();
        assert_eq!(t.frame(Duration::ZERO, ['s'], &labels()), Some(1));
        // Over the pause: the prefix clears, so 'p' starts fresh.
        assert_eq!(
            t.frame(PAUSE + Duration::from_millis(1), ['p'], &labels()),
            Some(0),
            "after the pause 'p' matches Paint, not 'sp'"
        );
        assert_eq!(t.prefix(), "p");
    }

    #[test]
    fn a_nonmatching_key_does_not_kill_the_prefix() {
        let mut t = TypeAhead::default();
        assert_eq!(t.frame(Duration::ZERO, ['s'], &labels()), Some(1));
        // 'z' extends to "sz" — no match this frame (None), but the
        // extension drops and "s" survives, so the next frame matches.
        assert_eq!(t.frame(Duration::ZERO, ['z'], &labels()), None);
        assert_eq!(t.prefix(), "s");
        assert_eq!(
            t.frame(Duration::from_millis(16), [], &labels()),
            Some(1),
            "the preserved prefix still matches"
        );
    }
}
