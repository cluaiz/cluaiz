//! ═══════════════════════════════════════════════════════════════════════
//!  Llama Kernel: Bare-Metal Assembly Kernels (AVX-512 / AVX2)
//! ═══════════════════════════════════════════════════════════════════════

#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

/// Hardware-aware mathematical dispatch interface.
pub trait BareMetalMath {
    unsafe fn ternary_dot_product(
        packed_weights: *const u8,
        activations: *const i8,
        output: *mut i32,
        count: usize,
    ) -> Result<(), &'static str>;
}

/// Industrial-standard ternary kernel using ` `.
pub struct Avx2MaddubsKernel;

#[cfg(target_arch = "x86_64")]
impl BareMetalMath for Avx2MaddubsKernel {
    unsafe fn ternary_dot_product(
        packed_weights: *const u8,
        activations: *const i8,
        output: *mut i32,
        count: usize,
    ) -> Result<(), &'static str> {
        if count == 0 { return Ok(()); }
        if !count.is_multiple_of(32) {
            return Err("count must be a multiple of 32 for AVX2 maddubs path");
        }

        let mask_lo = _mm256_set1_epi8(0x03_i8);
        let ones    = _mm256_set1_epi16(1);
        let mut accumulator = _mm256_setzero_si256();

        for block_idx in 0..(count / 32) {
            let packed_offset = block_idx * 8;
            let act_offset    = block_idx * 32;

            let packed_256 = _mm256_set1_epi64x(
                std::ptr::read_unaligned(packed_weights.add(packed_offset) as *const i64)
            );
            
            let idx0 = _mm256_and_si256(packed_256, mask_lo);
            let idx1 = _mm256_and_si256(_mm256_srli_epi16(packed_256, 2), mask_lo);
            let idx2 = _mm256_and_si256(_mm256_srli_epi16(packed_256, 4), mask_lo);
            let idx3 = _mm256_and_si256(_mm256_srli_epi16(packed_256, 6), mask_lo);

            let merged_lo = _mm256_unpacklo_epi8(idx0, idx1);
            let merged_hi = _mm256_unpacklo_epi8(idx2, idx3);
            let unpacked_weights = _mm256_unpacklo_epi16(merged_lo, merged_hi);

            let acts = _mm256_loadu_si256(activations.add(act_offset) as *const __m256i);
            let products = _mm256_maddubs_epi16(unpacked_weights, acts);
            let widened = _mm256_madd_epi16(products, ones);
            accumulator = _mm256_add_epi32(accumulator, widened);
        }

        _mm256_storeu_si256(output as *mut __m256i, accumulator);
        Ok(())
    }
}

/// Universal 32-bit container kernel using epi32 shifts and epi16 madd.
pub struct Avx2BitNet32Kernel;

#[cfg(target_arch = "x86_64")]
impl BareMetalMath for Avx2BitNet32Kernel {
    unsafe fn ternary_dot_product(
        packed_weights: *const u8,
        activations: *const i8,
        output: *mut i32,
        count: usize,
    ) -> Result<(), &'static str> {
        if count == 0 { return Ok(()); }
        if !count.is_multiple_of(32) {
            return Err("count must be a multiple of 32 for AVX2 32-bit path");
        }

        let mask_lo = _mm256_set1_epi32(0x03030303);
        let mut accumulator = _mm256_setzero_si256();

