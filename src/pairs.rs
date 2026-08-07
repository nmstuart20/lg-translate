//! The set of translation directions this build knows how to fetch and run.
//!
//! Candle only ships hardcoded `marian::Config` constructors for a handful of
//! OPUS-MT pairs, and none for ko-en or ru-en. `marian::Config` derives
//! `serde::Deserialize` though, so we download each model's `config.json`
//! instead of hardcoding anything -- adding a pair is then a table entry.

use crate::input::Script;

/// Local file names inside `model/<pair-id>/`.
pub const CONFIG_FILE: &str = "config.json";
pub const SOURCE_TOKENIZER_FILE: &str = "tokenizer-source.json";
pub const TARGET_TOKENIZER_FILE: &str = "tokenizer-target.json";

/// The only weight format loaded at runtime; Candle memory-maps it.
pub const WEIGHTS_FILE: &str = "model.safetensors";

/// Fallback when a repo never got a safetensors conversion. These files are
/// converted to [`WEIGHTS_FILE`] during setup rather than loaded directly:
/// the Helsinki-NLP checkpoints predate PyTorch 1.6 and are raw pickle streams,
/// not the ZIP container Candle's `from_pth` reads.
pub const PYTORCH_WEIGHTS_FILE: &str = "pytorch_model.bin";

/// Files the local tokenizer conversion needs, copied verbatim from the Hub.
pub const SPM_FILES: [&str; 4] = [
    "source.spm",
    "target.spm",
    "vocab.json",
    "tokenizer_config.json",
];

pub enum TokenizerSource {
    /// Someone already published converted `tokenizer.json` files on the Hub.
    Prebuilt {
        repo: &'static str,
        source: &'static str,
        target: &'static str,
    },
    /// Upstream only ships `source.spm`/`target.spm`/`vocab.json`, so the
    /// tokenizers have to be converted locally once (see `tools/`).
    Convert,
}

pub struct PairSpec {
    pub id: &'static str,
    pub label: &'static str,
    pub model_repo: &'static str,
    pub model_revision: Option<&'static str>,
    pub tokenizer: TokenizerSource,
    /// Script the model expects on its input side, used to auto-route lines.
    pub script: Script,
}

static PAIRS: &[PairSpec] = &[
    PairSpec {
        id: "en-es",
        label: "English -> Spanish",
        model_repo: "Helsinki-NLP/opus-mt-en-es",
        // The main branch has no safetensors; this PR branch added them.
        model_revision: Some("refs/pr/4"),
        tokenizer: TokenizerSource::Prebuilt {
            repo: "KeighBee/candle-marian",
            source: "tokenizer-marian-base-en-es-en.json",
            target: "tokenizer-marian-base-en-es-es.json",
        },
        script: Script::Latin,
    },
    PairSpec {
        id: "ko-en",
        label: "Korean -> English",
        model_repo: "Helsinki-NLP/opus-mt-ko-en",
        model_revision: None,
        tokenizer: TokenizerSource::Convert,
        script: Script::Hangul,
    },
    PairSpec {
        id: "ru-en",
        label: "Russian -> English",
        model_repo: "Helsinki-NLP/opus-mt-ru-en",
        model_revision: None,
        tokenizer: TokenizerSource::Convert,
        script: Script::Cyrillic,
    },
];

pub fn all() -> &'static [PairSpec] {
    PAIRS
}

pub fn find(id: &str) -> Option<&'static PairSpec> {
    PAIRS.iter().find(|p| p.id.eq_ignore_ascii_case(id))
}

/// The pair that takes `script` as its input, used when a line auto-routes.
pub fn for_script(script: Script) -> Option<&'static PairSpec> {
    PAIRS.iter().find(|p| p.script == script)
}

pub fn default_pair() -> &'static PairSpec {
    &PAIRS[0]
}
