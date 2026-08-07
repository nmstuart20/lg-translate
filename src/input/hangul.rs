//! Revised-Romanization input for Korean.
//!
//! Hangul syllables are laid out algorithmically in Unicode, so once a
//! romanized syllable is broken into (onset, nucleus, coda) jamo indices the
//! codepoint is arithmetic:
//!
//! ```text
//! U+AC00 + ((onset * 21) + nucleus) * 28 + coda
//! ```
//!
//! The work is the segmentation. Romanization is ambiguous about where one
//! syllable ends and the next begins -- in `hangeul`, the `n` has to become the
//! coda of 한 rather than the onset of the next syllable, while in `haseyo` the
//! `s` has to do the opposite. Rather than guess with a heuristic we parse with
//! backtracking: prefer no coda, fall back to the shortest coda that lets the
//! rest of the word parse. Failed positions are memoized, so a word costs
//! linear time in practice instead of exponential.
//!
//! Known limits, both inherent to romanization rather than to this parser:
//!
//! * Revised Romanization neutralizes final ㄱ/ㅋ, ㄷ/ㅌ and ㅂ/ㅍ, so a coda
//!   `k`/`t`/`p` always composes as ㄱ/ㄷ/ㅂ.
//! * `isseoyo` is equally 있어요 and 이써요; we take the second reading.
//! * Romanization transcribes pronunciation, so consonants that assimilate are
//!   not written as the letter they come from. The common `-mnida` case is
//!   handled in [`coda_candidates`]; others (`hangnyeon` for 학년) are not.
//!
//! Use `/input raw` with an OS IME when a word needs to dodge either one.

/// Choseong (onset) index for each spelling, longest spellings first.
/// ㅇ is the silent onset and is supplied as a fallback rather than matched.
const ONSETS: &[(&str, usize)] = &[
    ("kk", 1),
    ("gg", 1),
    ("tt", 4),
    ("dd", 4),
    ("pp", 8),
    ("bb", 8),
    ("ss", 10),
    ("jj", 13),
    ("ch", 14),
    ("g", 0),
    ("n", 2),
    ("d", 3),
    ("r", 5),
    ("l", 5),
    ("m", 6),
    ("b", 7),
    ("s", 9),
    ("j", 12),
    ("k", 15),
    ("t", 16),
    ("p", 17),
    ("h", 18),
];

/// The silent onset ㅇ, used when a syllable starts with its vowel.
const SILENT_ONSET: usize = 11;

/// Jungseong (nucleus) index for each spelling, longest spellings first so
/// `yeo` wins over `ye`, `eo` over `e`, and so on.
const VOWELS: &[(&str, usize)] = &[
    ("wae", 10),
    ("yae", 3),
    ("yeo", 6),
    ("eui", 19),
    ("ae", 1),
    ("eo", 4),
    ("eu", 18),
    ("oe", 11),
    ("ui", 19),
    ("wa", 9),
    ("we", 15),
    ("wi", 16),
    ("wo", 14),
    ("ya", 2),
    ("ye", 7),
    ("yo", 12),
    ("yu", 17),
    ("a", 0),
    ("e", 5),
    ("i", 20),
    ("o", 8),
    ("u", 13),
];

/// Jongseong (coda) index for each spelling, **shortest spellings first**.
/// The parser tries these in order, so a lone consonant is preferred as the
/// next syllable's onset before it is absorbed into a cluster coda.
const CODAS: &[(&str, usize)] = &[
    ("g", 1),
    ("k", 1),
    ("n", 4),
    ("d", 7),
    ("t", 7),
    ("l", 8),
    ("r", 8),
    ("m", 16),
    ("b", 17),
    ("p", 17),
    ("s", 19),
    ("j", 22),
    ("h", 27),
    ("kk", 2),
    ("gg", 2),
    ("gs", 3),
    ("ks", 3),
    ("nj", 5),
    ("nh", 6),
    ("lg", 9),
    ("lk", 9),
    ("lm", 10),
    ("lb", 11),
    ("ls", 12),
    ("lt", 13),
    ("lp", 14),
    ("lh", 15),
    ("bs", 18),
    ("ps", 18),
    ("ss", 20),
    ("ng", 21),
    ("ch", 23),
];

/// Convert romanized Korean to Hangul, leaving anything that does not parse
/// exactly as it was typed.
pub fn from_roman(text: &str) -> String {
    let mut out = String::new();

    // A hyphen is Revised Romanization's explicit syllable divider ("jung-ang"),
    // so it joins letters into one word here and is dropped once the word
    // converts. Grouping it with the letters also keeps a standalone dash --
    // which never parses -- passing through untouched.
    for (is_word, run) in hyphenated_runs(text) {
        if is_word {
            out.push_str(&convert_word(&run));
        } else {
            out.push_str(&run);
        }
    }

    out
}

fn hyphenated_runs(text: &str) -> Vec<(bool, String)> {
    let mut runs: Vec<(bool, String)> = Vec::new();

    for ch in text.chars() {
        let is_word = ch.is_ascii_alphabetic() || ch == '-';
        match runs.last_mut() {
            Some((kind, buf)) if *kind == is_word => buf.push(ch),
            _ => runs.push((is_word, ch.to_string())),
        }
    }

    runs
}

