# Offline Translator (Rust)

A small, CPU-only English -> Spanish translation utility for Windows.

It opens as a persistent command-line REPL:

```text
Offline Translator
English -> Spanish
Enter one line at a time. Press Ctrl-C to exit.

> Hello, how are you?
Hola, ¿cómo estás?

> I have a meeting tomorrow morning.
Tengo una reunión mañana por la mañana.

>
```

The executable stays running until `Ctrl-C`, `/quit`, `/exit`, or EOF.

## Design

- Rust executable
- Hugging Face Candle for CPU inference
- Marian / OPUS-MT English -> Spanish
- Hugging Face Tokenizers
- No Python at runtime
- No GPU
- After the model files have been downloaded, translation is fully offline

Candle has a native Marian-MT implementation. This project adapts its current Marian example into a persistent REPL.

## Windows prerequisites for building

Install:

1. Rust from https://rustup.rs/
2. Visual Studio Build Tools with **Desktop development with C++**
3. Git

Then open PowerShell in this directory.

## Build

```powershell
cargo build --release
```

The executable will be:

```text
target\release\offline-translator.exe
```

For a friendlier name:

```powershell
Copy-Item target\release\offline-translator.exe .\translate.exe
```

## Download the model

Put `translate.exe` in the project root, then:

```powershell
.\translate.exe --download-model
```

That creates:

```text
model\
  model.safetensors
  tokenizer-marian-base-en-es-en.json
  tokenizer-marian-base-en-es-es.json
```

The model is downloaded only during setup. Once these files exist, the machine can be disconnected from the internet.

## Run

```powershell
.\translate.exe
```

Then keep entering lines:

```text
> Where is the nearest airport?
¿Dónde está el aeropuerto más cercano?

> Please send me the document.
Por favor, envíame el documento.

>
```

Press `Ctrl-C` whenever you want to stop it.

## Commands

```text
/help
/clear
/quit
/exit
```

## Portable deployment

For another Windows computer, copy only:

```text
translator\
  translate.exe
  model\
    model.safetensors
    tokenizer-marian-base-en-es-en.json
    tokenizer-marian-base-en-es-es.json
```

No Rust toolchain or Python installation is required on the target computer.

## Model directory override

Normally `model\` is expected beside `translate.exe`.

You can override it:

```powershell
.\translate.exe --model-dir D:\translation-model
```

Download there:

```powershell
.\translate.exe --model-dir D:\translation-model --download-model
```

## Important size note

This version deliberately uses Candle's straightforward FP32 Marian path to keep the code native and maintainable. The executable itself is relatively small, but the model is the dominant part of the package and is substantially larger than a highly optimized INT8 build.

A later optimization step can quantize or switch inference backends if minimizing the model bundle below ~150 MB becomes more important than keeping the implementation this simple.

## Changing language pairs

This initial project is intentionally fixed to English -> Spanish.

Candle currently exposes Marian configurations for several OPUS-MT pairs. To change the pair, update:

- `MODEL_REPO`
- `MODEL_REVISION`
- tokenizer filenames/repository if necessary
- `marian::Config::opus_mt_en_es()`

The upstream Candle Marian example is the best reference for known-working repository/revision/tokenizer combinations.
