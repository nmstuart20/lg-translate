//! Loading and running one Marian translation direction.

use anyhow::{bail, Context, Error as E, Result};
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::generation::LogitsProcessor;
use candle_transformers::models::marian;
use std::{fs, path::Path};
use tokenizers::Tokenizer;

use crate::input::Script;
use crate::pairs::{self, PairSpec};

pub struct Translator {
    model: marian::MTModel,
    source_tokenizer: Tokenizer,
    target_tokenizer: Tokenizer,
    config: marian::Config,
    device: Device,
    max_tokens: usize,
    /// Only used to read the source's punctuation; see [`is_terminator`].
    script: Script,
}

impl Translator {
    /// Load the pair from `pair_dir` (`model/<pair-id>/`).
    pub fn load(pair: &PairSpec, pair_dir: &Path, max_tokens: usize) -> Result<Self> {
        let config_path = pair_dir.join(pairs::CONFIG_FILE);
        let src_tok_path = pair_dir.join(pairs::SOURCE_TOKENIZER_FILE);
        let dst_tok_path = pair_dir.join(pairs::TARGET_TOKENIZER_FILE);
        let weights_path = pair_dir.join(pairs::WEIGHTS_FILE);

        for path in [&weights_path, &config_path, &src_tok_path, &dst_tok_path] {
            if !path.exists() {
                bail!(
                    "Required file is missing: {}\nRun `translate --download-model {}` first.",
                    path.display(),
                    pair.id
                );
            }
        }

        let device = Device::Cpu;

        // `marian::Config` derives Deserialize, so every OPUS-MT pair is
        // described by its own `config.json`. Nothing here is pair-specific,
        // which is what lets the table in `pairs.rs` stay data-only.
        let config: marian::Config = serde_json::from_str(
            &fs::read_to_string(&config_path)
                .with_context(|| format!("failed to read {}", config_path.display()))?,
        )
        .with_context(|| format!("failed to parse {}", config_path.display()))?;

        let source_tokenizer = Tokenizer::from_file(&src_tok_path)
            .map_err(E::msg)
            .with_context(|| format!("failed to load {}", src_tok_path.display()))?;
        let target_tokenizer = Tokenizer::from_file(&dst_tok_path)
            .map_err(E::msg)
            .with_context(|| format!("failed to load {}", dst_tok_path.display()))?;

        let vb = load_weights(&weights_path, &device)?;
        let model = marian::MTModel::new(&config, vb)?;

        Ok(Self {
            model,
            source_tokenizer,
            target_tokenizer,
            config,
            device,
            max_tokens,
            script: pair.script,
        })
    }

    /// Translate one line, a sentence at a time.
    ///
    /// Marian is a sentence-level model. Given several sentences at once it
    /// frequently translates the first and silently drops the rest, so the
    /// line is split and each sentence is fed through on its own.
    pub fn translate(&mut self, text: &str) -> Result<String> {
        let mut translated = Vec::new();

        for sentence in split_sentences(text, self.script) {
            // Punctuation alone has nothing to translate, and the model
            // answers it with invented text, so it is passed through as typed.
            if sentence.chars().any(char::is_alphanumeric) {
                translated.push(self.translate_sentence(sentence)?);
            } else {
                translated.push(sentence.to_string());
            }
        }

        Ok(translated.join(" "))
    }

