import urllib.request
import os

MODEL_DIR = os.path.join(os.environ["USERPROFILE"], ".cluaiz", "models", "audio", "whisper-large-v3-turbo")
os.makedirs(MODEL_DIR, exist_ok=True)

HF_BASE = "https://huggingface.co/onnx-community/whisper-large-v3-turbo/resolve/main"

FILES = [
    ("tokenizer.json",      f"{HF_BASE}/tokenizer.json",       "~2MB"),
    ("vocab.json",          f"{HF_BASE}/vocab.json",            "~1MB"),
    ("merges.txt",          f"{HF_BASE}/merges.txt",            "~0.5MB"),
    ("added_tokens.json",   f"{HF_BASE}/added_tokens.json",     "~1KB"),
    ("special_tokens_map.json", f"{HF_BASE}/special_tokens_map.json", "~1KB"),
]

def progress(count, block_size, total_size):
    done = count * block_size
    if total_size > 0:
        pct = min(100, done * 100 // total_size)
        bar = "=" * (pct // 4) + " " * (25 - pct // 4)
        print(f"\r  [{bar}] {pct}% ({done//1024}KB)", end="", flush=True)

for fname, url, size in FILES:
    out_path = os.path.join(MODEL_DIR, fname)
    if os.path.exists(out_path):
        print(f"  Already exists: {fname} ({os.path.getsize(out_path)//1024}KB)")
        continue
    print(f"\nDownloading {fname} ({size})...")
    try:
        urllib.request.urlretrieve(url, out_path, reporthook=progress)
        print(f"\n  OK: {os.path.getsize(out_path)//1024}KB")
    except Exception as e:
        print(f"\n  FAILED: {e}")

print(f"\n\nAll files in model dir:")
for f in os.listdir(MODEL_DIR):
    size = os.path.getsize(os.path.join(MODEL_DIR, f))
    print(f"  {f:40s} {size//(1024*1024)}MB ({size//1024}KB)")
