# Offline Translator (Rust)

A lightweight language translation utility.

| Pair    | Direction          |
| ------- | ------------------ |
| `en-es` | English -> Spanish |
| `ko-en` | Korean -> English  |
| `ru-en` | Russian -> English |

It opens as a persistent command-line REPL:

```text
Offline Translator
English -> Spanish  [input: raw]
Enter one line at a time. Press Ctrl-C to exit.
Commands: /lang  /input  /help  /clear  /quit

> Hello, how are you?
Hola, ¿cómo estás?

> Привет, как дела?
Hey, how are you?

> 한국어를 배우고 싶습니다
I want to learn Korean.

>
```

A line containing Hangul or Cyrillic is routed to the matching model
automatically, so pasted text and OS input methods need no command. Plain ASCII
lines go to the *active* pair, which starts as `en-es` and is changed with
`/lang`.

The executable stays running until `Ctrl-C`, `/quit`, `/exit`, or EOF.

## Typing Korean and Russian on a US keyboard

Switching to a pair whose input is not Latin turns on romanized input, so the
models are reachable with nothing but the keys already on the keyboard. The
converted text is echoed above the translation so a misparse is visible rather
than silently mistranslated:

```text
> /lang ko-en
Active pair: Korean -> English  [input: roman]

> annyeonghaseyo
  안녕하세요
Hello.

> /lang ru-en
Active pair: Russian -> English  [input: roman]

> ya lyublyu chitat' knigi.
  я люблю читать книги.
I like reading books.
```

`/input raw` turns the conversion off — for pasting real Korean or Russian, or
for using the Windows Korean IME or Russian keyboard layout (`Win+Space`).
`/input roman` turns it back on.

### One-time Python step for ko-en and ru-en

`en-es` needs nothing but the download. The other two do, because their upstream
repositories are older and ship neither of the formats this program reads:

- Their weights are only published as `pytorch_model.bin`, in the pre-1.6
  PyTorch pickle format — a raw pickle stream rather than the ZIP container
  Candle reads. They are re-saved as `model.safetensors`.
- Their tokenizers are only published as `source.spm` / `target.spm` /
  `vocab.json`. They are converted to `tokenizer.json`.

`--download-model` runs `tools/convert_model.py` for this automatically when
Python is available. Install its dependencies first:

```powershell
pip install "transformers[sentencepiece]" protobuf
pip install torch --index-url https://download.pytorch.org/whl/cpu
```

If Python is not on `PATH`, the download still fetches everything and leaves a
copy of the script in the pair directory to run by hand:

```powershell
python model\ko-en\convert_model.py model\ko-en
```

The script is idempotent, and everything it writes is portable — it can be run
on one machine and the `model\` directory copied to another. **None of this is
needed at translation time**; the built executable never calls Python.

Once the model files exist, the machine can be disconnected from the internet.

## Commands

```text
/lang <pair>    switch the active pair (en-es, ko-en, ru-en)
/input roman    type Korean or Russian in ASCII
/input raw      pass typed text through untouched
/help           show pairs, commands, and the romanization tables
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

A new non-Latin script also needs a converter under `src/input/` and a match arm
in `input::romanize`, or it will only be usable in `/input raw`.
