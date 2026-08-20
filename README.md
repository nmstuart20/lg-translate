# Offline Translator (Rust)

A lightweight language translation utility (~300mb per language).

## Setup

**1. Build**

```bash
cargo build --release
```

**2. Install Python transformers and torch**

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

If the `--lang de-en` is passed in, the program will ask the user to select a language pair:

```text
Select a language pair:
    en-es  English -> Spanish
    de-en  German -> English
    ...

pair> de-en
```

To switch languages during run-time use the `/lang` command.

## Commands

```text
/lang <pair>    switch the active pair
                (en-es, de-en, el-en, es-en, ru-en, sv-en)
/help           show pairs, commands, and the palette keys
/clear          clear the terminal
/quit           exit
/exit
```

## Offline deploy

Copy over the executable and model directory:

```text
translator\
  lg-translate.exe
  model\
    en-es\
    de-en\
```
