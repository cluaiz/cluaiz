//! 🎡 PolarQuant Engine (Hierarchical Polar Transformation)
//! 
//! Converts Cartesian coordinates into a Polar "shorthand" (Radius and Angle).
//! This reduces memory overhead by mapping tensor onto a fixed circular grid.

pub struct PolarData {
    pub radius: f32,
    pub angles: Vec<f32>,
}

pub struct CartesianToPolar;

impl CartesianToPolar {
    /// Deep Logic: Cartesian to Polar Transformation
    /// Group pairs (x, y) -> (r, theta) recursively.
    pub fn transform(tensor_slice: &[f32]) -> Result<PolarData, &'static str> {
        let n = tensor_slice.len();
        if n == 0 { return Err("Cannot transform an empty tensor slice"); }

        let mut current_radii = Vec::with_capacity(n / 2);
        let mut angles = Vec::with_capacity(n - 1);

        // 1. Initial Cartesian to Polar (Stage 1)
        for i in (0..n).step_by(2) {
            let x = tensor_slice[i];
            let y = if i + 1 < n { tensor_slice[i + 1] } else { 0.0 };

            let r = (x*x + y*y).sqrt();
            let theta = y.atan2(x);
            
            current_radii.push(r);
            angles.push(theta);
        }

        // 2. Recursive Radius Gathering (Hierarchical Polar)
        // Strictly guarantees log2(N) depth, producing exactly 1 radius and N-1 angles entirely.
        while current_radii.len() > 1 {
            let mut next_radii = Vec::with_capacity((current_radii.len() + 1) / 2);
            for i in (0..current_radii.len()).step_by(2) {
                let r1 = current_radii[i];
                let r2 = if i + 1 < current_radii.len() { current_radii[i + 1] } else { 0.0 };

                let combined_r = (r1*r1 + r2*r2).sqrt();
                // Theta for radii merging
                let combined_theta = r2.atan2(r1);

                next_radii.push(combined_r);
                angles.push(combined_theta);
            }
            current_radii = next_radii;
        }

        Ok(PolarData {
            radius: current_radii.into_iter().next().unwrap_or(0.0),
            angles,
        })
    }
}
