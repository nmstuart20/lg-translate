# Offline Translator (Rust)

A lightweight language translation utility (~300mb per language).

## Setup

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
./lg-translate --download-model es-en
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
    ...

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
    ...
```