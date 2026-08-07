use anyhow::{bail, Context, Error as E, Result};
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::generation::LogitsProcessor;
use candle_transformers::models::marian;
use clap::Parser;
use hf_hub::{api::sync::Api, Repo, RepoType};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};
use tokenizers::Tokenizer;

const MODEL_REPO: &str = "Helsinki-NLP/opus-mt-en-es";
const MODEL_REVISION: &str = "refs/pr/4";
const TOKENIZER_REPO: &str = "KeighBee/candle-marian";

const MODEL_FILE: &str = "model.safetensors";
const SOURCE_TOKENIZER_FILE: &str = "tokenizer-marian-base-en-es-en.json";
const TARGET_TOKENIZER_FILE: &str = "tokenizer-marian-base-en-es-es.json";

#[derive(Parser, Debug)]
#[command(
    name = "translate",
    version,
    about = "Small offline English -> Spanish translator"
)]
struct Args {
    /// Directory containing model.safetensors and tokenizer JSON files.
    /// Defaults to a "model" folder beside translate.exe.
    #[arg(long)]
    model_dir: Option<PathBuf>,

    /// Download/update the required model files and exit.
    #[arg(long)]
    download_model: bool,

    /// Maximum number of generated tokens per input line.
    #[arg(long, default_value_t = 256)]
    max_tokens: usize,
}

struct Translator {
    model: marian::MTModel,
    source_tokenizer: Tokenizer,
    target_tokenizer: Tokenizer,
    config: marian::Config,
    device: Device,
    max_tokens: usize,
}

impl Translator {
    fn load(model_dir: &Path, max_tokens: usize) -> Result<Self> {
        let model_path = model_dir.join(MODEL_FILE);
        let src_tok_path = model_dir.join(SOURCE_TOKENIZER_FILE);
        let dst_tok_path = model_dir.join(TARGET_TOKENIZER_FILE);

        for path in [&model_path, &src_tok_path, &dst_tok_path] {
            if !path.exists() {
                bail!(
                    "Required file is missing: {}\nRun `translate.exe --download-model` first.",
                    path.display()
                );
            }
        }

        println!("Loading translation model...");
        let device = Device::Cpu;
        let config = marian::Config::opus_mt_en_es();

        let source_tokenizer =
            Tokenizer::from_file(&src_tok_path).map_err(E::msg).with_context(|| {
                format!("failed to load {}", src_tok_path.display())
            })?;
        let target_tokenizer =
            Tokenizer::from_file(&dst_tok_path).map_err(E::msg).with_context(|| {
                format!("failed to load {}", dst_tok_path.display())
            })?;

        // SAFETY:
        // Candle memory-maps the safetensors file. The file is only read, not modified,
        // and remains present on disk for the lifetime of the model.
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&[&model_path], DType::F32, &device)
                .with_context(|| format!("failed to load {}", model_path.display()))?
        };

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

    fn translate(&mut self, text: &str) -> Result<String> {
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

            let decoder_input =
                Tensor::new(&token_ids[start_pos..], &self.device)?.unsqueeze(0)?;

            let logits = self
                .model
                .decode(&decoder_input, &encoder_output, start_pos)?
                .squeeze(0)?;

            let logits = logits.get(logits.dim(0)? - 1)?;
            let token = logits_processor.sample(&logits)?;
            token_ids.push(token);

            if token == self.config.eos_token_id
                || token == self.config.forced_eos_token_id
            {
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

fn default_model_dir() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("could not determine executable path")?;
    let parent = exe
        .parent()
        .context("executable has no parent directory")?;
    Ok(parent.join("model"))
}

fn download_model(model_dir: &Path) -> Result<()> {
    fs::create_dir_all(model_dir)?;

    println!("Downloading English -> Spanish model...");
    println!("Destination: {}", model_dir.display());

    let api = Api::new().context("failed to initialize Hugging Face client")?;

    let model_repo = api.repo(Repo::with_revision(
        MODEL_REPO.to_string(),
        RepoType::Model,
        MODEL_REVISION.to_string(),
    ));
    let tokenizer_repo = api.model(TOKENIZER_REPO.to_string());

    let downloads = [
        (
            model_repo.get(MODEL_FILE)?,
            model_dir.join(MODEL_FILE),
        ),
        (
            tokenizer_repo.get(SOURCE_TOKENIZER_FILE)?,
            model_dir.join(SOURCE_TOKENIZER_FILE),
        ),
        (
            tokenizer_repo.get(TARGET_TOKENIZER_FILE)?,
            model_dir.join(TARGET_TOKENIZER_FILE),
        ),
    ];

    for (cached, destination) in downloads {
        fs::copy(&cached, &destination).with_context(|| {
            format!(
                "failed copying {} to {}",
                cached.display(),
                destination.display()
            )
        })?;
        println!("  {}", destination.display());
    }

    println!("\nModel download complete.");
    Ok(())
}

fn print_banner() {
    println!();
    println!("Offline Translator");
    println!("English -> Spanish");
    println!("Enter one line at a time. Press Ctrl-C to exit.");
    println!("Commands: /help  /clear  /quit");
    println!();
}

fn clear_screen() -> Result<()> {
    // ANSI clear-screen sequence works in modern Windows Terminal, PowerShell,
    // Command Prompt, and most other terminals.
    print!("\x1B[2J\x1B[1;1H");
    io::stdout().flush()?;
    Ok(())
}

fn repl(mut translator: Translator) -> Result<()> {
    print_banner();

    let stdin = io::stdin();

    loop {
        print!("> ");
        io::stdout().flush()?;

        let mut input = String::new();
        let bytes = stdin.read_line(&mut input)?;

        // EOF also exits cleanly (e.g. Ctrl-Z then Enter on Windows).
        if bytes == 0 {
            println!();
            break;
        }

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        match input {
            "/quit" | "/exit" => break,
            "/help" => {
                println!("Type English text and press Enter to translate it.");
                println!("/clear  clear the terminal");
                println!("/quit   exit");
                println!("Ctrl-C  exit immediately");
                println!();
            }
            "/clear" => {
                clear_screen()?;
                print_banner();
            }
            _ => match translator.translate(input) {
                Ok(translated) => println!("{translated}\n"),
                Err(err) => eprintln!("translation error: {err:#}\n"),
            },
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    let args = Args::parse();
    let model_dir = match args.model_dir {
        Some(path) => path,
        None => default_model_dir()?,
    };

    if args.download_model {
        return download_model(&model_dir);
    }

    let translator = Translator::load(&model_dir, args.max_tokens)?;
    repl(translator)
}