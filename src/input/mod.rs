//! Getting non-Latin text into the prompt from a US keyboard.
//!
//! [`editor`] reads the prompt with a symbol palette attached, so the arrow
//! keys can find and insert letters the keyboard has no key for. There is no
//! mapping from Latin letters to pick apart: what you see in the grid is what
//! goes into the line.
//!
//! [`Script`] only says which palette to offer. Nothing here decides where a
//! line is translated: every line goes to the pair the user selected.

pub mod editor;
pub mod palette;

use unicode_width::UnicodeWidthChar;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Script {
    Latin,
    Cyrillic,
    Greek,
}

/// Terminal columns `ch` occupies. Every palette symbol is one column wide,
/// but pasted text need not be, and the grid and the cursor both have to line
/// up over whatever is actually on the line.
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
    fn measures_text_by_terminal_columns() {
        assert_eq!(display_width("hi"), 2);
        assert_eq!(display_width("привет"), 6);
        assert_eq!(display_width("καλημέρα"), 8);
        assert_eq!(display_width("東京"), 4);
    }
}
