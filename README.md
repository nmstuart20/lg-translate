# Offline Translator (Rust)

A lightweight language translation utility.

## Setup

All one-time. Budget about 300 MB per pair.

**1. Build**

```bash
cargo build --release
```

**2. Install Python transformers and torch**

Every pair except `en-es` needs a one-time conversion step, because their
upstream repos ship their weights and tokenizers in formats this program can't
read.

You may need to create a python virtual environement.

```bash
python -m venv venv
source venv/bin/activate
pip install "transformers[sentencepiece]" protobuf
pip install torch --index-url https://download.pytorch.org/whl/cpu
```

**3. Download the models.**

One per pair you want, or `--download-model all` for every one of them.

```bash
./lg-translate --download-model de-en
./lg-translate --download-model el-en
./lg-translate --download-model es-en
./lg-translate --download-model ru-en
```

**4. Run it.**

```bash
./lg-translate
```

## Picking a pair

There is no default direction. Startup lists the pairs and waits for one:

```text
Select a language pair:
    en-es  English -> Spanish
    de-en  German -> English
    el-en  Greek -> English
    es-en  Spanish -> English
    ru-en  Russian -> English

pair> de-en
```

`--lang de-en` skips the question. Whatever is chosen, **every line goes to
that pair** — nothing is inferred from what you type, so a German line typed
while `es-en` is active is translated as though it were Spanish. Switch with
`/lang` before switching languages.

## Commands

```text
/lang <pair>    switch the active pair
                (en-es, de-en, el-en, es-en, ru-en)
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
    de-en\
    el-en\
    es-en\
    ru-en\
```

Only the pairs you actually want need to be there; the rest are listed as
`(not downloaded)`.

## Adding a language pair

Add an entry to the table in `src/pairs.rs`:

```rust
PairSpec {
    id: "fr-en",
    label: "French -> English",
    model_repo: "Helsinki-NLP/opus-mt-fr-en",
    model_revision: None,
    tokenizer: TokenizerSource::Convert,
    script: Script::Latin,
}
```

- `id` is the directory under `model/` and the name `/lang` takes; it does not
  have to match the repo. `el-en` is served by `opus-mt-grk-en`, since
  Helsinki-NLP published no `el-en` and the Greek-languages model covers it.
- `model_revision` pins a non-default branch, which is how `en-es` reaches a
  safetensors conversion that never landed on `main`.
- `tokenizer` is `Prebuilt` when converted `tokenizer.json` files already exist
  on the Hub, and `Convert` when the one-time local step above is needed.
- `script` selects the palette offered while the pair is active. Several pairs
  sharing a script is fine — nothing is decided by it beyond that.

A new non-Latin script also needs a `Script` variant and a symbol list in
`input::palette`, otherwise its letters will not be pickable and pasted text or
an OS input method are the only way in. An alphabet needs nothing else: the
editor inserts one picked symbol as one character. A script that composes —
where picking two symbols in a row has to produce one — would need that step
written into `Buffer::insert_char` and `Buffer::backspace`, which today assume
one symbol is one character.
