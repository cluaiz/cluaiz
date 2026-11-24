// Replay of run 44's expert prefetch call pattern on the real model file: each call asks for `per_call`
// random 1 MiB expert slices out of the expert region; "every" asks every call (fork before the fix),
// "once" skips slices already asked (fork after the fix, Windows). Then the slices are touched as the CPU
// matmul would. Prints the total time in PrefetchVirtualMemory and in the touches.
// Usage: replay <file> <region_start_mib> <region_mib> <calls> <per_call> <every|once|none> <seed>
use std::collections::HashSet;
use std::ffi::c_void;
use std::time::Instant;

#[repr(C)]
struct Range { addr: *mut c_void, size: usize }

#[link(name = "kernel32")]
extern "system" {
    fn CreateFileW(n: *const u16, a: u32, s: u32, sa: *mut c_void, d: u32, f: u32, t: *mut c_void) -> *mut c_void;
    fn CreateFileMappingW(h: *mut c_void, sa: *mut c_void, p: u32, hi: u32, lo: u32, n: *const u16) -> *mut c_void;
    fn MapViewOfFile(h: *mut c_void, a: u32, hi: u32, lo: u32, n: usize) -> *mut c_void;
    fn PrefetchVirtualMemory(p: *mut c_void, n: usize, r: *mut Range, f: u32) -> i32;
    fn GetCurrentProcess() -> *mut c_void;
}

fn wide(s: &str) -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() }

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let path = &a[1];
    let start: usize = a[2].parse().unwrap();
    let region: usize = a[3].parse().unwrap();
    let calls: usize = a[4].parse().unwrap();
    let per_call: usize = a[5].parse().unwrap();
    let mode = a[6].as_str();
    let mut seed: u64 = a[7].parse().unwrap();
    let h = unsafe { CreateFileW(wide(path).as_ptr(), 0x80000000, 1, std::ptr::null_mut(), 3, 0, std::ptr::null_mut()) };
    let m = unsafe { CreateFileMappingW(h, std::ptr::null_mut(), 2, 0, 0, std::ptr::null()) };
    let base = unsafe { MapViewOfFile(m, 4, 0, 0, 0) } as *mut u8;
    assert!(!base.is_null());
    let mut asked: HashSet<usize> = HashSet::new();
    let (mut t_pvm, mut t_touch, mut asked_mib, mut skipped_mib) = (0f64, 0f64, 0usize, 0usize);
    let mut sum = 0u64;
    for _ in 0..calls {
        let mut sel = Vec::with_capacity(per_call);
        for _ in 0..per_call {
            seed ^= seed << 13; seed ^= seed >> 7; seed ^= seed << 17;
            sel.push((seed as usize) % region);
        }
        let mut v: Vec<Range> = Vec::new();
        for &s in &sel {
            if mode == "once" && !asked.insert(s) { skipped_mib += 1; continue; }
            if mode == "none" { continue; }
            v.push(Range { addr: unsafe { base.add((start + s) << 20) } as *mut c_void, size: 1 << 20 });
        }
        asked_mib += v.len();
        if !v.is_empty() {
            let t = Instant::now();
            unsafe { PrefetchVirtualMemory(GetCurrentProcess(), v.len(), v.as_mut_ptr(), 0) };
            t_pvm += t.elapsed().as_secs_f64();
        }
        let t = Instant::now();
        for &s in &sel {
            let p = unsafe { base.add((start + s) << 20) };
            let mut i = 0; while i < (1 << 20) { sum += unsafe { std::ptr::read_volatile(p.add(i)) } as u64; i += 4096; }
        }
        t_touch += t.elapsed().as_secs_f64();
    }
    println!("mode {mode}: {calls} calls x {per_call} MiB, asked {asked_mib} MiB, skipped {skipped_mib} MiB, prefetch {:.2} s, touch {:.2} s, total {:.2} s (sum {})",
        t_pvm, t_touch, t_pvm + t_touch, sum % 7);
}
