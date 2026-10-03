//! Byte offsets and UTF-16 offsets, and the one conversion between them.
//!
//! Typst counts in BYTES of UTF-8; a browser's `String` and every textarea
//! selection count in UTF-16 CODE UNITS. The two agree on ASCII and nowhere
//! else — an `é` is two bytes and one unit, an emoji four bytes and two
//! units — so a span that crosses the boundary unconverted lands in the wrong
//! place on the first non-ASCII character. Every offset this module's callers
//! hand to JavaScript goes through [`Offsets::utf16`], and every offset
//! JavaScript hands in goes through [`Offsets::byte`].

/// One text's table of character boundaries, in both units.
pub struct Offsets {
    /// `(byte, utf16)` at the start of every character, then at the end of
    /// the text: strictly increasing in both coordinates.
    marks: Vec<(usize, usize)>,
}

impl Offsets {
    pub fn of(text: &str) -> Offsets {
        let mut marks = Vec::with_capacity(text.len() + 1);
        let mut unit = 0;
        for (byte, c) in text.char_indices() {
            marks.push((byte, unit));
            unit += c.len_utf16();
        }
        marks.push((text.len(), unit));
        Offsets { marks }
    }

    /// The UTF-16 offset of a byte offset. A byte inside a character rounds
    /// DOWN to that character's start; a byte past the end is the end.
    pub fn utf16(&self, byte: usize) -> usize {
        let at = self.marks.partition_point(|(b, _)| *b <= byte);
        self.marks[at.saturating_sub(1)].1
    }

    /// The byte offset of a UTF-16 offset, rounded down the same way — a
    /// cursor between the two halves of a surrogate pair is before the pair.
    #[cfg(any(feature = "ide", test))]
    pub fn byte(&self, unit: usize) -> usize {
        let at = self.marks.partition_point(|(_, u)| *u <= unit);
        self.marks[at.saturating_sub(1)].0
    }
}

#[cfg(test)]
mod tests {
    use super::Offsets;

    const TEXTS: &[&str] = &["", "plain", "é", "a😀b", "#let x = \"ü\"\n= Überschrift 🎉"];

    #[test]
    fn ascii_is_the_identity() {
        let offsets = Offsets::of("hello");
        for i in 0..=5 {
            assert_eq!(offsets.utf16(i), i);
            assert_eq!(offsets.byte(i), i);
        }
    }

    /// The law: at every character boundary the two conversions are inverse.
    #[test]
    fn boundaries_round_trip() {
        for text in TEXTS {
            let offsets = Offsets::of(text);
            for (byte, _) in text.char_indices().chain([(text.len(), ' ')]) {
                assert_eq!(offsets.byte(offsets.utf16(byte)), byte, "{text:?} at {byte}");
            }
            let units = text.encode_utf16().count();
            assert_eq!(offsets.utf16(text.len()), units);
        }
    }

    #[test]
    fn inside_a_character_rounds_down() {
        // 😀 is bytes 1..5 and units 1..3.
        let offsets = Offsets::of("a😀b");
        assert_eq!(offsets.utf16(3), 1);
        assert_eq!(offsets.byte(2), 1);
        assert_eq!(offsets.byte(3), 5);
        assert_eq!(offsets.utf16(99), 4);
    }
}
