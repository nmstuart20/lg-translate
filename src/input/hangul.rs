//! Composing picked jamo into Hangul syllables.
//!
//! The palette offers the 40 jamo people actually pick (19 onsets, 21 vowels),
//! but Korean is written in syllable blocks, not letters in a row. This module
//! is the automaton in between: each picked jamo either merges into the
//! syllable being built or starts a new one, exactly as a 2-set IME behaves, so
//! the line reads as Korean while it is typed.
//!
//! Syllables are laid out algorithmically in Unicode, so composing is
//! arithmetic once the three slots are known:
//!
//! ```text
//! U+AC00 + ((onset * 21) + vowel) * 28 + coda
//! ```
//!
//! Two rules carry most of the work, and both come from the fact that a
//! consonant's role is only settled by what comes *after* it:
//!
//! * A consonant picked after a complete syllable becomes that syllable's coda
//!   (가 + ㅁ -> 감), because that is the reading that is still open to change.
//! * A vowel picked next takes the coda back out and makes it the onset of a
//!   new syllable (감 + ㅏ -> 가마), which is the only way 가마 can be typed.
//!
//! A vowel with no consonant waiting takes the silent onset ㅇ, since that is
//! how a vowel-initial syllable is written anyway (ㅏ alone is not a word).

/// Compatibility jamo for each choseong (onset) index.
#[rustfmt::skip]
const CHOSEONG: [char; 19] = [
    'ㄱ', 'ㄲ', 'ㄴ', 'ㄷ', 'ㄸ', 'ㄹ', 'ㅁ', 'ㅂ', 'ㅃ', 'ㅅ',
    'ㅆ', 'ㅇ', 'ㅈ', 'ㅉ', 'ㅊ', 'ㅋ', 'ㅌ', 'ㅍ', 'ㅎ',
];

/// The silent onset ㅇ, used when a syllable starts with its vowel.
const SILENT_ONSET: usize = 11;

/// Compatibility jamo for each jungseong (vowel) index.
#[rustfmt::skip]
const JUNGSEONG: [char; 21] = [
    'ㅏ', 'ㅐ', 'ㅑ', 'ㅒ', 'ㅓ', 'ㅔ', 'ㅕ', 'ㅖ', 'ㅗ', 'ㅘ',
    'ㅙ', 'ㅚ', 'ㅛ', 'ㅜ', 'ㅝ', 'ㅞ', 'ㅟ', 'ㅠ', 'ㅡ', 'ㅢ', 'ㅣ',
];

/// Compatibility jamo for each jongseong (coda) index. Index 0 is "no coda"
/// and has no jamo of its own, so this table starts at index 1.
#[rustfmt::skip]
const JONGSEONG: [char; 27] = [
    'ㄱ', 'ㄲ', 'ㄳ', 'ㄴ', 'ㄵ', 'ㄶ', 'ㄷ', 'ㄹ', 'ㄺ', 'ㄻ',
    'ㄼ', 'ㄽ', 'ㄾ', 'ㄿ', 'ㅀ', 'ㅁ', 'ㅂ', 'ㅄ', 'ㅅ', 'ㅆ',
    'ㅇ', 'ㅈ', 'ㅊ', 'ㅋ', 'ㅌ', 'ㅍ', 'ㅎ',
];

/// The two-consonant codas, as (first, second, combined). A cluster is built by
/// picking its two consonants in a row, and split back apart when a vowel
/// claims the second one (앉 + ㅏ -> 안자).
#[rustfmt::skip]
const CLUSTERS: [(char, char, char); 11] = [
    ('ㄱ', 'ㅅ', 'ㄳ'),
    ('ㄴ', 'ㅈ', 'ㄵ'),
    ('ㄴ', 'ㅎ', 'ㄶ'),
    ('ㄹ', 'ㄱ', 'ㄺ'),
    ('ㄹ', 'ㅁ', 'ㄻ'),
    ('ㄹ', 'ㅂ', 'ㄼ'),
    ('ㄹ', 'ㅅ', 'ㄽ'),
    ('ㄹ', 'ㅌ', 'ㄾ'),
    ('ㄹ', 'ㅍ', 'ㄿ'),
    ('ㄹ', 'ㅎ', 'ㅀ'),
    ('ㅂ', 'ㅅ', 'ㅄ'),
];

