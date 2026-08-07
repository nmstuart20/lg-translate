//! Loading and running one Marian translation direction.

use anyhow::{bail, Context, Error as E, Result};
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::generation::LogitsProcessor;
use candle_transformers::models::marian;
use std::{fs, path::Path};
use tokenizers::Tokenizer;

use crate::pairs::{self, PairSpec};

pub struct Translator {
    model: marian::MTModel,
    source_tokenizer: Tokenizer,
    target_tokenizer: Tokenizer,
    config: marian::Config,
    device: Device,
    max_tokens: usize,
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
        })
    }

    pub fn translate(&mut self, text: &str) -> Result<String> {
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

fn load_weights(path: &Path, device: &Device) -> Result<VarBuilder<'static>> {
    // SAFETY:
    // Candle memory-maps the safetensors file. The file is only read, not modified,
    // and remains present on disk for the lifetime of the model.
    unsafe {
        VarBuilder::from_mmaped_safetensors(&[path], DType::F32, device)
            .with_context(|| format!("failed to load {}", path.display()))
    }
}
