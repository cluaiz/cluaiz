import urllib.request, json, sys, os

component_type = sys.argv[1] # cli, engine, kernel_llama, kernel_onnx, driver_llama, driver_onnx
version = sys.argv[2]
repo = sys.argv[3]
tag = sys.argv[4]
token = os.environ.get("GITHUB_TOKEN")

url = f"https://api.github.com/repos/{repo}/releases/tags/{tag}"

req = urllib.request.Request(url)
req.add_header("Accept", "application/vnd.github.v3+json")
if token: req.add_header("Authorization", f"token {token}")

assets = []
try:
    with urllib.request.urlopen(req) as response:
        assets = json.loads(response.read().decode()).get("assets", [])
except Exception as e:
    print(f"API Fetch Error (might be first time release): {e}")

# Default Skeleton
registry_data = {
  "registry_name": "cluaiz-global-registry",
  "version": version,
  "components": { 
      "cli": {"status": "pending", "binaries": {}, "failed": []}, 
      "engine": {"status": "pending", "binaries": {}, "failed": []} 
  },
  "kernels": { 
      "llama_kernel": {"status": "pending", "binaries": {}, "failed": []}, 
      "onnx_kernel": {"status": "pending", "binaries": {}, "failed": []} 
  },
  "drivers": { 
      "llama": {"status": "pending", "binaries": {}, "failed": []}, 
      "onnx": {"status": "pending", "binaries": {}, "failed": []} 
  }
}

# Try to merge with existing registry from release
for a in assets:
    if a["name"] == "cluaiz-package-registry.json":
        try:
            req_dl = urllib.request.Request(a["url"]) # Must use API URL for private/auth downloads
            req_dl.add_header("Accept", "application/octet-stream")
            if token: req_dl.add_header("Authorization", f"token {token}")
            with urllib.request.urlopen(req_dl) as res:
                existing_data = json.loads(res.read().decode('utf-8'))
                if existing_data.get("version") == version:
                    registry_data = existing_data
        except Exception as e:
            print(f"Failed to fetch existing registry: {e}")
        break

EXPECTED_TARGETS = {
    "cli": ["win-x64", "win-arm64", "linux-x64", "linux-arm64", "mac-arm64", "mac-x64"],
    "engine": ["win-x64", "win-arm64", "linux-x64", "linux-arm64", "android-arm64", "android-x64", "mac-arm64", "mac-x64", "ios-arm64"],
    "kernel_llama": ["win-x64-avx512", "win-x64-avx2", "linux-x64-avx512", "linux-x64-avx2", "linux-arm64-neon", "mac-arm64-neon", "mac-x64-avx2", "ios-arm64-neon", "android-arm64-neon"],
    "kernel_onnx": ["win-x64-avx512", "win-x64-avx2", "linux-x64-avx512", "linux-x64-avx2", "linux-arm64-neon", "mac-arm64-neon", "mac-x64-avx2", "ios-arm64-neon", "android-arm64-neon"],
    "driver_llama": ["win-x64-cuda-13", "win-x64-cuda-12", "win-x64-cuda-11", "win-x64-openvino", "win-x64-vulkan", "linux-x64-cuda-13", "linux-x64-cuda-12", "linux-x64-cuda-11", "linux-x64-openvino", "linux-x64-sycl", "linux-x64-rocm", "linux-x64-hip", "linux-x64-cann", "linux-x64-vulkan", "mac-arm64-metal", "mac-x64-metal", "android-arm64-qnn", "ios-arm64-metal"],
    "driver_onnx": ["win-x64-cuda-13", "win-x64-cuda-12", "win-x64-cuda-11", "win-x64-openvino", "win-x64-vulkan", "linux-x64-cuda-13", "linux-x64-cuda-12", "linux-x64-cuda-11", "linux-x64-openvino", "linux-x64-sycl", "linux-x64-rocm", "linux-x64-hip", "linux-x64-cann", "linux-x64-vulkan", "mac-arm64-metal", "mac-x64-metal", "android-arm64-qnn", "ios-arm64-metal"]
}

# Setup pointers
if component_type == "cli":
    ptr = registry_data["components"]["cli"]
elif component_type == "engine":
    ptr = registry_data["components"]["engine"]
elif component_type == "kernel_llama":
    ptr = registry_data["kernels"]["llama_kernel"]
elif component_type == "kernel_onnx":
    ptr = registry_data["kernels"]["onnx_kernel"]
elif component_type == "driver_llama":
    ptr = registry_data["drivers"]["llama"]
elif component_type == "driver_onnx":
    ptr = registry_data["drivers"]["onnx"]
else:
    sys.exit(0)

expected = EXPECTED_TARGETS[component_type]
actual = []

for a in assets:
    name = a["name"]
    url = a["browser_download_url"]
    
    if name.endswith(".json"): continue
    
    if component_type == "cli":
        if name.startswith("cluaiz-") and not name.startswith("cluaiz-driver") and not name.startswith("cluaiz-kernel") and not name.startswith("cluaiz-engine"):
            parts = name.split(f"-{version}-")
            if len(parts) > 1:
                key = parts[-1].replace(".exe", "")
                ptr["binaries"][key] = url
                actual.append(key)
                
    elif component_type == "engine":
        if name.startswith("cluaiz-engine") or name.startswith("libcluaiz-engine"):
            parts = name.split(f"-{version}-")
            if len(parts) > 1:
                key = parts[-1].rsplit(".", 1)[0]
                ptr["binaries"][key] = url
                actual.append(key)
                
    elif component_type == "kernel_llama":
        if name.startswith("cluaiz-kernel-") or name.startswith("libcluaiz-kernel-"):
            parts = name.split(f"-{version}-")
            if len(parts) > 1:
                key = parts[-1].rsplit(".", 1)[0]
                ptr["binaries"][key] = url
                actual.append(key)
                
    elif component_type == "kernel_onnx":
        if name.startswith("cluaiz-onnx-kernel-") or name.startswith("libcluaiz-onnx-kernel-"):
            parts = name.split(f"-{version}-")
            if len(parts) > 1:
                key = parts[-1].rsplit(".", 1)[0]
                ptr["binaries"][key] = url
                actual.append(key)
                
    elif component_type == "driver_llama":
        if name.startswith("cluaiz-driver-") or name.startswith("libcluaiz-driver-"):
            parts = name.split(f"-{version}-")
            if len(parts) > 1:
                key = parts[-1].rsplit(".", 1)[0]
                ptr["binaries"][key] = url
                actual.append(key)
                
    elif component_type == "driver_onnx":
        if name.startswith("cluaiz-onnx-driver-") or name.startswith("libcluaiz-onnx-driver-"):
            parts = name.split(f"-{version}-")
            if len(parts) > 1:
                key = parts[-1].rsplit(".", 1)[0]
                ptr["binaries"][key] = url
                actual.append(key)

failed_list = [t for t in expected if t not in actual]
ptr["failed"] = failed_list

if not failed_list and actual:
    ptr["status"] = "success"
elif actual:
    ptr["status"] = "partial_success"
else:
    ptr["status"] = "failed"

with open("cluaiz-package-registry.json", "w") as out:
    json.dump(registry_data, out, indent=2)

print(f"Generated cluaiz-package-registry.json for {component_type}")
