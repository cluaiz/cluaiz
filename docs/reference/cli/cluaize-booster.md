# `cluaiz booster` Command Reference

The `booster` command configures the hardware acceleration profile inside `system_booster.json`.

---

## 📋 Syntax & Usage

```bash
cluaiz booster [options]
```

### Options:
* `--mode <PROFILE>`: Choose compute profile (`edge`, `multitasking`, `balance`, `max_boost`, `ultra_max_boost`, `hyper_cluster`).
* `--kv-quant <LEVEL>`: Choose KV Cache quantization (`auto`, `kv16`, `kv8`, `kv4`).
* `--context-shift <STATE>`: Adjust context shifting limits (`auto`, `off`, `minimal`, `standard`, `aggressive`, `extreme`).
* `--spec-decode <on/off/auto>`: Toggle speculative decoding logic.

### Examples:
```bash
cluaiz booster --mode edge
cluaiz booster --mode max_boost --kv-quant kv8
```
