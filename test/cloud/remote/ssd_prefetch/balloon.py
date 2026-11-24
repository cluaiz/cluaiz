"""Hold <MiB> of touched anonymous memory until killed (pp_gap.sh: less room for the page cache in a cgroup)."""
import sys
import time

mib = int(sys.argv[1])
block = bytearray(mib * 1024 * 1024)
for i in range(0, len(block), 4096):
    block[i] = 1
print(f"balloon holds {mib} MiB", flush=True)
while True:
    time.sleep(60)
