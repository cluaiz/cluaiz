"""CEP through the C library (04-implementation-plan P5, tests CEP-F2..F4) with Python ctypes.

    python test/cep/ffi_python.py --lib target/release/cluaiz_cep.dll --home <isolated CLUAIZ_HOME>

The library starts the Engine inside this process with that home. Never point it at the owner's
own home.
"""
import argparse
import ctypes
import json
import os
import sys
import threading
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--lib", required=True)
    parser.add_argument("--home", required=True)
    args = parser.parse_args()
    os.environ["CLUAIZ_HOME"] = args.home
    token_before = open(os.path.join(args.home, "session.token"), encoding="utf-8").read()

    # Python looks for a DLL's dependencies only in a few folders, not PATH: add the GPU runtime's
    # (CUDA_PATH/bin holds cuBLAS for a CUDA build)
    if hasattr(os, "add_dll_directory"):
        for folder in [os.environ.get("CUDA_PATH", "")]:
            for sub in ("bin", os.path.join("bin", "x64")):
                if folder and os.path.isdir(os.path.join(folder, sub)):
                    os.add_dll_directory(os.path.join(folder, sub))
    lib = ctypes.CDLL(os.path.abspath(args.lib))
    lib.cep_call.restype, lib.cep_call.argtypes = ctypes.c_void_p, [ctypes.c_char_p]
    lib.cep_free.restype, lib.cep_free.argtypes = None, [ctypes.c_void_p]

    def raw(message: bytes):
        pointer = lib.cep_call(message)
        if not pointer:
            return None
        try:
            return json.loads(ctypes.string_at(pointer).decode("utf-8"))
        finally:
            lib.cep_free(pointer)

    def cep(method, **params):
        return raw(json.dumps({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}).encode())

    results = []

    def check(name, ok, detail):
        results.append(ok)
        print(f"{'PASS' if ok else 'FAIL'} {name:36s} {detail}", flush=True)

    started = time.time()
    info = cep("system.info")["result"]
    check("system.info (first call starts Engine)", info["engine"] == "Cluaiz Engine", f"{time.time() - started:.1f} s, version {info['version']}")
    names = [m["name"] for m in cep("rpc.discover")["result"]["methods"]]
    check("rpc.discover", "fs.read" in names and "tasks.get" in names, f"{len(names)} methods")
    batch = raw(json.dumps([{"jsonrpc": "2.0", "id": "a", "method": "tasks.list"},
                            {"jsonrpc": "2.0", "method": "tasks.list"},
                            {"jsonrpc": "2.0", "id": "b", "method": "nope.nope"}]).encode())
    check("batch in order, notification silent", [r["id"] for r in batch] == ["a", "b"] and batch[1]["error"]["code"] == -32601, batch)
    check("notification only -> NULL", raw(b'{"jsonrpc":"2.0","method":"tasks.list"}') is None, "")
    check("NULL input -> parse error", raw(None)["error"]["code"] == -32700, "")
    check("invalid UTF-8 -> parse error", raw(b'{"jsonrpc":"2.0","id":1,"method":"\xff\xfe"}')["error"]["code"] == -32700, "")
    check("owner can write", cep("fs.write", path="ffi-check.txt", text="from ffi").get("result", {}).get("bytes") == 8, "")
    check("and read back", cep("fs.read", path="ffi-check.txt")["result"]["text"] == "from ffi", "")
    cep("fs.delete", path="ffi-check.txt")

    errors = []

    def worker():
        for _ in range(100):
            reply = cep("tasks.list")
            if "result" not in reply:
                errors.append(reply)
    threads = [threading.Thread(target=worker) for _ in range(4)]
    started = time.time()
    [t.start() for t in threads]
    [t.join() for t in threads]
    check("4 threads x 100 calls", not errors, f"{time.time() - started:.2f} s, {len(errors)} errors")

    token_after = open(os.path.join(args.home, "session.token"), encoding="utf-8").read()
    check("home session token left alone", token_before == token_after, "")
    print(f"\n{sum(results)} of {len(results)} passed")
    return 0 if all(results) else 1


if __name__ == "__main__":
    sys.exit(main())