fn convert_word(word: &str) -> String {
    let lowered = word.to_ascii_lowercase();
    let mut composed = String::new();

    for segment in lowered.split('-') {
        if segment.is_empty() {
            return word.to_string();
        }
        let chars: Vec<char> = segment.chars().collect();
        match parse(&chars) {
            Some(hangul) => composed.push_str(&hangul),
            // English words, names and typos stay legible instead of turning
            // into nonsense syllables.
            None => return word.to_string(),
        }
    }

    composed
}

fn parse(chars: &[char]) -> Option<String> {
    let mut out = String::new();
    let mut failed = vec![false; chars.len() + 1];

    if walk(chars, 0, &mut out, &mut failed) {
        Some(out)
    } else {
        None
    }
}

fn walk(chars: &[char], pos: usize, out: &mut String, failed: &mut [bool]) -> bool {
    if pos == chars.len() {
        return true;
    }
    // Whether the remainder parses depends only on where it starts, so one
    // failure at a position rules it out for every path that reaches it.
    if failed[pos] {
        return false;
    }

    for (onset_len, onset) in onset_candidates(chars, pos) {
        let after_onset = pos + onset_len;

        for (vowel_len, vowel) in matches_at(chars, after_onset, VOWELS) {
            let after_vowel = after_onset + vowel_len;

            for (coda_len, coda) in coda_candidates(chars, after_vowel) {
                let mark = out.len();
                out.push(compose(onset, vowel, coda));

                if walk(chars, after_vowel + coda_len, out, failed) {
                    return true;
                }

                out.truncate(mark);
            }
        }
    }

    failed[pos] = true;
    false
}

fn onset_candidates(chars: &[char], pos: usize) -> Vec<(usize, usize)> {
    let mut candidates = matches_at(chars, pos, ONSETS);
    // Consuming the consonant is more often right than treating it as the start
    // of a vowel-initial syllable, so the silent onset goes last.
    candidates.push((0, SILENT_ONSET));
    candidates
}

fn coda_candidates(chars: &[char], pos: usize) -> Vec<(usize, usize)> {
    // Index 0 is "no coda", tried first: `haseyo` is 하세요, not 핫에요.
    let mut candidates = vec![(0usize, 0usize)];

    // Romanization writes pronunciation, and a final ㅂ before ㄴ assimilates to
    // [m] -- which is why the very common formal ending -ㅂ니다 is romanized
    // "-mnida". Offering ㅂ ahead of ㅁ there gets `gamsahamnida` to 감사합니다
    // instead of 감사함니다. ㅁ is still offered below, so the rarer words that
    // genuinely have it still parse.
    if chars.get(pos) == Some(&'m') && chars.get(pos + 1) == Some(&'n') {
        candidates.push((1, 17));
    }

    candidates.extend(matches_at(chars, pos, CODAS));
    candidates
}

fn matches_at(chars: &[char], pos: usize, table: &[(&str, usize)]) -> Vec<(usize, usize)> {
    table
        .iter()
        .filter_map(|(spelling, index)| {
            // Every spelling is ASCII, so byte length is character length.
            let len = spelling.len();
            if pos + len <= chars.len()
                && chars[pos..pos + len].iter().copied().eq(spelling.chars())
            {
                Some((len, *index))
            } else {
                None
            }
        })
        .collect()
}

fn compose(onset: usize, vowel: usize, coda: usize) -> char {
    let code = 0xAC00 + ((onset * 21 + vowel) * 28 + coda) as u32;
    // Indices come from the tables above and are in range by construction.
    char::from_u32(code).expect("hangul syllable index out of range")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn composes_syllables_arithmetically() {
        assert_eq!(compose(18, 0, 4), '한');
        assert_eq!(compose(0, 18, 8), '글');
        assert_eq!(compose(0, 0, 16), '감');
    }

    #[test]
    fn resolves_coda_versus_next_onset() {
        // `n` must become a coda here...
        assert_eq!(from_roman("hangeul"), "한글");
        // ...but `s` must not there.
        assert_eq!(from_roman("haseyo"), "하세요");
        // Requires backtracking past the shorter `n` coda to reach `ng`.
        assert_eq!(from_roman("annyeong"), "안녕");
    }

    #[test]
    fn converts_common_phrases() {
        assert_eq!(from_roman("annyeonghaseyo"), "안녕하세요");
        assert_eq!(from_roman("gamsahamnida"), "감사합니다");
        assert_eq!(from_roman("seoul"), "서울");
        assert_eq!(from_roman("hanguk"), "한국");
    }

    #[test]
    fn keeps_punctuation_spacing_and_case() {
        assert_eq!(from_roman("annyeong, seoul!"), "안녕, 서울!");
        assert_eq!(from_roman("Seoul"), "서울");
    }

    #[test]
    fn hyphen_divides_syllables_and_is_dropped() {
        assert_eq!(from_roman("jung-ang"), "중앙");
        // A dash that is not a syllable divider survives.
        assert_eq!(from_roman("seoul - hanguk"), "서울 - 한국");
    }

    #[test]
    fn leaves_unparseable_words_alone() {
        assert_eq!(from_roman("xyz"), "xyz");
        assert_eq!(from_roman("wifi"), "wifi");
        // Anything that *is* a valid romanization does convert, so this mode is
        // only for lines meant as Korean -- `/input raw` is the escape hatch.
        assert_eq!(from_roman("hello"), "헬로");
    }
}
