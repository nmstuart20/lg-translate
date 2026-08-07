//! Turning what a US keyboard can type into what the models expect.
//!
//! Two jobs live here:
//!
//! * [`detect`] classifies a line by script so a line that already contains
//!   Hangul or Cyrillic (pasted, or typed with an OS IME) routes itself to the
//!   right model without a command.
//! * [`InputMode::Roman`] converts ASCII romanization into Hangul or Cyrillic,
//!   so the models are reachable with nothing but the keys already on the
//!   keyboard.

pub mod cyrillic;
pub mod hangul;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Script {
    Latin,
    Cyrillic,
    Hangul,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputMode {
    /// Pass typed text through untouched (for an OS IME, or pasted text).
    Raw,
    /// Interpret ASCII as romanized text and convert it to the target script.
    Roman,
}

impl InputMode {
    pub fn name(self) -> &'static str {
        match self {
            InputMode::Raw => "raw",
            InputMode::Roman => "roman",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "raw" => Some(InputMode::Raw),
            "roman" | "romanized" => Some(InputMode::Roman),
            _ => None,
        }
    }
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

/// Convert romanized ASCII into `script`. Returns the input unchanged for
/// `Script::Latin`, which needs no conversion.
pub fn romanize(script: Script, text: &str) -> String {
    match script {
        Script::Latin => text.to_string(),
        Script::Cyrillic => cyrillic::from_roman(text),
        Script::Hangul => hangul::from_roman(text),
    }
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
    }
}
