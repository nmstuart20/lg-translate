mod download;
mod input;
mod model;
mod pairs;

use anyhow::{bail, Context, Result};
use clap::Parser;
use std::{
    collections::HashMap,
    io::{self, Write},
    path::{Path, PathBuf},
};

use input::{InputMode, Script};
use model::Translator;
use pairs::PairSpec;

#[derive(Parser, Debug)]
#[command(
    name = "translate",
    version,
    about = "Small offline translator (English -> Spanish, Korean -> English, Russian -> English)"
)]
struct Args {
    /// Directory holding one subdirectory per language pair.
    /// Defaults to a "model" folder beside translate.exe.
    #[arg(long)]
    model_dir: Option<PathBuf>,

    /// Download the given pair ("en-es", "ko-en", "ru-en") and exit.
    /// Pass "all" or omit the value for every pair.
    #[arg(long, value_name = "PAIR", num_args = 0..=1, default_missing_value = "all")]
    download_model: Option<String>,

    /// Language pair to start in.
    #[arg(long, value_name = "PAIR")]
    lang: Option<String>,

    /// Maximum number of generated tokens per input line.
    #[arg(long, default_value_t = 256)]
    max_tokens: usize,
}

struct Session {
    model_dir: PathBuf,
    max_tokens: usize,
    /// Which pair plain-ASCII lines are translated with.
    active: &'static PairSpec,
    input_mode: InputMode,
    /// Loaded lazily so startup only pays for the pairs actually used.
    loaded: HashMap<&'static str, Translator>,
}

impl Session {
    fn new(model_dir: PathBuf, max_tokens: usize, active: &'static PairSpec) -> Self {
        Self {
            model_dir,
            max_tokens,
            active,
            input_mode: default_input_mode(active),
            loaded: HashMap::new(),
        }
    }

    fn translator_for(&mut self, pair: &'static PairSpec) -> Result<&mut Translator> {
        if !self.loaded.contains_key(pair.id) {
            println!("Loading {} model...", pair.label);
            let pair_dir = self.model_dir.join(pair.id);
            let translator = Translator::load(pair, &pair_dir, self.max_tokens)?;
            self.loaded.insert(pair.id, translator);
        }

        Ok(self
            .loaded
            .get_mut(pair.id)
            .expect("translator inserted above"))
    }

    /// Decide which model handles `line`, and what text to hand it.
    ///
    /// A line that already contains Hangul or Cyrillic -- pasted, or typed with
    /// an OS IME -- routes itself. A plain ASCII line belongs to the active
    /// pair, and is transliterated first when that pair reads a non-Latin
    /// script and romanized input is on.
    fn route(&self, line: &str) -> (&'static PairSpec, String) {
        match input::detect(line) {
            Script::Latin => {
                let text = if self.input_mode == InputMode::Roman {
                    input::romanize(self.active.script, line)
                } else {
                    line.to_string()
                };
                (self.active, text)
            }
            script => match pairs::for_script(script) {
                Some(pair) => (pair, line.to_string()),
                // No model for that script; let the active pair try.
                None => (self.active, line.to_string()),
            },
        }
    }
}

/// Romanized input is the default for pairs that read a non-Latin script, since
/// that is the whole point of it; Latin-input pairs have nothing to convert.
fn default_input_mode(pair: &PairSpec) -> InputMode {
    match pair.script {
        Script::Latin => InputMode::Raw,
        _ => InputMode::Roman,
    }
}

fn default_model_dir() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("could not determine executable path")?;
    let parent = exe.parent().context("executable has no parent directory")?;
    Ok(parent.join("model"))
}

fn resolve_pair(id: &str) -> Result<&'static PairSpec> {
    pairs::find(id).with_context(|| {
        let known: Vec<&str> = pairs::all().iter().map(|p| p.id).collect();
        format!("unknown language pair {id:?} (known: {})", known.join(", "))
    })
}

fn print_banner(session: &Session) {
    println!();
    println!("Offline Translator");
    println!(
        "{}  [input: {}]",
        session.active.label,
        session.input_mode.name()
    );
    println!("Enter one line at a time. Press Ctrl-C to exit.");
    println!("Commands: /lang  /input  /help  /clear  /quit");
    println!();
}

