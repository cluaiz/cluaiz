import os
import sys
import json
import hashlib
from datetime import datetime, timezone

def compute_sha256(filepath):
    sha256_hash = hashlib.sha256()
    with open(filepath, "rb") as f:
        for byte_block in iter(lambda: f.read(4096), b""):
            sha256_hash.update(byte_block)
    return sha256_hash.hexdigest()

def main():
    if len(sys.argv) < 3:
        print("Usage: python secure_registry_builder.py <version> <repo>")
        sys.exit(1)
        
    version = sys.argv[1]
    repo = sys.argv[2]
    
    # We assume this script runs in the GitHub Actions runner
    # after `actions/download-artifact` has downloaded all build artifacts into `./artifacts/`
    artifacts_dir = "./artifacts"
    
    registry = {
        "registry_name": "cluaiz-global-registry",
        "version": version,
        "release_date": datetime.now(timezone.utc).isoformat(),
        "build_status": "success",
        "components": {
            "cli": {"status": "success", "version": version, "binaries": {}, "hashes": {}},
        },
        "kernels": {
            "llama_kernel": {"status": "success", "version": version, "binaries": {}, "hashes": {}},
            "onnx_kernel": {"status": "success", "version": version, "binaries": {}, "hashes": {}}
        },
        "drivers": {
            "llama": {"status": "success", "version": version, "binaries": {}, "hashes": {}},
            "onnx": {"status": "success", "version": version, "binaries": {}, "hashes": {}}
        }
    }
    
    base_url = f"https://github.com/{repo}/releases/download/{version}"
    
    if os.path.exists(artifacts_dir):
        # Walk through all downloaded artifacts
        for root, _, files in os.walk(artifacts_dir):
            for file in files:
                # We only care about the actual binaries, not metadata files
                if file.endswith(".json") or file.endswith(".sha256"):
                    continue
                    
                filepath = os.path.join(root, file)
                file_hash = compute_sha256(filepath)
                url = f"{base_url}/{file}"
                
                # Categorize based on filename
                if file.startswith("cluaiz-kernel-"):
                    platform = file.replace(f"cluaiz-kernel-{version}-", "").replace(".dll", "").replace("lib", "").replace(".so", "").replace(".dylib", "")
                    registry["kernels"]["llama_kernel"]["binaries"][platform] = url
                    registry["kernels"]["llama_kernel"]["hashes"][platform] = file_hash
                elif file.startswith("cluaiz-onnx-kernel-"):
                    platform = file.replace(f"cluaiz-onnx-kernel-{version}-", "").replace(".dll", "").replace("lib", "").replace(".so", "").replace(".dylib", "")
                    registry["kernels"]["onnx_kernel"]["binaries"][platform] = url
                    registry["kernels"]["onnx_kernel"]["hashes"][platform] = file_hash
                elif file.startswith("cluaiz-driver-"):
                    platform = file.replace(f"cluaiz-driver-{version}-", "").replace(".dll", "").replace("lib", "").replace(".so", "").replace(".dylib", "")
                    registry["drivers"]["llama"]["binaries"][platform] = url
                    registry["drivers"]["llama"]["hashes"][platform] = file_hash
                elif file.startswith("cluaiz-onnx-driver-"):
                    platform = file.replace(f"cluaiz-onnx-driver-{version}-", "").replace(".zip", "").replace("lib", "")
                    registry["drivers"]["onnx"]["binaries"][platform] = url
                    registry["drivers"]["onnx"]["hashes"][platform] = file_hash
                elif file.startswith("cluaiz-"):
                    # CLI
                    platform = file.replace(f"cluaiz-{version}-", "").replace(".exe", "").replace(".zip", "").replace(".tar.gz", "")
                    registry["components"]["cli"]["binaries"][platform] = url
                    registry["components"]["cli"]["hashes"][platform] = file_hash
                    
    with open("cluaiz-registry.json", "w") as f:
        json.dump(registry, f, indent=2)
        
    print(f"Secure Registry built successfully!")

if __name__ == "__main__":
    main()
