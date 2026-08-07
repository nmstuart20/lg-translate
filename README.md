# Offline Translator (Rust)

A lightweight language translation utility.

## Setup

All one-time. Budget about 300 MB per pair.

**1. Build**

```bash
cargo build --release
```

**2. Install Python transformers and torch**

`ko-en` and `ru-en` need a one-time conversion step, because their upstream
repos ship their weights and tokenizers in formats this program can't read.

You may need to create a python virtual environement.

```bash
python -m venv venv
source venv/bin/activate
pip install "transformers[sentencepiece]" protobuf
pip install torch --index-url https://download.pytorch.org/whl/cpu
```

**3. Download the models.**

```bash
./lg-translate --download-model ko-en
./lg-translate --download-model ru-en
```

**4. Run it.**

```bash
./lg-translate
```

## Typing Korean and Russian on a US keyboard

Press **Up** or **Down** at the prompt and the active script's alphabet opens
under it. The arrow keys move the highlight, **Enter** drops the highlighted
letter into the line, and **Esc** closes the grid and gives Enter back to the
prompt. There is nothing to memorize — the letters you see are the letters you
get:

```text
> /lang ru-en
Active pair: Russian -> English

> привет
   а  б  в  г  д  е  ё  ж  з  и  й  к  л  м  н  о  п [р] с  т  у  ф  х  ц  ч
   ш  щ  ъ  ы  ь  э  ю  я  А  Б  В  Г  Д  Е  Ё  Ж  З  И  Й  К  Л  М  Н  О  П
   Р  С  Т  У  Ф  Х  Ц  Ч  Ш  Щ  Ъ  Ы  Ь  Э  Ю  Я
  Russian · ↑↓←→ move · Enter insert · Esc close
```

The grid stays hidden until an arrow key asks for it, so commands and English
are typed normally. Typing any character also closes it, which keeps
`/lang ko-en` + Enter working without a detour.

Korean is picked one jamo at a time and composed into syllables as you go, the
way a Korean IME behaves — a consonant picked after a vowel becomes that
syllable's coda, and the next vowel takes it back out to start a new syllable:

```text
ㅎ  ㅏ  ㄴ            ->  한
ㄱ  ㅏ  ㅁ  ㅏ        ->  가마
ㅇ ㅏ ㄴ ㄴ ㅕ ㅇ     ->  안녕
```

Backspace removes one jamo rather than the whole block (감 → 가 → ㄱ), so a
wrong coda costs one keystroke. The palette lists the 19 onsets and 21 vowels;
cluster codas (ㄳ, ㄺ, ㅄ …) form on their own from two consonants in a row.

Pasting works too, as does an OS input method (the Windows Korean IME or
Russian layout, `Win+Space`) — a line that already contains Hangul or Cyrillic
is routed to the matching model whether you picked it, pasted it, or typed it.

## Commands

```text
/lang <pair>    switch the active pair (en-es, ko-en, ru-en)
/help           show pairs, commands, and the palette keys
/clear          clear the terminal
/quit           exit
/exit
```

## Portable deployment

For another computer, copy only:

```text
translator\
  translate.exe
  model\
    en-es\
    ko-en\
    ru-en\
```

## Adding a language pair

Add an entry to the table in `src/pairs.rs`:

```rust
PairSpec {
    id: "de-en",
    label: "German -> English",
    model_repo: "Helsinki-NLP/opus-mt-de-en",
    model_revision: None,
    tokenizer: TokenizerSource::Convert,
    script: Script::Latin,
}
```

- `model_revision` pins a non-default branch, which is how `en-es` reaches a
  safetensors conversion that never landed on `main`.
- `tokenizer` is `Prebuilt` when converted `tokenizer.json` files already exist
  on the Hub, and `Convert` when the one-time local step above is needed.
- `script` is what routes a line to this model automatically. Two pairs reading
  the same script is fine, but only the first is auto-routed — reach the other
  with `/lang`.

A new non-Latin script also needs a `Script` variant, a range in
`input::detect`, and a symbol list in `input::palette` — otherwise its lines
will not route and its letters will not be pickable, leaving pasted text and an
OS input method as the only way in. Scripts that compose (as Hangul does) need
a module like `input::hangul` wired into `Buffer::insert_symbol`; alphabets that
do not compose, like Cyrillic, need only the list.
