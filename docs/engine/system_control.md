# Hardware Profile (`hardware_profile.json`)

The planner needs to know this machine: RAM, CPU and GPUs. The hardware probe
(`engine/hardware/src/prober.rs`) collects this.

- **First start:** a deep probe (a few seconds: memory benchmark, disks, NVIDIA GPUs through NVML)
  runs once and is saved to `<CLUAIZ_HOME>/config/hardware_profile.json`.
- **Later starts:** the file is read and only free RAM and free VRAM are measured again.
- **Probe again:** `cluaiz calibrate` re-runs the deep probe. It also runs by itself when total
  RAM, CPU threads or the NVIDIA GPUs no longer match the file (RAM added, a VM resized).
- **Non-NVIDIA GPUs:** NVML sees NVIDIA GPUs only. When it finds none, the GPUs that llama.cpp's
  own backends report (Metal, Vulkan) are used. These are not saved in the file.

What the planner made of it: `cluaiz status`, `cluaiz show <model>`, CEP `system.info` and
`system.plan`.

There is no `system_control.json`. Older docs described one; it was never built.