        for block_idx in 0..(count / 32) {
            let packed_offset = block_idx * 8;
            let act_offset    = block_idx * 32;

            let packed_256 = _mm256_set1_epi64x(
                std::ptr::read_unaligned(packed_weights.add(packed_offset) as *const i64)
            );
            
            // Native 32-bit lane shifts to prevent boundary tax
            let idx0 = _mm256_and_si256(packed_256, mask_lo);
            let idx1 = _mm256_and_si256(_mm256_srli_epi32(packed_256, 2), mask_lo);
            let idx2 = _mm256_and_si256(_mm256_srli_epi32(packed_256, 4), mask_lo);
            let idx3 = _mm256_and_si256(_mm256_srli_epi32(packed_256, 6), mask_lo);

            let merged_lo = _mm256_unpacklo_epi8(idx0, idx1);
            let merged_hi = _mm256_unpacklo_epi8(idx2, idx3);
            let unpacked_weights_bytes = _mm256_unpacklo_epi16(merged_lo, merged_hi);

            // Word-Math Path: Expand 8-bit bytes to 16-bit words for safe madd
            let lower_128 = _mm256_castsi256_si128(unpacked_weights_bytes);
            let upper_128 = _mm256_extracti128_si256(unpacked_weights_bytes, 1);
            
            let weights_word_lo = _mm256_cvtepi8_epi16(lower_128);
            let weights_word_hi = _mm256_cvtepi8_epi16(upper_128);

            let acts = _mm256_loadu_si256(activations.add(act_offset) as *const __m256i);
            
            let acts_lower_128 = _mm256_castsi256_si128(acts);
            let acts_upper_128 = _mm256_extracti128_si256(acts, 1);

            let acts_word_lo = _mm256_cvtepi8_epi16(acts_lower_128);
            let acts_word_hi = _mm256_cvtepi8_epi16(acts_upper_128);

            let prod_lo = _mm256_madd_epi16(weights_word_lo, acts_word_lo);
            let prod_hi = _mm256_madd_epi16(weights_word_hi, acts_word_hi);


            // Accumulate directly into 32-bit registers (no maddubs needed)
            accumulator = _mm256_add_epi32(accumulator, prod_lo);
            accumulator = _mm256_add_epi32(accumulator, prod_hi);
        }

        _mm256_storeu_si256(output as *mut __m256i, accumulator);
        Ok(())
    }
}

/// Atomic Dynamic Dispatcher: Selects the correct BareMetalMath implementation at runtime.
pub struct KernelDispatcher;

impl KernelDispatcher {
    /// Inspects the model's container format to route to the optimal bare-metal kernel.
    pub unsafe fn dispatch_ternary_dot_product(
        is_32bit_container: bool,
        packed_weights: *const u8,
        activations: *const i8,
        output: *mut i32,
        count: usize,
    ) -> Result<(), &'static str> {
        if is_32bit_container {
            // Future-proof perfectly aligned 32-bit path
            Avx2BitNet32Kernel::ternary_dot_product(packed_weights, activations, output, count)
        } else {
            // Legacy 16-bit or 8-bit fallback path
            Avx2MaddubsKernel::ternary_dot_product(packed_weights, activations, output, count)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dispatcher_resolution() {
        // Just verify that compilation and resolution works correctly
        unsafe {
            let mut output = vec![0i32; 1];
            let packed = vec![0u8; 32];
            let acts = vec![0i8; 32];
            
            // Should route to 32-bit aligned path
            let res32 = KernelDispatcher::dispatch_ternary_dot_product(
                true,
                packed.as_ptr(),
                acts.as_ptr(),
                output.as_mut_ptr(),
                32
            );
            assert!(res32.is_ok());

            // Should route to legacy 16-bit path
            let res16 = KernelDispatcher::dispatch_ternary_dot_product(
                false,
                packed.as_ptr(),
                acts.as_ptr(),
                output.as_mut_ptr(),
                32
            );
            assert!(res16.is_ok());
        }
    }
}

/// 🚀 Sovereign Injection Point: Expose the AVX2 BitNet Kernel to C++ (GGML).
/// This allows `llama.cpp` to route `GGML_TYPE_TQ1_0` math operations to our ultra-fast Rust implementation.
#[no_mangle]
pub unsafe extern "C" fn cluaiz_fast_ternary_dot(
    packed_weights: *const u8,
    activations: *const i8,
    output: *mut i32,
    count: usize,
) -> i32 {
    #[cfg(target_arch = "x86_64")]
    {
        // 1. We assume 32-bit containerized alignment for modern BitNet models
        let result = KernelDispatcher::dispatch_ternary_dot_product(
            true, // is_32bit_container
            packed_weights,
            activations,
            output,
            count
        );
        match result {
            Ok(_) => 0, // 0 = Success
            Err(_) => -1, // -1 = Fallback to GGML default emulation
        }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        // Fallback for non-x86_64 architectures (will trigger ggml's emulation)
        -1
    }
}
