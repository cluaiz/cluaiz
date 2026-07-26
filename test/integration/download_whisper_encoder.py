import urllib.request
import os

encoder_url = "https://huggingface.co/onnx-community/whisper-large-v3-turbo/resolve/main/onnx/encoder_model_int8.onnx"
out_dir = os.path.join(os.environ["USERPROFILE"], ".cluaiz", "models", "audio", "whisper-large-v3-turbo")
out_path = os.path.join(out_dir, "encoder_model_int8.onnx")

os.makedirs(out_dir, exist_ok=True)

if os.path.exists(out_path):
    size_mb = os.path.getsize(out_path) / (1024*1024)
    print(f"Already exists: {out_path} ({size_mb:.1f} MB)")
else:
    print(f"Downloading encoder_model_int8.onnx (~75MB) from HuggingFace...")
    print(f"  -> {out_path}")
    
    def progress(count, block_size, total_size):
        done = count * block_size
        if total_size > 0:
            pct = min(100, done * 100 // total_size)
            print(f"\r  Progress: {pct}% ({done//(1024*1024)}MB/{total_size//(1024*1024)}MB)", end="", flush=True)

    urllib.request.urlretrieve(encoder_url, out_path, reporthook=progress)
    print(f"\nDownload complete: {os.path.getsize(out_path)/(1024*1024):.1f} MB")

print("Done!")