const SYLLABLE_BASE: u32 = 0xAC00;
const SYLLABLE_LAST: u32 = 0xD7A3;

/// Add `jamo` to the end of `buf`, merging it into the syllable in progress
/// where Korean orthography says it belongs.
///
/// Only ever touches the last character, so the caller can use it for any
/// insertion at the end of a line and fall back to a plain insert elsewhere.
pub fn push(buf: &mut Vec<char>, jamo: char) {
    match (choseong_index(jamo), jungseong_index(jamo)) {
        (_, Some(vowel)) => push_vowel(buf, vowel),
        (Some(_), None) => push_consonant(buf, jamo),
        // Not something the palette offers (a cluster coda pasted in, say);
        // nothing sensible to merge it with.
        (None, None) => buf.push(jamo),
    }
}

fn push_consonant(buf: &mut Vec<char>, jamo: char) {
    if let Some((onset, vowel, coda)) = buf.last().copied().and_then(decompose) {
        // An open syllable takes the consonant as its coda...
        if coda == 0 {
            if let Some(index) = jongseong_index(jamo) {
                replace_last(buf, compose(onset, vowel, index));
                return;
            }
        // ...and one that already has a coda can still grow it to a cluster.
        } else if let Some(index) = combine(JONGSEONG[coda - 1], jamo).and_then(jongseong_index) {
            replace_last(buf, compose(onset, vowel, index));
            return;
        }
    }

    // Nothing to attach to. It stands alone until a vowel makes it an onset.
    buf.push(jamo);
}

fn push_vowel(buf: &mut Vec<char>, vowel: usize) {
    if let Some(last) = buf.last().copied() {
        // A consonant left standing was waiting for exactly this.
        if let Some(onset) = choseong_index(last) {
            replace_last(buf, compose(onset, vowel, 0));
            return;
        }

        // A coda belongs to this vowel, not to the syllable it is sitting on,
        // so hand it over -- keeping the first half of a cluster behind.
        if let Some((prev_onset, prev_vowel, coda)) = decompose(last) {
            if coda > 0 {
                let (kept, moved) = match split(JONGSEONG[coda - 1]) {
                    Some((first, second)) => (jongseong_index(first).unwrap_or(0), second),
                    None => (0, JONGSEONG[coda - 1]),
                };

                if let Some(onset) = choseong_index(moved) {
                    replace_last(buf, compose(prev_onset, prev_vowel, kept));
                    buf.push(compose(onset, vowel, 0));
                    return;
                }
            }
        }
    }

    buf.push(compose(SILENT_ONSET, vowel, 0));
}

/// Remove one *jamo* from the end of `buf` rather than one character, so a
/// mis-picked coda can be taken back without losing the whole syllable
/// (감 -> 가 -> ㄱ). Returns false when the last character is not a syllable
/// and the caller should just delete it.
pub fn backspace(buf: &mut Vec<char>) -> bool {
    let Some((onset, vowel, coda)) = buf.last().copied().and_then(decompose) else {
        return false;
    };

    let stripped = match coda {
        // No coda left to take: the vowel goes, leaving the bare onset.
        0 => CHOSEONG[onset],
        // A cluster only loses its second half.
        _ => match split(JONGSEONG[coda - 1]) {
            Some((first, _)) => compose(onset, vowel, jongseong_index(first).unwrap_or(0)),
            None => compose(onset, vowel, 0),
        },
    };

    replace_last(buf, stripped);
    true
}

fn replace_last(buf: &mut Vec<char>, ch: char) {
    buf.pop();
    buf.push(ch);
}

fn compose(onset: usize, vowel: usize, coda: usize) -> char {
    let code = SYLLABLE_BASE + ((onset * 21 + vowel) * 28 + coda) as u32;
    // Indices come from the tables above and are in range by construction.
    char::from_u32(code).expect("hangul syllable index out of range")
}

/// Split a precomposed syllable back into (onset, vowel, coda) indices.
fn decompose(ch: char) -> Option<(usize, usize, usize)> {
    let code = ch as u32;
    if !(SYLLABLE_BASE..=SYLLABLE_LAST).contains(&code) {
        return None;
    }

    let offset = (code - SYLLABLE_BASE) as usize;
    Some((offset / (21 * 28), (offset / 28) % 21, offset % 28))
}

