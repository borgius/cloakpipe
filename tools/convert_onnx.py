"""
Convert a downloaded DistilBERT-PII safetensors model to a quantized ONNX model.

Usage: python convert_onnx.py <model_dir>

  model_dir  — directory containing the downloaded HuggingFace model files
               (config.json, tokenizer_config.json, model.safetensors, …).

Outputs:
  <model_dir>/quantized/model_quantized.onnx  — INT8 quantized ONNX (~63 MB)

The original model.safetensors and the intermediate float32 ONNX are deleted
after successful quantization to minimise disk usage.
"""

import sys
import torch
import pathlib
from transformers import AutoModelForTokenClassification
from onnxruntime.quantization import quantize_dynamic, QuantType

model_dir = sys.argv[1]
fp32_path = pathlib.Path(model_dir) / "quantized" / "model_fp32.onnx"
out_path  = pathlib.Path(model_dir) / "quantized" / "model_quantized.onnx"
fp32_path.parent.mkdir(parents=True, exist_ok=True)

print("  Loading model…", flush=True)
model = AutoModelForTokenClassification.from_pretrained(model_dir)
model.eval()

dummy_ids  = torch.zeros(1, 16, dtype=torch.long)
dummy_mask = torch.ones(1, 16, dtype=torch.long)

print("  Exporting float32 ONNX…", flush=True)
torch.onnx.export(
    model,
    (dummy_ids, dummy_mask),
    str(fp32_path),
    input_names=["input_ids", "attention_mask"],
    output_names=["logits"],
    dynamic_axes={
        "input_ids":      {0: "batch", 1: "seq"},
        "attention_mask": {0: "batch", 1: "seq"},
        "logits":         {0: "batch", 1: "seq"},
    },
    opset_version=14,
)

print("  Quantizing to INT8…", flush=True)
quantize_dynamic(
    str(fp32_path),
    str(out_path),
    weight_type=QuantType.QInt8,
)
fp32_path.unlink()  # remove the large float32 intermediate

# The safetensors weights are no longer needed once we have the quantized ONNX.
safetensors = pathlib.Path(model_dir) / "model.safetensors"
if safetensors.exists():
    safetensors.unlink()

print(f"  Saved: {out_path}  ({out_path.stat().st_size // 1024 // 1024} MB)", flush=True)
