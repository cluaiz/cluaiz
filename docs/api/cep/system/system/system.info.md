---
method: "system.info"
transport: "POST /v1/cep"
category: "system"
category_title: "System & Infrastructure"
group: "system"
summary: "Engine name and version, CEP version, hardware, and the LAN address"
scope: "read"
execution: "sync"
mcp: true
owner_only: false
destructive: false
idempotent: true
params: []
request_payload: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "method": "system.info",
    "params": {}
  }
response: |
  {
    "id": 1,
    "jsonrpc": "2.0",
    "result": {
      "cep_version": 1,
      "engine": "Cluaiz Engine",
      "hardware": {
        "architecture": "x86_64",
        "cpu": {
          "brand": "AMD Ryzen 7 7435HS",
          "logical_threads": 16,
          "physical_cores": 8
        },
        "gpus": [
          {
            "compute_capability": "8.9",
            "index": 0,
            "name": "NVIDIA GeForce RTX 3050 A Laptop GPU",
            "vendor": "NVIDIA",
            "vram_total_gb": 4.0
          }
        ],
        "memory": {
          "available_gb": 7.47,
          "total_gb": 23.69
        },
        "os_name": "Windows",
        "os_version": "11"
      },
      "network": {
        "lan_ip": "192.168.1.20"
      },
      "version": "0.1.0"
    }
  }
errors: []
---

# system.info

What the Engine is and the machine it runs on: the Engine's name and version, the CEP version, the hardware profile, and the LAN address other devices can reach it at. An app calls it first to check it is talking to a Cluaiz Engine and which CEP version it speaks.

## What it does

1. Reads the hardware profile the Engine probed at start (`HardwareProber::current()`): OS, CPU, memory, GPUs, storage. It is the profile the planner uses; it does not probe again.
2. Finds the LAN address by asking the OS which local address would reach a public one (a UDP socket connected to `8.8.8.8:80`). Connecting a UDP socket sends no packet. When there is no network, `network.lan_ip` is `null`.
3. Answers. Nothing is loaded, written or changed.

## Result

| Field | Meaning |
|---|---|
| `engine`, `version` | `"Cluaiz Engine"` and the Engine's build version |
| `cep_version` | The protocol version; a change here means a breaking change to CEP |
| `hardware` | `architecture`, `os_name`, `os_version`, `hostname`, `cpu`, `memory`, `gpus[]`, `storage` |
| `network.lan_ip` | The address on the local network, or `null` |

For numbers that change every second (free memory, free VRAM), use [`system.live`](system.live.md).

## Example

```bash
curl -s http://127.0.0.1:8000/v1/cep -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"jsonrpc":"2.0","id":1,"method":"system.info","params":{}}'
```

Leave out the `Authorization` header when API authentication is off.
