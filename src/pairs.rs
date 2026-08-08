//! The set of translation directions this build knows how to fetch and run.
//!
//! Candle only ships hardcoded `marian::Config` constructors for a handful of
//! OPUS-MT pairs, and none for most of the ones here. `marian::Config` derives
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
    pub script: Script,
}

/// The order the selection prompt and `/help` list them in.
static PAIRS: &[PairSpec] = &[
    PairSpec {
        id: "en-es",
        label: "English -> Spanish",
        model_repo: "Helsinki-NLP/opus-mt-en-es",
        model_revision: Some("refs/pr/4"),
        tokenizer: TokenizerSource::Prebuilt {
            repo: "KeighBee/candle-marian",
            source: "tokenizer-marian-base-en-es-en.json",
            target: "tokenizer-marian-base-en-es-es.json",
        },
        script: Script::Latin,
    },
    PairSpec {
        id: "de-en",
        label: "German -> English",
        model_repo: "Helsinki-NLP/opus-mt-de-en",
        model_revision: None,
        tokenizer: TokenizerSource::Convert,
        script: Script::Latin,
    },
    PairSpec {
        id: "el-en",
        label: "Greek -> English",
        model_repo: "Helsinki-NLP/opus-mt-grk-en",
        model_revision: None,
        tokenizer: TokenizerSource::Convert,
        script: Script::Greek,
    },
    PairSpec {
        id: "es-en",
        label: "Spanish -> English",
        model_repo: "Helsinki-NLP/opus-mt-es-en",
        model_revision: None,
        tokenizer: TokenizerSource::Convert,
        script: Script::Latin,
    },
    PairSpec {
        id: "ru-en",
        label: "Russian -> English",
        model_repo: "Helsinki-NLP/opus-mt-ru-en",
        model_revision: None,
        tokenizer: TokenizerSource::Convert,
        script: Script::Cyrillic,
    },
    PairSpec {
        id: "sv-en",
        label: "Swedish -> English",
        model_repo: "Helsinki-NLP/opus-mt-sv-en",
        model_revision: None,
        tokenizer: TokenizerSource::Convert,
        script: Script::Latin,
    },
];

pub fn all() -> &'static [PairSpec] {
    PAIRS
}

pub fn find(id: &str) -> Option<&'static PairSpec> {
    PAIRS.iter().find(|p| p.id.eq_ignore_ascii_case(id))
}

/// Every known id, for the messages that have to name them all.
pub fn ids() -> Vec<&'static str> {
    PAIRS.iter().map(|p| p.id).collect()
}
