# Per run: tg/pp from the tool's table, disk reads in the tg phase (the seconds before the process
# exit that 3 x 128 tokens at the measured speed took, plus the 1-token warm-up), free RAM then.
import csv, glob, os, re, sys, statistics as st
d = sys.argv[1]
rows = []
for md in sorted(glob.glob(os.path.join(d, 'r*_*.md'))):
    tag = os.path.basename(md)[:-3]
    rnd, cfg = tag.split('_', 1)
    txt = open(md, encoding='utf-8', errors='replace').read()
    def val(t):
        m = re.search(r'\|\s*' + t + r'\s*\|\s*([\d.]+)\s*\S+\s*([\d.]+)\s*\|', txt)
        return float(m.group(1)) if m else None
    pp, tg = val('pp512'), val('tg128')
    perf = os.path.join(d, tag + '.perf.csv')
    tgread = minav = None
    if os.path.exists(perf) and tg:
        s = list(csv.reader(open(perf, encoding='utf-8', errors='replace')))[1:]
        vals = []
        for r in s:
            try: vals.append((float(r[1]), float(r[2])))
            except: pass
        # drop the teardown seconds (free RAM jumps back by > 2 GB)
        while len(vals) > 2 and vals[-1][1] - vals[-2][1] > 2000: vals.pop()
        n = int(3 * 128 / tg) + 1
        tail = vals[-n:]
        tgread = sum(v[0] for v in tail) / 1e9
        minav = min(v[1] for v in tail)
    rows.append((int(rnd[1:]), cfg, pp, tg, tgread, minav))
rows.sort()
print('| round | config | pp512 | tg128 | GB read in tg phase | min free MiB in tg phase |')
print('|---|---|---|---|---|---|')
for r in rows:
    print('| %d | %s | %s | %s | %s | %s |' % (r[0], r[1], r[2], r[3], '%.2f' % r[4] if r[4] is not None else '?', '%.0f' % r[5] if r[5] is not None else '?'))
print()
thr = float(sys.argv[2]) if len(sys.argv) > 2 else 0.3
print('Median over rounds >= 2 with at most %.1f GB read in the tg phase:' % thr)
print('| config | tg128 median | n | tg128 values | pp512 median |')
print('|---|---|---|---|---|')
for cfg in dict.fromkeys(r[1] for r in rows):
    w = [r for r in rows if r[1] == cfg and r[0] >= 2 and r[3] and r[4] is not None and r[4] <= thr]
    tgs = [r[3] for r in w]; pps = [r[2] for r in w if r[2]]
    print('| %s | %s | %d | %s | %s |' % (cfg, '%.2f' % st.median(tgs) if tgs else '?', len(tgs), ', '.join('%.2f' % t for t in tgs), '%.2f' % st.median(pps) if pps else '?'))
