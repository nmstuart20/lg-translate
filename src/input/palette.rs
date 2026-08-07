//! The symbol grids the arrow keys browse.
//!
//! One flat list per script, laid out into a grid at draw time so it reflows
//! with the terminal width. The order is the alphabet's own order rather than
//! anything keyboard-derived -- finding a letter here is a visual search, so
//! the arrangement people already know beats one organized around ASCII.

use super::Script;

pub struct Palette {
    /// Shown above the grid, e.g. "Russian".
    pub label: &'static str,
    pub symbols: &'static [char],
}

impl Palette {
    /// Width of one grid cell: the widest symbol plus a space on each side.
    /// Uniform cells keep the columns aligned when the highlight moves.
    pub fn cell_width(&self) -> usize {
        let widest = self
            .symbols
            .iter()
            .map(|&c| super::display_width_char(c))
            .max()
            .unwrap_or(1);
        widest + 2
    }
}

/// Russian, lowercase then uppercase. Both cases are in the one grid rather
/// than behind a shift toggle, so every reachable letter is on screen.
#[rustfmt::skip]
const CYRILLIC: &[char] = &[
    'а', 'б', 'в', 'г', 'д', 'е', 'ё', 'ж', 'з', 'и', 'й',
    'к', 'л', 'м', 'н', 'о', 'п', 'р', 'с', 'т', 'у', 'ф',
    'х', 'ц', 'ч', 'ш', 'щ', 'ъ', 'ы', 'ь', 'э', 'ю', 'я',
    'А', 'Б', 'В', 'Г', 'Д', 'Е', 'Ё', 'Ж', 'З', 'И', 'Й',
    'К', 'Л', 'М', 'Н', 'О', 'П', 'Р', 'С', 'Т', 'У', 'Ф',
    'Х', 'Ц', 'Ч', 'Ш', 'Щ', 'Ъ', 'Ы', 'Ь', 'Э', 'Ю', 'Я',
];

/// Korean jamo: the 19 consonants that can open a syllable, then the 21
/// vowels. Codas are not listed -- picking a consonant after a vowel makes it
/// one, and the cluster codas (ㄳ, ㄺ, ...) form from two consonants in a row,
/// so everything writable is reachable from these 40 (see [`super::hangul`]).
#[rustfmt::skip]
const HANGUL: &[char] = &[
    'ㄱ', 'ㄲ', 'ㄴ', 'ㄷ', 'ㄸ', 'ㄹ', 'ㅁ', 'ㅂ', 'ㅃ', 'ㅅ',
    'ㅆ', 'ㅇ', 'ㅈ', 'ㅉ', 'ㅊ', 'ㅋ', 'ㅌ', 'ㅍ', 'ㅎ',
    'ㅏ', 'ㅐ', 'ㅑ', 'ㅒ', 'ㅓ', 'ㅔ', 'ㅕ', 'ㅖ', 'ㅗ', 'ㅘ',
    'ㅙ', 'ㅚ', 'ㅛ', 'ㅜ', 'ㅝ', 'ㅞ', 'ㅟ', 'ㅠ', 'ㅡ', 'ㅢ', 'ㅣ',
];

static CYRILLIC_PALETTE: Palette = Palette {
    label: "Russian",
    symbols: CYRILLIC,
};

static HANGUL_PALETTE: Palette = Palette {
    label: "Korean",
    symbols: HANGUL,
};

/// The palette for `script`, or `None` for Latin, which the keyboard already
/// covers.
pub fn for_script(script: Script) -> Option<&'static Palette> {
    match script {
        Script::Latin => None,
        Script::Cyrillic => Some(&CYRILLIC_PALETTE),
        Script::Hangul => Some(&HANGUL_PALETTE),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin_has_no_palette() {
        assert!(for_script(Script::Latin).is_none());
    }

    #[test]
    fn cyrillic_covers_both_cases_of_the_alphabet() {
        let palette = for_script(Script::Cyrillic).unwrap();
        assert_eq!(palette.symbols.len(), 66);
        assert_eq!(palette.cell_width(), 3);
    }

    #[test]
    fn hangul_lists_onsets_then_vowels() {
        let palette = for_script(Script::Hangul).unwrap();
        assert_eq!(palette.symbols.len(), 40);
        // Wide symbols, so the cells are one column wider than Cyrillic's.
        assert_eq!(palette.cell_width(), 4);
    }
}