    fn translate_sentence(&mut self, text: &str) -> Result<String> {
        // The model keeps self- and cross-attention KV caches across forward passes.
        // They belong to the previous line, so they must be cleared before starting a
        // new one; otherwise the decoder attends to the old sentence and immediately
        // emits "." followed by EOS. Resetting here (rather than after generating)
        // also recovers cleanly if a previous translation errored partway through.
        self.model.reset_kv_cache();

        let mut source_ids = self
            .source_tokenizer
            .encode(text, true)
            .map_err(E::msg)?
            .get_ids()
            .to_vec();

        source_ids.push(self.config.eos_token_id);

        let source = Tensor::new(source_ids.as_slice(), &self.device)?.unsqueeze(0)?;
        let encoder_output = self.model.encoder().forward(&source, 0)?;

        let mut token_ids = vec![self.config.decoder_start_token_id];

        // Greedy decoding. For a utility translator this is deterministic and cheap.
        let mut logits_processor = LogitsProcessor::new(0, None, None);

        for index in 0..self.max_tokens {
            let context_size = if index >= 1 { 1 } else { token_ids.len() };
            let start_pos = token_ids.len().saturating_sub(context_size);

            let decoder_input = Tensor::new(&token_ids[start_pos..], &self.device)?.unsqueeze(0)?;

            let logits = self
                .model
                .decode(&decoder_input, &encoder_output, start_pos)?
                .squeeze(0)?;

            let logits = logits.get(logits.dim(0)? - 1)?;
            let token = logits_processor.sample(&logits)?;
            token_ids.push(token);

            if token == self.config.eos_token_id || token == self.config.forced_eos_token_id {
                break;
            }
        }

        // Remove decoder start/end control tokens before turning IDs back into text.
        let output_ids: Vec<u32> = token_ids
            .into_iter()
            .filter(|id| {
                *id != self.config.decoder_start_token_id
                    && *id != self.config.eos_token_id
                    && *id != self.config.forced_eos_token_id
            })
            .collect();

        self.target_tokenizer
            .decode(&output_ids, true)
            .map_err(E::msg)
    }
}

/// Ends a sentence in every script here. Greek adds ASCII `;`, which is its
/// question mark -- the palette offers no punctuation, so `;` is what actually
/// gets typed. Elsewhere it is a semicolon and must not split.
fn is_terminator(ch: char, script: Script) -> bool {
    matches!(ch, '.' | '!' | '?' | '…' | '\u{037E}') || (script == Script::Greek && ch == ';')
}

/// Stays with the sentence it closes, so `(Nej.)` and `"Nej."` are not cut
/// before their final bracket.
fn is_closing(ch: char) -> bool {
    matches!(ch, '"' | '\'' | ')' | ']' | '}' | '»' | '”' | '’' | '›')
}

/// Could plausibly open the next sentence. A lowercase letter could not, which
/// is what keeps abbreviations such as "t.ex. äpplen" in one piece.
fn starts_sentence(ch: char) -> bool {
    ch.is_uppercase()
        || ch.is_numeric()
        || matches!(
            ch,
            '"' | '\'' | '(' | '[' | '{' | '«' | '“' | '‘' | '‹' | '-' | '–' | '—'
        )
}

/// Whether the `.` at `dot` follows a single letter, as in "J. R. R. Tolkien".
/// A real sentence essentially never ends in a lone letter.
fn is_lone_initial(chars: &[(usize, char)], dot: usize) -> bool {
    if dot == 0 || !chars[dot - 1].1.is_alphabetic() {
        return false;
    }
    dot < 2 || !chars[dot - 2].1.is_alphanumeric()
}

/// Split `text` into sentences, trimmed and in order.
///
/// The rule is deliberately conservative: a split needs a terminator, then
/// whitespace, then something that could open a sentence. Anything less is
/// left joined, because a missed split only costs some quality while a wrong
/// one cuts a sentence in half and mistranslates both halves. Decimals,
/// abbreviations and initials all fail one of the three conditions.
fn split_sentences(text: &str, script: Script) -> Vec<&str> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut sentences = Vec::new();
    let mut start = 0;
    let mut i = 0;

    while i < chars.len() {
        let (idx, ch) = chars[i];

        if !is_terminator(ch, script) || (ch == '.' && is_lone_initial(&chars, i)) {
            i += 1;
            continue;
        }

        // Take the whole run of terminators and closing brackets, so "Vad?!"
        // ends where it looks like it ends.
        let mut end = idx + ch.len_utf8();
        let mut after = i + 1;
        while let Some(&(next_idx, next_ch)) = chars.get(after) {
            if !is_terminator(next_ch, script) && !is_closing(next_ch) {
                break;
            }
            end = next_idx + next_ch.len_utf8();
            after += 1;
        }

        // ...then whitespace...
        let mut next = after;
        while chars.get(next).is_some_and(|(_, ch)| ch.is_whitespace()) {
            next += 1;
        }

        // ...then an opening. Running off the end means the terminator closed
        // the line, which the trailing push below already covers.
        match chars.get(next) {
            Some(&(next_idx, next_ch)) if next > after && starts_sentence(next_ch) => {
                sentences.push(text[start..end].trim());
                start = next_idx;
                i = next;
            }
            _ => i = after,
        }
    }

    let tail = text[start..].trim();
    if !tail.is_empty() {
        sentences.push(tail);
    }

    sentences
}

