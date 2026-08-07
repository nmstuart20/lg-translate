//! Transliteration input for Russian.
//!
//! Much simpler than Hangul: Cyrillic has no syllable composition, so this is a
//! greedy longest-match over a fixed table. Digraphs (`shch`, `zh`, `ya`, ...)
//! are listed before the single letters they start with, so `sh` never gets a
//! chance to be read as `s` + `h`.
//!
//! The scheme is the informal one people actually type rather than a transliteration
//! standard, and it is deliberately forgiving: both `h` and `kh` give х and
//! both `c` and `ts` give ц. The three letters with no Latin counterpart use
//! the quote keys: `e'` is э, `'` is ь, `"` is ъ. Letters with no rule (`q`)
//! pass through so they stay visible instead of vanishing.
//!
//! The one contextual rule is `y`, which is ы after a consonant but й after a
//! vowel -- otherwise `syn` and `chay` cannot both be typed the obvious way.

/// Ordered longest-spelling-first; the matcher takes the first hit.
const RULES: &[(&str, &str)] = &[
    ("shch", "щ"),
    // The reflexive verb endings -тся/-ться are common enough that reading
    // their `ts` as ц would be wrong more often than right, so they are spelled
    // out ahead of the `ts` rule. `tsvet` still reaches цвет.
    ("tsya", "тся"),
    ("sch", "щ"),
    ("e'", "э"),
    ("yo", "ё"),
    ("zh", "ж"),
    ("kh", "х"),
    ("ts", "ц"),
    ("ch", "ч"),
    ("sh", "ш"),
    ("yu", "ю"),
    ("ya", "я"),
    ("ye", "е"),
    ("a", "а"),
    ("b", "б"),
    ("v", "в"),
    ("g", "г"),
    ("d", "д"),
    ("e", "е"),
    ("z", "з"),
    ("i", "и"),
    ("j", "й"),
    ("k", "к"),
    ("l", "л"),
    ("m", "м"),
    ("n", "н"),
    ("o", "о"),
    ("p", "п"),
    ("r", "р"),
    ("s", "с"),
    ("t", "т"),
    ("u", "у"),
    ("f", "ф"),
    ("h", "х"),
    ("c", "ц"),
    ("y", "ы"),
    ("x", "х"),
    ("w", "в"),
    // The signs have no Latin letter, so they borrow the quote keys:
    // `chitat'` -> читать, `pod"ezd` -> подъезд.
    ("'", "ь"),
    ("\"", "ъ"),
];

/// Convert transliterated Russian to Cyrillic, preserving spacing, digits and
/// punctuation, and capitalizing output where the input was capitalized.
pub fn from_roman(text: &str) -> String {
    let mut out = String::new();

    for (is_word, run) in word_runs(text) {
        if is_word {
            out.push_str(&convert_run(&run));
        } else {
            out.push_str(&run);
        }
    }

    out
}

fn word_runs(text: &str) -> Vec<(bool, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut runs: Vec<(bool, String)> = Vec::new();

    for (index, &ch) in chars.iter().enumerate() {
        let is_word = ch.is_ascii_alphabetic() || is_sign(&chars, index);
        match runs.last_mut() {
            Some((kind, buf)) if *kind == is_word => buf.push(ch),
            _ => runs.push((is_word, ch.to_string())),
        }
    }

    runs
}

/// Whether a quote character is standing in for ь/ъ rather than quoting.
///
/// `'` only ever follows a letter inside a word (`chitat'`), while `"` sits
/// between two (`pod"ezd`). Requiring that much context leaves quotation marks
/// around a phrase alone.
fn is_sign(chars: &[char], index: usize) -> bool {
    let after_letter = index > 0 && chars[index - 1].is_ascii_alphabetic();

    match chars[index] {
        '\'' => after_letter,
        '"' => after_letter && chars.get(index + 1).is_some_and(char::is_ascii_alphabetic),
        _ => false,
    }
}

fn convert_run(run: &str) -> String {
    let original: Vec<char> = run.chars().collect();
    let lowered: Vec<char> = run.to_ascii_lowercase().chars().collect();

    let mut out = String::new();
    let mut pos = 0;

    while pos < lowered.len() {
        match rule_at(&lowered, pos) {
            Some((len, mut cyrillic)) => {
                // `ya`/`ye`/`yo`/`yu` already matched as digraphs above, so a
                // bare `y` here is either ы or й depending on what precedes it.
                if cyrillic == "ы" && follows_vowel(&out) {
                    cyrillic = "й";
                }
                // A digraph counts as capitalized when its first letter is, so
                // `Yalta` gives Ялта rather than ЯЛта or ялта.
                if original[pos].is_ascii_uppercase() {
                    out.extend(cyrillic.chars().flat_map(char::to_uppercase));
                } else {
                    out.push_str(cyrillic);
                }
                pos += len;
            }
            None => {
                // No rule (`q`, and the apostrophes handled below): keep it.
                out.push(original[pos]);
                pos += 1;
            }
        }
    }

    out
}

fn follows_vowel(out: &str) -> bool {
    const VOWELS: &str = "аеёиоуыэюя";
    out.chars()
        .last()
        .map(|c| {
            let lowered = c.to_lowercase().next().unwrap_or(c);
            VOWELS.contains(lowered)
        })
        .unwrap_or(false)
}

fn rule_at(chars: &[char], pos: usize) -> Option<(usize, &'static str)> {
    RULES.iter().find_map(|(spelling, cyrillic)| {
        // Every spelling is ASCII, so byte length is character length.
        let len = spelling.len();
        if pos + len <= chars.len() && chars[pos..pos + len].iter().copied().eq(spelling.chars()) {
            Some((len, *cyrillic))
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_words() {
        assert_eq!(from_roman("privet"), "привет");
        assert_eq!(from_roman("spasibo"), "спасибо");
        assert_eq!(from_roman("horosho"), "хорошо");
    }

    #[test]
    fn prefers_digraphs_over_single_letters() {
        assert_eq!(from_roman("shchi"), "щи");
        assert_eq!(from_roman("zhurnal"), "журнал");
    }

    #[test]
    fn resolves_y_by_what_precedes_it() {
        // After a consonant it is ы...
        assert_eq!(from_roman("syn"), "сын");
        assert_eq!(from_roman("yazyk"), "язык");
        // ...and after a vowel it is й.
        assert_eq!(from_roman("chay"), "чай");
        assert_eq!(from_roman("novyy"), "новый");
    }

    #[test]
    fn reflexive_endings_are_not_read_as_ts() {
        assert_eq!(from_roman("nahoditsya"), "находится");
        assert_eq!(from_roman("uchit'sya"), "учиться");
        // A genuine ц is still reachable.
        assert_eq!(from_roman("tsvet"), "цвет");
    }

    #[test]
    fn quote_keys_reach_the_letters_without_latin_counterparts() {
        assert_eq!(from_roman("e'to"), "это");
        assert_eq!(from_roman("chitat'"), "читать");
        assert_eq!(from_roman("pod\"ezd"), "подъезд");
        // Quotation marks around a phrase are not signs.
        assert_eq!(from_roman("on skazal \"privet\""), "он сказал \"привет\"");
    }

    #[test]
    fn preserves_case_punctuation_and_unknown_letters() {
        assert_eq!(from_roman("Privet, mir!"), "Привет, мир!");
        assert_eq!(from_roman("Yalta"), "Ялта");
        assert_eq!(from_roman("q"), "q");
    }
}
