"""Takes GPU memory from another process while the Engine loads (run 21, step-down test).
Usage: vram_hog.py <start> <GB to leave free> <hold s>. <start> is seconds to wait, or `load` to wait
until another process has taken 1 GB (the load has begun), so the memory goes while the model loads.
Uses the CUDA runtime through ctypes."""
import ctypes, glob, sys, time

start, leave_gb, hold = sys.argv[1], float(sys.argv[2]), float(sys.argv[3])
paths = sorted(glob.glob("/usr/local/cuda/lib64/libcudart.so*")) or ["libcudart.so"]
cuda = ctypes.CDLL(paths[0])
free, total = ctypes.c_size_t(), ctypes.c_size_t()


def free_gb():
    cuda.cudaMemGetInfo(ctypes.byref(free), ctypes.byref(total))
    return free.value / 1024**3


if start == "load":
    before = free_gb()
    deadline = time.time() + 600
    while free_gb() > before - 1.0 and time.time() < deadline:
        time.sleep(0.2)
else:
    time.sleep(float(start))
now = free_gb()
size = int((now - leave_gb) * 1024**3)
ptr = ctypes.c_void_p()
rc = cuda.cudaMalloc(ctypes.byref(ptr), ctypes.c_size_t(max(size, 0)))
print(f"{time.strftime('%H:%M:%S')} free {now:.2f} GB, took {size / 1024**3:.2f} GB, rc {rc}", flush=True)
time.sleep(hold)
cuda.cudaFree(ptr)
print(f"{time.strftime('%H:%M:%S')} released", flush=True)