fn print_help(session: &Session) {
    println!("Type a line and press Enter to translate it.");
    println!();
    println!("Lines containing Hangul or Cyrillic are routed to the matching model");
    println!("automatically, so pasted text and OS input methods just work.");
    println!("Plain ASCII lines go to the active pair.");
    println!();

    println!("Pairs:");
    for pair in pairs::all() {
        let marker = if pair.id == session.active.id {
            "*"
        } else {
            " "
        };
        let state = if download::is_ready(pair, &session.model_dir) {
            ""
        } else {
            "  (not downloaded)"
        };
        println!("  {marker} {:<6} {}{state}", pair.id, pair.label);
    }
    println!();

    println!("/lang <pair>    switch the active pair");
    println!("/input roman    type Korean or Russian in ASCII (default for those pairs)");
    println!("/input raw      pass typed text through untouched");
    println!("/clear          clear the terminal");
    println!("/quit           exit");
    println!("Ctrl-C          exit immediately");
    println!();

    if session.input_mode == InputMode::Roman {
        match session.active.script {
            Script::Hangul => {
                println!("Romanized Korean uses Revised Romanization:");
                println!("  annyeonghaseyo -> 안녕하세요     gamsahamnida -> 감사합니다");
                println!(
                    "  g/k, d/t, b/p are ㄱ/ㅋ, ㄷ/ㅌ, ㅂ/ㅍ; a hyphen forces a syllable break."
                );
                println!("  Words that are not valid romanization are left as typed.");
                println!();
            }
            Script::Cyrillic => {
                println!("Transliterated Russian:");
                println!("  privet -> привет     shchi -> щи     chay -> чай");
                println!("  zh ж, kh х, ts ц, ch ч, sh ш, shch щ, yu ю, ya я, yo ё");
                println!("  y is ы after a consonant and й after a vowel; j is й.");
                println!("  Quote keys reach the rest: e' -> э, ' -> ь, \" -> ъ");
                println!("    e'to -> это    chitat' -> читать    pod\"ezd -> подъезд");
                println!();
            }
            Script::Latin => {}
        }
    }
}

fn clear_screen() -> Result<()> {
    // ANSI clear-screen sequence works in modern Windows Terminal, PowerShell,
    // Command Prompt, and most other terminals.
    print!("\x1B[2J\x1B[1;1H");
    io::stdout().flush()?;
    Ok(())
}

fn set_lang(session: &mut Session, arg: &str) {
    match pairs::find(arg) {
        Some(pair) => {
            session.active = pair;
            session.input_mode = default_input_mode(pair);
            println!(
                "Active pair: {}  [input: {}]\n",
                pair.label,
                session.input_mode.name()
            );
        }
        None => {
            let known: Vec<&str> = pairs::all().iter().map(|p| p.id).collect();
            eprintln!("unknown pair {arg:?} (known: {})\n", known.join(", "));
        }
    }
}

fn set_input_mode(session: &mut Session, arg: &str) {
    match InputMode::parse(arg) {
        Some(mode) => {
            session.input_mode = mode;
            println!("Input mode: {}\n", mode.name());
        }
        None => eprintln!("unknown input mode {arg:?} (known: raw, roman)\n"),
    }
}

fn handle_line(session: &mut Session, line: &str) {
    let (pair, text) = session.route(line);

    // Show what romanization produced, so a misparse is obvious rather than
    // silently mistranslated.
    if text != line {
        println!("  {text}");
    }

    match session.translator_for(pair) {
        Ok(translator) => match translator.translate(&text) {
            Ok(translated) => println!("{translated}\n"),
            Err(err) => eprintln!("translation error: {err:#}\n"),
        },
        Err(err) => eprintln!("could not load {}: {err:#}\n", pair.label),
    }
}

fn repl(mut session: Session) -> Result<()> {
    print_banner(&session);

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

        let (command, arg) = match input.split_once(char::is_whitespace) {
            Some((command, rest)) => (command, rest.trim()),
            None => (input, ""),
        };

        match command {
            "/quit" | "/exit" => break,
            "/help" => print_help(&session),
            "/clear" => {
                clear_screen()?;
                print_banner(&session);
            }
            "/lang" if !arg.is_empty() => set_lang(&mut session, arg),
            "/lang" => println!("usage: /lang <pair>   (see /help)\n"),
            "/input" if !arg.is_empty() => set_input_mode(&mut session, arg),
            "/input" => println!("usage: /input raw|roman\n"),
            _ => handle_line(&mut session, input),
        }
    }

    Ok(())
}

fn download_pairs(selector: &str, model_dir: &Path) -> Result<()> {
    let selected: Vec<&'static PairSpec> = if selector == "all" {
        pairs::all().iter().collect()
    } else {
        vec![resolve_pair(selector)?]
    };

    // One pair failing -- most often because the conversion step found no
    // Python to run -- should not throw away the pairs that did work, so each
    // is reported and the rest still run.
    let mut failed: Vec<&'static PairSpec> = Vec::new();

    for pair in &selected {
        if let Err(err) = download::download(pair, model_dir) {
            eprintln!("{}: {err:#}\n", pair.label);
            failed.push(pair);
        }
    }

    let ready: Vec<&str> = selected
        .iter()
        .filter(|pair| download::is_ready(pair, model_dir))
        .map(|pair| pair.id)
        .collect();

    if !ready.is_empty() {
        println!("Ready to use: {}", ready.join(", "));
    }

    if failed.is_empty() {
        println!("Model download complete.");
        return Ok(());
    }

    let names: Vec<&str> = failed.iter().map(|pair| pair.id).collect();
    bail!("could not finish setting up: {}", names.join(", "))
}

fn main() -> Result<()> {
    let args = Args::parse();

    let model_dir = match args.model_dir {
        Some(path) => path,
        None => default_model_dir()?,
    };

    if let Some(selector) = args.download_model {
        return download_pairs(&selector, &model_dir);
    }

    let active = match args.lang.as_deref() {
        Some(id) => resolve_pair(id)?,
        None => pairs::default_pair(),
    };

    repl(Session::new(model_dir, args.max_tokens, active))
}
