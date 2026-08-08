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

/// Greek, lowercase then uppercase, each followed by its accented vowels.
///
/// The accents are not decoration: modern Greek writes a tonos on every
/// polysyllabic word, and the model was trained on text that has them. Both
/// sigmas are listed, since which one a word ends with is not something the
/// grid can decide.
#[rustfmt::skip]
const GREEK: &[char] = &[
    'α', 'β', 'γ', 'δ', 'ε', 'ζ', 'η', 'θ', 'ι', 'κ', 'λ', 'μ',
    'ν', 'ξ', 'ο', 'π', 'ρ', 'σ', 'ς', 'τ', 'υ', 'φ', 'χ', 'ψ', 'ω',
    'ά', 'έ', 'ή', 'ί', 'ό', 'ύ', 'ώ', 'ϊ', 'ϋ', 'ΐ', 'ΰ',
    'Α', 'Β', 'Γ', 'Δ', 'Ε', 'Ζ', 'Η', 'Θ', 'Ι', 'Κ', 'Λ', 'Μ',
    'Ν', 'Ξ', 'Ο', 'Π', 'Ρ', 'Σ', 'Τ', 'Υ', 'Φ', 'Χ', 'Ψ', 'Ω',
    'Ά', 'Έ', 'Ή', 'Ί', 'Ό', 'Ύ', 'Ώ', 'Ϊ', 'Ϋ',
];

static CYRILLIC_PALETTE: Palette = Palette {
    label: "Russian",
    symbols: CYRILLIC,
};

static GREEK_PALETTE: Palette = Palette {
    label: "Greek",
    symbols: GREEK,
};

/// The palette for `script`, or `None` for Latin, which the keyboard already
/// covers.
pub fn for_script(script: Script) -> Option<&'static Palette> {
    match script {
        Script::Latin => None,
        Script::Cyrillic => Some(&CYRILLIC_PALETTE),
        Script::Greek => Some(&GREEK_PALETTE),
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
    fn greek_covers_both_cases_and_their_accents() {
        let palette = for_script(Script::Greek).unwrap();
        assert_eq!(palette.symbols.len(), 69);
        assert_eq!(palette.cell_width(), 3);
        // The tonos vowels are what a modern Greek line actually needs.
        assert!(palette.symbols.contains(&'ή'));
        assert!(palette.symbols.contains(&'ς'));
    }
}
