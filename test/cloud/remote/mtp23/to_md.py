"""Markdown tables from `cluaiz bench --mtp-compare -o json` (the same table the tool prints)."""
import json
import sys

d = json.load(open(sys.argv[1], encoding="utf-8"))
t = d.get("temperature", 0) or 0
print(f"{d['model_id']} | {'temperature ' + str(t) if t else 'greedy'}, thinking off, {d['rounds']} rounds of MTP off, on and auto")
for p in d["placements"]:
    gb = lambda v: "?" if v is None else f"{v:.2f} GB"
    print()
    print(f"{p['placement']} | ngl {p['n_gpu_layers']} | n_cpu_moe {p['n_cpu_moe']} | n_ctx {p['n_ctx']} | "
          f"from disk {p['disk_stream_gb']:.1f} GB | VRAM taken: off {gb(p['vram_taken_gb_off'])}, on {gb(p['vram_taken_gb_on'])}")
    if p.get("mtp_on_failed"):
        print(f"MTP on did not load: {p['mtp_on_failed']}")
    print()
    print("| prompt | tokens | t/s off | t/s on | gain | gain range | kept | t/s auto | gain auto | auto guessed / stopped | prompt t/s off | prompt t/s on | same text |")
    print("| ------ | -----: | ------: | -----: | ---: | ---------: | ---: | -------: | --------: | :--------------------: | -------------: | ------------: | :-------: |")
    for r in p["prompts"]:
        print(f"| {r['prompt']} | {r['tokens']} | {r['generation_tps_off']:.2f} | {r['generation_tps_on']:.2f} | "
              f"{r['gain']:.2f}x | {r['gain_min']:.2f}-{r['gain_max']:.2f} | {r['kept'] * 100:.0f}% | "
              f"{r.get('generation_tps_auto', 0):.2f} | {r.get('gain_auto', 0):.2f}x | "
              f"{r.get('auto_drafted_runs', 0)}/{r.get('auto_stopped_runs', 0)} of {r.get('auto_runs', 0)} | "
              f"{r['prompt_tps_off']:.1f} | {r['prompt_tps_on']:.1f} | {'yes' if r['same_text'] else 'no'} |")
