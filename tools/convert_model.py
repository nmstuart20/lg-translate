#!/usr/bin/env python3
"""Prepare a downloaded Marian model for Rust.
The tokenizer conversion is adapted from the upstream Candle Marian recipe:
https://huggingface.co/KeighBee/candle-marian/blob/main/convert_tok.py
"""

import argparse
import json
import shutil
import sys
import tempfile
from pathlib import Path

try:
    from transformers import AutoTokenizer
    from transformers.convert_slow_tokenizer import (
        SpmConverter,
        import_protobuf,
        requires_backends,
    )
except ImportError:
    sys.exit(
        "Missing dependencies. Install them with:\n"
        '    pip install "transformers[sentencepiece]" protobuf\n'
        "    pip install torch --index-url https://download.pytorch.org/whl/cpu"
    )

SAFETENSORS = "model.safetensors"
PYTORCH_BIN = "pytorch_model.bin"
SOURCE_OUT = "tokenizer-source.json"
TARGET_OUT = "tokenizer-target.json"
SPM_FILES = ("source.spm", "target.spm", "vocab.json", "tokenizer_config.json")


class MarianConverter(SpmConverter):
    """Builds a fast tokenizer from one of the two .spm files.

    Marian keeps a single shared `vocab.json` for both sides but a separate
    SentencePiece model per side, so piece scores come from the .spm while the
    token ids have to come from vocab.json.
    """

    def __init__(self, *args, index: int = 0):
        requires_backends(self, "protobuf")
        # Skip SpmConverter.__init__, which would load the wrong proto.
        super(SpmConverter, self).__init__(*args)

        model_pb2 = import_protobuf()
        proto = model_pb2.ModelProto()
        with open(self.original_tokenizer.spm_files[index], "rb") as handle:
            proto.ParseFromString(handle.read())
        self.proto = proto

        vocab_path = Path(self.original_tokenizer.spm_files[0]).parent / "vocab.json"
        with open(vocab_path, encoding="utf-8") as handle:
            self._vocab = json.load(handle)

    def vocab(self, proto):
        size = max(self._vocab.values()) + 1
        vocab = [("<NIL>", -100.0)] * size
        for piece in proto.pieces:
            index = self._vocab.get(piece.piece)
            if index is None:
                continue
            vocab[index] = (piece.piece, piece.score)
        return vocab


def convert_weights(model_dir: Path) -> None:
    if (model_dir / SAFETENSORS).exists():
        print("  weights: already converted")
        return

    if not (model_dir / PYTORCH_BIN).exists():
        sys.exit(f"{model_dir}: neither {SAFETENSORS} nor {PYTORCH_BIN} is present")

    try:
        from transformers import MarianMTModel
    except ImportError:
        sys.exit(
            "Converting weights needs PyTorch. Install it with:\n"
            "    pip install torch --index-url https://download.pytorch.org/whl/cpu"
        )

    print("  weights: converting to safetensors...")
    model = MarianMTModel.from_pretrained(str(model_dir))

    # Marian ties the encoder, decoder and lm_head embeddings to one shared
    # tensor, which safetensors refuses to store more than once. Going through
    # save_pretrained lets transformers apply its own tying rules rather than
    # us guessing which aliases are safe to drop. It is written to a temporary
    # directory so the config.json we downloaded is left untouched.
    with tempfile.TemporaryDirectory(dir=model_dir) as staging:
        model.save_pretrained(staging, safe_serialization=True)
        shutil.move(str(Path(staging) / SAFETENSORS), model_dir / SAFETENSORS)

    print(f"  {model_dir / SAFETENSORS}")

    (model_dir / PYTORCH_BIN).unlink()
    print(f"  removed {PYTORCH_BIN} (superseded; re-downloadable)")


def convert_tokenizers(model_dir: Path) -> None:
    if (model_dir / SOURCE_OUT).exists() and (model_dir / TARGET_OUT).exists():
        print("  tokenizers: already converted")
        return

    missing = [name for name in SPM_FILES if not (model_dir / name).exists()]
    if missing:
        sys.exit(f"{model_dir}: missing {', '.join(missing)}")

    print("  tokenizers: converting...")
    tokenizer = AutoTokenizer.from_pretrained(str(model_dir), use_fast=False)

    for index, out_name in ((0, SOURCE_OUT), (1, TARGET_OUT)):
        converted = MarianConverter(tokenizer, index=index).converted()
        destination = model_dir / out_name
        converted.save(str(destination))
        print(f"  {destination}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "model_dir",
        type=Path,
        nargs="+",
        help="a downloaded pair directory, e.g. model/de-en",
    )
    args = parser.parse_args()

    for model_dir in args.model_dir:
        print(f"Preparing {model_dir}")
        convert_weights(model_dir)
        convert_tokenizers(model_dir)


if __name__ == "__main__":
    main()
