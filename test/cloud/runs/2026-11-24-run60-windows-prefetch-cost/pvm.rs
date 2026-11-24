// Micro-benchmark: cost of PrefetchVirtualMemory on a read-only file mapping, for pages that are
// (a) in the working set, (b) trimmed to the standby list, (c) possibly cold. Also the cost of touching
// the pages afterwards. Usage: pvm <file> <offset_mib> <n_ranges> <range_kib> <iters>
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
    fn SetProcessWorkingSetSize(p: *mut c_void, mn: usize, mx: usize) -> i32;
}

fn wide(s: &str) -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() }

fn touch(base: *const u8, ranges: &[(usize, usize)]) -> u64 {
    let mut s = 0u64;
    for &(o, n) in ranges { let mut i = 0; while i < n { s += unsafe { std::ptr::read_volatile(base.add(o + i)) } as u64; i += 4096; } }
    s
}

fn pvm(base: *mut u8, ranges: &[(usize, usize)]) -> f64 {
    let mut v: Vec<Range> = ranges.iter().map(|&(o, n)| Range { addr: unsafe { base.add(o) } as *mut c_void, size: n }).collect();
    let t = Instant::now();
    let ok = unsafe { PrefetchVirtualMemory(GetCurrentProcess(), v.len(), v.as_mut_ptr(), 0) };
    let us = t.elapsed().as_secs_f64() * 1e6;
    assert!(ok != 0, "PrefetchVirtualMemory failed");
    us
}

fn trim() { unsafe { SetProcessWorkingSetSize(GetCurrentProcess(), usize::MAX, usize::MAX); } }

fn median(mut v: Vec<f64>) -> f64 { v.sort_by(|a, b| a.partial_cmp(b).unwrap()); v[v.len() / 2] }

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (path, off_mib, n, kib, iters): (&str, usize, usize, usize, usize) =
        (&a[1], a[2].parse().unwrap(), a[3].parse().unwrap(), a[4].parse().unwrap(), a[5].parse().unwrap());
    let h = unsafe { CreateFileW(wide(path).as_ptr(), 0x80000000, 1, std::ptr::null_mut(), 3, 0, std::ptr::null_mut()) };
    assert!(h as isize != -1, "open failed");
    let m = unsafe { CreateFileMappingW(h, std::ptr::null_mut(), 2, 0, 0, std::ptr::null()) };
    let base = unsafe { MapViewOfFile(m, 4, 0, 0, 0) } as *mut u8;
    assert!(!base.is_null());
    // n ranges of kib KiB, 2 x range apart (experts of one tensor are not adjacent for a random selection)
    let ranges: Vec<(usize, usize)> = (0..n).map(|i| (off_mib * 1048576 + i * 2 * kib * 1024, kib * 1024)).collect();
    let total_mib = (n * kib) as f64 / 1024.0;

    // (d) first call on pages not touched by this process (may be cold or in standby): call time only
    let skip = a.len() > 6 && a[6] == "nopvm";
    let cold = if skip { 0.0 } else { pvm(base, &ranges) };
    let t = Instant::now(); touch(base, &ranges); let cold_touch = t.elapsed().as_secs_f64() * 1e6;

    // (a) resident in the working set
    touch(base, &ranges);
    let ws: Vec<f64> = (0..iters).map(|_| pvm(base, &ranges)).collect();
    let t = Instant::now(); for _ in 0..iters { touch(base, &ranges); } let ws_touch = t.elapsed().as_secs_f64() * 1e6 / iters as f64;

    // (b) trimmed to standby, then prefetch, then touch; and trimmed then touch without prefetch
    let (mut sb_pvm, mut sb_touch_after, mut sb_touch_plain) = (vec![], vec![], vec![]);
    for _ in 0..iters {
        touch(base, &ranges); trim();
        sb_pvm.push(pvm(base, &ranges));
        let t = Instant::now(); touch(base, &ranges); sb_touch_after.push(t.elapsed().as_secs_f64() * 1e6);
        trim();
        let t = Instant::now(); touch(base, &ranges); sb_touch_plain.push(t.elapsed().as_secs_f64() * 1e6);
    }
    println!("ranges {n} x {kib} KiB = {total_mib:.1} MiB at offset {off_mib} MiB, {iters} iterations");
    println!("cold-or-standby first call: pvm {cold:.0} us, then touch {cold_touch:.0} us");
    println!("in working set: pvm median {:.0} us (min {:.0}), touch {ws_touch:.0} us", median(ws.clone()), ws.iter().cloned().fold(f64::MAX, f64::min));
    println!("standby: pvm median {:.0} us, touch after pvm {:.0} us, touch without pvm {:.0} us",
        median(sb_pvm), median(sb_touch_after), median(sb_touch_plain));
}
