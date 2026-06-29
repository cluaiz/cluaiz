# `cluaiz serve` Command Reference

The `serve` command starts the background Engine Daemon process. It sets up both the HTTP API server and the Named Pipe IPC channel.

---

## 📋 Syntax & Usage

```bash
cluaiz serve [options]
```

### Options:
* `--port <PORT>`: The port to bind the HTTP API server to (default: `8000`).
* `--host <IP>`: The IP address to listen on (default: `127.0.0.1`).
* `--enable-cors <true/false>`: Toggle Cross-Origin Resource Sharing (CORS) header injection (default: `true`).

---

## ⚙️ Behind the Scenes (Execution Flow)

1. **Locks Instance:** Attempts to lock a system-wide lock file (`.serve_lock`) to prevent starting multiple conflicting daemons.
2. **Loads Configs:** Parses `Permission.json`, `system_booster.json`, and `system_control.json` from `~/.cluaiz/engine/`.
3. **IPC Named Pipe Initialization:** Spawns a background thread listening on `\\.\pipe\cluaiz_engine_pipe` for desktop applications (Tauri UI).
4. **Starts Axum Web Server:** Binds to the designated port and hosts endpoints for REST API access.
