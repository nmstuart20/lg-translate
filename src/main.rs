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

use input::{
    editor::{self, Editor, Line},
    Script,
};
use model::Translator;
use pairs::PairSpec;

#[derive(Parser, Debug)]
#[command(name = "translate", version, about = "Small offline translator")]
struct Args {
    /// Directory that holds model subdirectories
    /// defaults to a "model" folder beside executable
    #[arg(long)]
    model_dir: Option<PathBuf>,

    /// Download the given pair (e.g. "de-en") and exit.
    /// Pass "all" or omit the value for every pair.
    #[arg(long, value_name = "PAIR", num_args = 0..=1, default_missing_value = "all")]
    download_model: Option<String>,

    /// Language pair the cli with. Without it, the prompt asks for one.
    #[arg(long, value_name = "PAIR")]
    lang: Option<String>,

    /// Maximum number of generated tokens per input line.
    #[arg(long, default_value_t = 256)]
    max_tokens: usize,
}

struct Session {
    model_dir: PathBuf,
    max_tokens: usize,
    /// The active language being translated
    active: &'static PairSpec,
    /// Loaded lazily so startup only pays for the pairs actually used.
    loaded: HashMap<&'static str, Translator>,
}

impl Session {
    fn new(model_dir: PathBuf, max_tokens: usize, active: &'static PairSpec) -> Self {
        Self {
            model_dir,
            max_tokens,
            active,
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
}

fn default_model_dir() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("could not determine executable path")?;
    let parent = exe.parent().context("executable has no parent directory")?;
    Ok(parent.join("model"))
}

fn resolve_pair(id: &str) -> Result<&'static PairSpec> {
    pairs::find(id).with_context(|| {
        format!(
            "unknown language pair {id:?} (known: {})",
            pairs::ids().join(", ")
        )
    })
}

/// The pair table, flagging the active one and anything still missing from
/// disk. Shown both when choosing a pair and by `/help`.
fn print_pairs(model_dir: &Path, active: Option<&PairSpec>) {
    for pair in pairs::all() {
        let marker = match active {
            Some(active) if active.id == pair.id => "*",
            _ => " ",
        };
        let state = if download::is_ready(pair, model_dir) {
            ""
        } else {
            "  (not downloaded)"
        };
        println!("  {marker} {:<6} {}{state}", pair.id, pair.label);
    }
}

/// Ask user which language pair to use if no pair was passed as an argument
fn select_pair(editor: &mut Editor, model_dir: &Path) -> Result<Option<&'static PairSpec>> {
    println!();
    println!("lg-translator");
    println!();
    println!("Select a language pair:");
    print_pairs(model_dir, None);
    println!();

    loop {
        // Latin, so this reads as a plain prompt -- the palette belongs to the
        // pair being chosen here, which is not known yet.
        let line = match editor.read_line("pair> ", Script::Latin)? {
            Line::Text(line) => line,
            Line::Eof | Line::Interrupted => {
                println!();
                return Ok(None);
            }
        };

        match line.trim() {
            "" => continue,
            "/quit" | "/exit" => return Ok(None),
            id => match pairs::find(id) {
                Some(pair) => return Ok(Some(pair)),
                None => eprintln!("unknown pair {id:?} (known: {})", pairs::ids().join(", ")),
            },
        }
    }
}

fn print_banner(session: &Session) {
    println!();
    println!("lg-translator");
    println!("{}", session.active.label);
    if editor::palette_available(session.active.script) {
        println!("Press Up or Down to browse the alphabet and pick letters.");
    }
    println!("Enter one line at a time. Press Ctrl-C to exit.");
    println!("Commands: /lang  /help  /clear  /quit");
    println!();
}

fn print_help(session: &Session) {
    println!("Type a line and press Enter to translate it.");
    println!();
    println!("Every line goes to the active pair (*), whatever it is written in.");
    println!("Switch pairs with /lang before typing in another language.");
    println!();

    println!("Pairs:");
    print_pairs(&session.model_dir, Some(session.active));
    println!();

    println!("/lang <pair>    switch the active pair");
    println!("/clear          clear the terminal");
    println!("/quit           exit");
    println!("Ctrl-C          exit immediately");
    println!();

    if editor::palette_available(session.active.script) {
        println!("Picking letters:");
        println!("  Up or Down     open the alphabet under the prompt");
        println!("  arrow keys     move the highlight around it");
        println!("  Enter          insert the highlighted letter");
        println!("  Esc            close it, giving Enter back to the prompt");
        println!("  typing         also closes it, so commands still work");
        println!();
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
            println!("Active pair: {}\n", pair.label);
        }
        None => eprintln!(
            "unknown pair {arg:?} (known: {})\n",
            pairs::ids().join(", ")
        ),
    }
}

fn handle_line(session: &mut Session, line: &str) {
    let pair = session.active;

    match session.translator_for(pair) {
        Ok(translator) => match translator.translate(line) {
            Ok(translated) => println!("{translated}\n"),
            Err(err) => eprintln!("translation error: {err:#}\n"),
        },
        Err(err) => eprintln!("could not load {}: {err:#}\n", pair.label),
    }
}

fn repl(model_dir: PathBuf, max_tokens: usize, lang: Option<&'static PairSpec>) -> Result<()> {
    let mut editor = Editor::new();

    let active = match lang {
        Some(pair) => pair,
        None => match select_pair(&mut editor, &model_dir)? {
            Some(pair) => pair,
            // Gave up at the selection prompt; nothing was loaded.
            None => return Ok(()),
        },
    };

    let mut session = Session::new(model_dir, max_tokens, active);
    print_banner(&session);

    loop {
        // The palette follows the active pair, since that is the script the
        // next line is expected to be in.
        let line = match editor.read_line("> ", session.active.script)? {
            Line::Text(line) => line,
            // EOF also exits cleanly (e.g. Ctrl-Z then Enter on Windows).
            Line::Eof | Line::Interrupted => {
                println!();
                break;
            }
        };

        let input = line.trim();
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

    let lang = args.lang.as_deref().map(resolve_pair).transpose()?;

    repl(model_dir, args.max_tokens, lang)
}
