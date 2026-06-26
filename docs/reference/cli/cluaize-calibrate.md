# `cluaize calibrate` Command Reference

The `calibrate` command audits local workstation execution thresholds.

---

## 📋 Syntax & Usage

```bash
cluaize calibrate
```

---

## ⚙️ Execution Flow

1. **Hardware Audit:** Checks active device interfaces (CUDA, Metal, Vulkan).
2. **Bandwidth Test:** Runs memory speed benchmarks to estimate system RAM bandwidth and PCIe transfer latency.
3. **Writes Profile:** Synchronizes variables inside `system_control.json` to ensure compatible baseline options are pre-selected.
