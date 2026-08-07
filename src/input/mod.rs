//! Getting non-Latin text into the prompt from a US keyboard.
//!
//! Two jobs live here:
//!
//! * [`detect`] classifies a line by script so a line that already contains
//!   Hangul or Cyrillic (pasted, or typed with an OS IME) routes itself to the
//!   right model without a command.
//! * [`editor`] reads the prompt with a symbol palette attached, so the arrow
//!   keys can find and insert letters the keyboard has no key for. There is no
//!   mapping from Latin letters to pick apart: what you see in the grid is what
//!   goes into the line.

pub mod editor;
pub mod hangul;
pub mod palette;

use unicode_width::UnicodeWidthChar;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Script {
    Latin,
    Cyrillic,
    Hangul,
}

/// Which script a line is *already* written in.
///
/// Only reports `Cyrillic`/`Hangul` when such characters are actually present;
/// a plain ASCII line is `Latin` and its meaning depends on the active pair.
pub fn detect(text: &str) -> Script {
    let mut cyrillic = 0usize;
    let mut hangul = 0usize;

    for ch in text.chars() {
        match ch as u32 {
            // Cyrillic and Cyrillic Supplement.
            0x0400..=0x052F => cyrillic += 1,
            // Conjoining jamo, compatibility jamo, and precomposed syllables.
            0x1100..=0x11FF | 0x3130..=0x318F | 0xAC00..=0xD7A3 => hangul += 1,
            _ => {}
        }
    }

    if hangul > 0 && hangul >= cyrillic {
        Script::Hangul
    } else if cyrillic > 0 {
        Script::Cyrillic
    } else {
        Script::Latin
    }
}

/// Terminal columns `ch` occupies. Hangul is double-width and Cyrillic is not,
/// so the palette and the cursor both need this to line up.
pub fn display_width_char(ch: char) -> usize {
    ch.width().unwrap_or(0)
}

pub fn display_width(text: &str) -> usize {
    text.chars().map(display_width_char).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_scripts() {
        assert_eq!(detect("hello"), Script::Latin);
        assert_eq!(detect("привет"), Script::Cyrillic);
        assert_eq!(detect("안녕하세요"), Script::Hangul);
        // Mixed lines route by the non-Latin content they contain.
        assert_eq!(detect("say 안녕 to them"), Script::Hangul);
        // A jamo left standing at the end of a line still reads as Korean.
        assert_eq!(detect("ㄱ"), Script::Hangul);
    }

    #[test]
    fn measures_double_width_scripts() {
        assert_eq!(display_width("hi"), 2);
        assert_eq!(display_width("привет"), 6);
        assert_eq!(display_width("한국"), 4);
    }
}
