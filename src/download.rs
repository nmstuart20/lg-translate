//! One-time setup: fetch each pair's weights, config and tokenizers.

use anyhow::{bail, Context, Result};
use hf_hub::{api::sync::Api, Repo, RepoType};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::pairs::{self, PairSpec, TokenizerSource};

/// The converter is embedded so a copied-around executable can still set up a
/// new pair without the source tree next to it.
const CONVERTER: &str = include_str!("../tools/convert_model.py");
const CONVERTER_NAME: &str = "convert_model.py";

pub fn download(pair: &PairSpec, model_dir: &Path) -> Result<()> {
    let pair_dir = model_dir.join(pair.id);
    fs::create_dir_all(&pair_dir)?;

    println!("Downloading {} ({})...", pair.label, pair.model_repo);
    println!("Destination: {}", pair_dir.display());

    let api = Api::new().context("failed to initialize Hugging Face client")?;

    let model_repo = match pair.model_revision {
        Some(revision) => api.repo(Repo::with_revision(
            pair.model_repo.to_string(),
            RepoType::Model,
            revision.to_string(),
        )),
        None => api.model(pair.model_repo.to_string()),
    };

    fetch(&model_repo, pairs::CONFIG_FILE, &pair_dir)?;

    // Several OPUS-MT repos only ever published the PyTorch pickle, which has
    // to be converted below before it can be loaded.
    let mut needs_conversion = false;
    if fetch(&model_repo, pairs::WEIGHTS_FILE, &pair_dir).is_err() {
        fetch(&model_repo, pairs::PYTORCH_WEIGHTS_FILE, &pair_dir).with_context(|| {
            format!(
                "{} has neither {} nor {}",
                pair.model_repo,
                pairs::WEIGHTS_FILE,
                pairs::PYTORCH_WEIGHTS_FILE
            )
        })?;
        needs_conversion = true;
    }

    match pair.tokenizer {
        TokenizerSource::Prebuilt {
            repo,
            source,
            target,
        } => {
            let tokenizer_repo = api.model(repo.to_string());
            fetch_as(
                &tokenizer_repo,
                source,
                &pair_dir,
                pairs::SOURCE_TOKENIZER_FILE,
            )?;
            fetch_as(
                &tokenizer_repo,
                target,
                &pair_dir,
                pairs::TARGET_TOKENIZER_FILE,
            )?;
        }
        TokenizerSource::Convert => {
            for name in pairs::SPM_FILES {
                fetch(&model_repo, name, &pair_dir)?;
            }
            needs_conversion = true;
        }
    }

    if needs_conversion {
        convert(&pair_dir)?;
    }

    println!("{} ready.\n", pair.label);
    Ok(())
}

fn fetch(repo: &hf_hub::api::sync::ApiRepo, name: &str, pair_dir: &Path) -> Result<PathBuf> {
    fetch_as(repo, name, pair_dir, name)
}

fn fetch_as(
    repo: &hf_hub::api::sync::ApiRepo,
    remote_name: &str,
    pair_dir: &Path,
    local_name: &str,
) -> Result<PathBuf> {
    let cached = repo.get(remote_name)?;
    let destination = pair_dir.join(local_name);

    fs::copy(&cached, &destination).with_context(|| {
        format!(
            "failed copying {} to {}",
            cached.display(),
            destination.display()
        )
    })?;

    println!("  {}", destination.display());
    Ok(destination)
}

/// Run the bundled converter, which re-saves legacy PyTorch weights as
/// safetensors and turns `.spm` files into `tokenizer.json`.
///
/// This is the one step that needs Python, and only at setup. If it cannot run
/// here, the script is left on disk so it can be run by hand -- possibly on a
/// different machine, since everything it produces is portable.
fn convert(pair_dir: &Path) -> Result<()> {
    let script = pair_dir.join(CONVERTER_NAME);
    fs::write(&script, CONVERTER)
        .with_context(|| format!("failed to write {}", script.display()))?;

    println!("Preparing model files (needs Python)...");

    for interpreter in ["python3", "python"] {
        match Command::new(interpreter)
            .arg(&script)
            .arg(pair_dir)
            .status()
        {
            Ok(status) if status.success() => return Ok(()),
            // Ran, but failed -- usually missing transformers. Report rather
            // than silently trying the next interpreter name.
            Ok(_) => break,
            // Interpreter not on PATH; try the next name.
            Err(_) => continue,
        }
    }

    bail!(
        "could not prepare the model files automatically.\n\n\
         Install the dependencies and run the bundled script by hand:\n\
         \x20   pip install \"transformers[sentencepiece]\" protobuf\n\
         \x20   pip install torch --index-url https://download.pytorch.org/whl/cpu\n\
         \x20   python {} {}\n\n\
         It is idempotent, so re-running it is safe. Everything it writes is\n\
         portable, so it can also be run on another machine and the resulting\n\
         directory copied across.",
        script.display(),
        pair_dir.display(),
    )
}

/// Whether everything needed to load `pair` is already on disk.
pub fn is_ready(pair: &PairSpec, model_dir: &Path) -> bool {
    let pair_dir = model_dir.join(pair.id);

    pair_dir.join(pairs::WEIGHTS_FILE).exists()
        && pair_dir.join(pairs::CONFIG_FILE).exists()
        && pair_dir.join(pairs::SOURCE_TOKENIZER_FILE).exists()
        && pair_dir.join(pairs::TARGET_TOKENIZER_FILE).exists()
}