fn load_weights(path: &Path, device: &Device) -> Result<VarBuilder<'static>> {
    // SAFETY:
    // Candle memory-maps the safetensors file. The file is only read, not modified,
    // and remains present on disk for the lifetime of the model.
    unsafe {
        VarBuilder::from_mmaped_safetensors(&[path], DType::F32, device)
            .with_context(|| format!("failed to load {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(text: &str) -> Vec<&str> {
        split_sentences(text, Script::Latin)
    }

    #[test]
    fn splits_on_sentence_boundaries() {
        assert_eq!(
            split("Vår kära hund älskar godis. Hon är en snäll flicka."),
            ["Vår kära hund älskar godis.", "Hon är en snäll flicka."]
        );
        assert_eq!(split("Vad?! Jag vet inte."), ["Vad?!", "Jag vet inte."]);
        assert_eq!(
            split("Hon gick. 5 minuter senare kom han."),
            ["Hon gick.", "5 minuter senare kom han."]
        );
    }

    #[test]
    fn single_sentences_are_passed_through_whole() {
        assert_eq!(
            split("Hon är en snäll flicka."),
            ["Hon är en snäll flicka."]
        );
        // No terminator at all, and a trailing one, are the same one sentence.
        assert_eq!(split("Hon är en snäll flicka"), ["Hon är en snäll flicka"]);
        assert_eq!(split("  Hej!  "), ["Hej!"]);
    }

    #[test]
    fn does_not_split_inside_numbers_or_abbreviations() {
        // No whitespace after the dot.
        assert_eq!(
            split("Det kostar 3.5 miljoner."),
            ["Det kostar 3.5 miljoner."]
        );
        // Whitespace, but what follows cannot open a sentence.
        assert_eq!(
            split("Jag gillar frukt, t.ex. äpplen och päron."),
            ["Jag gillar frukt, t.ex. äpplen och päron."]
        );
        // A lone letter before the dot is an initial, not an ending.
        assert_eq!(
            split("J. R. R. Tolkien skrev böcker."),
            ["J. R. R. Tolkien skrev böcker."]
        );
    }

    #[test]
    fn quotes_and_brackets_close_with_their_sentence() {
        assert_eq!(split("\"Nej.\" Han gick."), ["\"Nej.\"", "Han gick."]);
        assert_eq!(split("(Nej.) Han gick."), ["(Nej.)", "Han gick."]);
        // An opening quote is a plausible start, so the split still happens.
        assert_eq!(
            split("Han gick. \"Nej\", sa hon."),
            ["Han gick.", "\"Nej\", sa hon."]
        );
    }

    #[test]
    fn semicolon_splits_only_for_greek() {
        // In Greek ';' is the question mark; anywhere else it joins clauses.
        assert_eq!(
            split_sentences("Πώς είσαι; Καλά.", Script::Greek),
            ["Πώς είσαι;", "Καλά."]
        );
        assert_eq!(
            split_sentences("Han kom; Hon gick.", Script::Latin),
            ["Han kom; Hon gick."]
        );
    }

    #[test]
    fn punctuation_only_input_stays_one_piece() {
        // translate() passes these through untouched rather than asking the
        // model to invent something for them.
        assert_eq!(split("..."), ["..."]);
        assert!(split("   ").is_empty());
    }
}
