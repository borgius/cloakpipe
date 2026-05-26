#!/usr/bin/env bash
# Download the DistilBERT-PII quantized ONNX model.
#
# Strategy:
#   1. Always fetch tokenizer/config files from HuggingFace (tiny, needed for inference).
#   2. Try GitHub Releases for the ONNX model (~63 MB, fast).
#   3. Fall back to HuggingFace safetensors download + Python ONNX conversion.
#
# Usage: download_model.sh [--force] [--target-dir <dir>]

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEFAULT_HOME="${HOME:-$REPO_ROOT}/.cloakpipe"
GLOBAL_HOME="${CLOAKPIPE_HOME:-${CLOAKPIPE_CONFIG_HOME:-$DEFAULT_HOME}}"
MODEL_DIR="${CLOAKPIPE_MODEL_DIR:-$GLOBAL_HOME/models/distilbert-pii}"
ONNX_PATH="$MODEL_DIR/quantized/model_quantized.onnx"
RELEASE_URL="https://github.com/borgius/cloakpipe/releases/download/models-v1/model_quantized.onnx"
HF_BASE="https://huggingface.co/ab-ai/pii_model_based_on_distilbert/resolve/main"
HF_TOKENIZER_FILES=(config.json tokenizer_config.json special_tokens_map.json tokenizer.json)
VENV_DIR="$GLOBAL_HOME/gliner-pii-venv"
VENV_PY="$VENV_DIR/bin/python"
TORCH_INDEX="https://download.pytorch.org/whl/cpu"
FORCE=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --force)
            FORCE=1
            shift
            ;;
        --target-dir)
            if [[ $# -lt 2 ]]; then
                echo "--target-dir requires a directory" >&2
                exit 2
            fi
            MODEL_DIR="$2"
            ONNX_PATH="$MODEL_DIR/quantized/model_quantized.onnx"
            shift 2
            ;;
        *)
            echo "Unknown argument: $1" >&2
            exit 2
            ;;
    esac
done

mkdir -p "$MODEL_DIR/quantized"

# ---------------------------------------------------------------------------
# Step 1: Always ensure tokenizer/config files are present (tiny, from HF)
# ---------------------------------------------------------------------------
echo "Ensuring tokenizer files..."
for f in "${HF_TOKENIZER_FILES[@]}"; do
    dest="$MODEL_DIR/$f"
    if [[ -f "$dest" && $FORCE -eq 0 ]]; then
        printf "  %-35s already present\n" "$f"
    else
        printf "  Downloading %s\n" "$f"
        curl -fSL --progress-bar "$HF_BASE/$f" -o "$dest"
    fi
done

# ---------------------------------------------------------------------------
# Step 2: Try GitHub Release for the ONNX model
# ---------------------------------------------------------------------------
if [[ -f "$ONNX_PATH" && $FORCE -eq 0 ]]; then
    echo "ONNX model already present."
    exit 0
fi

echo "Trying GitHub Release..."
tmp="$ONNX_PATH.tmp"
if curl -fsSL --max-time 300 --progress-bar -o "$tmp" "$RELEASE_URL" 2>&1; then
    bytes=$(wc -c < "$tmp")
    if (( bytes > 10 * 1024 * 1024 )); then
        mv "$tmp" "$ONNX_PATH"
        echo "Downloaded from GitHub Release ($(( bytes / 1024 / 1024 )) MB)."
        exit 0
    fi
    rm -f "$tmp"
    echo "Release file too small — model not yet uploaded, falling back to HuggingFace."
else
    rm -f "$tmp" 2>/dev/null || true
    echo "Release download failed — falling back to HuggingFace."
fi

# ---------------------------------------------------------------------------
# Step 3: Download model.safetensors from HuggingFace
# ---------------------------------------------------------------------------
echo "Downloading model.safetensors from HuggingFace..."
safetensors="$MODEL_DIR/model.safetensors"
if [[ ! -f "$safetensors" || $FORCE -eq 1 ]]; then
    curl -fSL --progress-bar "$HF_BASE/model.safetensors" -o "$safetensors"
fi

# ---------------------------------------------------------------------------
# Step 4: Ensure Python venv with required packages
# ---------------------------------------------------------------------------
if [[ ! -x "$VENV_PY" ]]; then
    echo "Creating Python venv at $VENV_DIR..."
    base_py="$(command -v python3.12 2>/dev/null \
        || command -v python3.11 2>/dev/null \
        || command -v python3.10 2>/dev/null \
        || command -v python3)"
    "$base_py" -m venv "$VENV_DIR"
    "$VENV_PY" -m pip install --upgrade pip --quiet
fi

echo "Installing conversion dependencies..."
"$VENV_PY" -m pip install --quiet \
    --extra-index-url "$TORCH_INDEX" \
    torch "transformers<4.45" onnx onnxruntime

# ---------------------------------------------------------------------------
# Step 5: Convert safetensors → quantized ONNX
# ---------------------------------------------------------------------------
echo "Converting safetensors → ONNX (this may take a minute)..."
"$VENV_PY" "$REPO_ROOT/tools/convert_onnx.py" "$MODEL_DIR"

echo ""
echo "DistilBERT-PII model ready. Run: cloakpipe scan <file>"
