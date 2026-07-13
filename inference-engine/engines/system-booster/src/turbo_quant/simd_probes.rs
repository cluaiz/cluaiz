//! Bare-Metal SIMD Probes
//! Strictly gated by OS/Arch config macros to guarantee cross-OS compilation.

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub fn check_avx2_support() -> bool {
    use std::arch::asm;
    let ebx: u32;
    unsafe {
        // CPUID EAX=7, ECX=0 returns extended features in EBX, ECX, EDX
        // AVX2 is bit 5 of EBX.
        asm!(
            "push rbx",        // Save rbx (LLVM reserves it on x86_64 sometimes)
            "cpuid",
            "mov {0:e}, ebx",  // extract ebx
            "pop rbx",         // Restore rbx
            out(reg) ebx,
            inout("eax") 7 => _,
            inout("ecx") 0 => _,
            out("edx") _,
        );
    }
    (ebx & (1 << 5)) != 0
}

#[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
pub fn check_avx2_support() -> bool {
    // Graceful fallback for ARM / Apple Silicon / Raspberry Pi
    false
}
