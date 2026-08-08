//! One-time setup: fetch each pair's weights, config and tokenizers.

use anyhow::{bail, Context, Result};
use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::Command,
};

use crate::pairs::{self, PairSpec, TokenizerSource};

/// The converter is embedded so a copied-around executable can still set up a
/// new pair without the source tree next to it.
const CONVERTER: &str = include_str!("../tools/convert_model.py");
const CONVERTER_NAME: &str = "convert_model.py";

const HUB_ENDPOINT: &str = "https://huggingface.co";

/// A Hub repository at a pinned revision -- everything needed to build the
/// download URL for one of its files.
struct Repo {
    id: &'static str,
    revision: &'static str,
}

impl Repo {
    fn new(id: &'static str, revision: Option<&'static str>) -> Self {
        Repo {
            id,
            revision: revision.unwrap_or("main"),
        }
    }

    fn url(&self, file: &str) -> String {
        // A revision is one path segment, so the slashes in the likes of
        // `refs/pr/4` have to be escaped rather than passed through.
        let revision = self.revision.replace('/', "%2F");
        format!("{HUB_ENDPOINT}/{}/resolve/{revision}/{file}", self.id)
    }
}

pub fn download(pair: &PairSpec, model_dir: &Path) -> Result<()> {
    let pair_dir = model_dir.join(pair.id);
    fs::create_dir_all(&pair_dir)?;

    println!("Downloading {} ({})...", pair.label, pair.model_repo);
    println!("Destination: {}", pair_dir.display());

    let model_repo = Repo::new(pair.model_repo, pair.model_revision);

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
            let tokenizer_repo = Repo::new(repo, None);
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

fn fetch(repo: &Repo, name: &str, pair_dir: &Path) -> Result<PathBuf> {
    fetch_as(repo, name, pair_dir, name)
}

fn fetch_as(repo: &Repo, remote_name: &str, pair_dir: &Path, local_name: &str) -> Result<PathBuf> {
    let url = repo.url(remote_name);
    let mut response = ureq::get(&url)
        .call()
        .with_context(|| format!("failed to download {url}"))?;

    let total = response
        .headers()
        .get("content-length")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());

    // Download alongside the destination and rename once complete, so an
    // interrupted setup cannot leave a truncated file that `is_ready` accepts.
    let destination = pair_dir.join(local_name);
    let partial = pair_dir.join(format!("{local_name}.part"));
    let mut file = fs::File::create(&partial)
        .with_context(|| format!("failed to create {}", partial.display()))?;

    copy_with_progress(
        response.body_mut().as_reader(),
        &mut file,
        local_name,
        total,
    )
    .with_context(|| format!("failed while downloading {url}"))?;
    drop(file);

    fs::rename(&partial, &destination)
        .with_context(|| format!("failed to write {}", destination.display()))?;

    println!("  {}", destination.display());
    Ok(destination)
}

fn copy_with_progress(
    mut reader: impl Read,
    mut sink: impl Write,
    label: &str,
    total: Option<u64>,
) -> io::Result<()> {
    let mut buffer = vec![0u8; 64 * 1024];
    let mut done: u64 = 0;
    let mut drawn: u64 = 0;

    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        sink.write_all(&buffer[..read])?;
        done += read as u64;

        // Redraw once per megabyte; the weights are the only files big enough
        // for this to show up at all, and the terminal is the slow part.
        if done - drawn >= 1 << 20 {
            drawn = done;
            match total {
                Some(total) if total > 0 => print!(
                    "\r  {label}  {} / {} ({}%)",
                    megabytes(done),
                    megabytes(total),
                    done * 100 / total
                ),
                _ => print!("\r  {label}  {}", megabytes(done)),
            }
            let _ = io::stdout().flush();
        }
    }

    sink.flush()?;
    if drawn > 0 {
        // Clear the progress line so the final path below replaces it.
        print!("\r\x1b[2K");
    }
    Ok(())
}

fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1_000_000.0)
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
