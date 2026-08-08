//! 🌀 Random Rotation Engine (FWHT)
//! 
//! The Fast Walsh-Hadamard Transform (FWHT) is used to perform 
//! "tensor-oblivious" rotation of vectors. This spreads out outlier 
//! values and prepares the vector for high-quality quantization.

/// In-place Fast Walsh-Hadamard Transform
/// n must be a power of 2.
pub fn apply_fwht(tensor_slice: &mut [f32]) -> Result<(), &'static str> {
    let n = tensor_slice.len();
    if !n.is_power_of_two() { return Err("FWHT requires vector length to be a power of two"); }

    let mut h = 1;
    while h < n {
        for i in (0..n).step_by(h * 2) {
            for j in i..i+h {
                let x = tensor_slice[j];
                let y = tensor_slice[j + h];

                // --- BARE METAL BUTTERFLY ---
                #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
                unsafe {
                    let mut sum: f32;
                    let mut diff: f32;
                    // Properly using xmm_reg to route f32 into AVX instructions
                    std::arch::asm!(
                        "vaddss {sum}, {x}, {y}",
                        "vsubss {diff}, {x}, {y}",
                        x = in(xmm_reg) x,
                        y = in(xmm_reg) y,
                        sum = out(xmm_reg) sum,
                        diff = out(xmm_reg) diff,
                    );
                    tensor_slice[j] = sum;
                    tensor_slice[j + h] = diff;
                }
                
                #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
                {
                    tensor_slice[j] = x + y;
                    tensor_slice[j + h] = x - y;
                }
            }
        }
        h *= 2;
    }

    // Standard scaling for FWHT
    let scale = (n as f32).sqrt().recip();
    for x in tensor_slice.iter_mut() {
        *x *= scale;
    }

    Ok(())
}