fn choseong_index(ch: char) -> Option<usize> {
    CHOSEONG.iter().position(|&c| c == ch)
}

fn jungseong_index(ch: char) -> Option<usize> {
    JUNGSEONG.iter().position(|&c| c == ch)
}

/// Coda index for `ch`, one higher than its table position because index 0 is
/// reserved for "no coda".
fn jongseong_index(ch: char) -> Option<usize> {
    JONGSEONG.iter().position(|&c| c == ch).map(|i| i + 1)
}

fn combine(first: char, second: char) -> Option<char> {
    CLUSTERS
        .iter()
        .find(|(a, b, _)| *a == first && *b == second)
        .map(|(_, _, cluster)| *cluster)
}

fn split(cluster: char) -> Option<(char, char)> {
    CLUSTERS
        .iter()
        .find(|(_, _, c)| *c == cluster)
        .map(|(first, second, _)| (*first, *second))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pick each jamo in turn, as the palette would.
    fn typed(jamo: &str) -> String {
        let mut buf = Vec::new();
        for ch in jamo.chars() {
            push(&mut buf, ch);
        }
        buf.into_iter().collect()
    }

    #[test]
    fn composes_syllables_arithmetically() {
        assert_eq!(compose(18, 0, 4), '한');
        assert_eq!(compose(0, 18, 8), '글');
        assert_eq!(decompose('한'), Some((18, 0, 4)));
        assert_eq!(decompose('a'), None);
    }

    #[test]
    fn builds_a_syllable_slot_by_slot() {
        assert_eq!(typed("ㄱ"), "ㄱ");
        assert_eq!(typed("ㄱㅏ"), "가");
        assert_eq!(typed("ㄱㅏㅁ"), "감");
    }

    #[test]
    fn a_vowel_takes_the_previous_coda_as_its_onset() {
        // The coda has to move, or 가마 is untypeable.
        assert_eq!(typed("ㄱㅏㅁㅏ"), "가마");
        assert_eq!(typed("ㅎㅏㄴㄱㅜㄱ"), "한국");
        assert_eq!(typed("ㅇㅏㄴㄴㅕㅇ"), "안녕");
    }

    #[test]
    fn cluster_codas_form_and_come_apart_again() {
        assert_eq!(typed("ㅇㅏㄴㅈ"), "앉");
        // Only the second consonant moves to the new syllable.
        assert_eq!(typed("ㅇㅏㄴㅈㅏ"), "안자");
        assert_eq!(typed("ㅇㅣㄹㄱ"), "읽");
    }

    #[test]
    fn a_vowel_with_nothing_waiting_gets_the_silent_onset() {
        assert_eq!(typed("ㅏ"), "아");
        // 가 is closed, so the next vowel starts its own syllable.
        assert_eq!(typed("ㄱㅏㅏ"), "가아");
        assert_eq!(typed("ㅇㅏㄴㄴㅕㅇㅎㅏㅅㅔㅇㅛ"), "안녕하세요");
    }

    #[test]
    fn backspace_removes_one_jamo_at_a_time() {
        let mut buf: Vec<char> = "감".chars().collect();
        assert!(backspace(&mut buf));
        assert_eq!(buf, vec!['가']);
        assert!(backspace(&mut buf));
        assert_eq!(buf, vec!['ㄱ']);
        // A bare jamo is one character already; the caller deletes it.
        assert!(!backspace(&mut buf));
    }

    #[test]
    fn backspace_takes_a_cluster_apart_before_the_syllable() {
        let mut buf: Vec<char> = "앉".chars().collect();
        assert!(backspace(&mut buf));
        assert_eq!(buf, vec!['안']);
    }

    #[test]
    fn non_hangul_is_left_alone() {
        let mut buf: Vec<char> = "ok".chars().collect();
        assert!(!backspace(&mut buf));
        assert_eq!(buf, vec!['o', 'k']);

        // A consonant after a non-syllable just lands next to it.
        assert_eq!(typed("ㄱ"), "ㄱ");
    }
}
