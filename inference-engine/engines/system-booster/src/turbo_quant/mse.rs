//! 📉 MSE Engine (Lloyd-Max Scalar Quantization)
//! 
//! Minimizes Mean Squared Reconstruction Error by finding the optimal 
//! mapping of continuous values to discrete symbols.

pub struct ScalarQuantizer {
    pub centroids: Vec<f32>,
}

impl ScalarQuantizer {
    /// Deep Logic: Beta-Distribution Aware Lloyd-Max
    /// Iteratively finds optimal centroids, assuming tensor_slice approximates Beta(1/2, (d-1)/2).
    /// After FWHT, the angles are heavily concentrated around pi/2 for high d.
    pub fn train_beta_aware(tensor_slice: &[f32], num_bins: usize, _dim: usize) -> Self {
        let mut centroids = vec![0.0; num_bins];
        
        // 1. Beta-Aware Initialization
        // Instead of uniform min-max, we initialize centroids heavily 
        // concentrated around the expected mean (pi/2) to mimic the Beta density.
        let mean = std::f32::consts::PI / 2.0;
        let spread = std::f32::consts::PI / 4.0; // Approximation of variance spread
        for i in 0..num_bins {
            // Map uniformly onto an S-curve (logit-like) as a rough inverse CDF for Beta
            let p = (i as f32 + 0.5) / (num_bins as f32);
            // using tan as a simple proxy for inverse CDF concentration
            let offset = (p * std::f32::consts::PI - std::f32::consts::PI/2.0).tan() * (spread / 3.0);
            centroids[i] = mean + offset;
        }

        // 2. Lloyd-Max Iterations
        for _ in 0..10 { // 10 iterations for convergence
            let mut clusters: Vec<Vec<f32>> = vec![vec![]; num_bins];
            
            // Partition tensor_slice into nearest clusters
            for &x in tensor_slice {
                let mut best_idx = 0;
                let mut min_dist = f32::INFINITY;
                for (idx, &c) in centroids.iter().enumerate() {
                    let d = (x - c).abs();
                    if d < min_dist {
                        min_dist = d;
                        best_idx = idx;
                    }
                }
                clusters[best_idx].push(x);
            }

            // Update centroids to cluster means
            for (idx, cluster) in clusters.iter().enumerate() {
                if !cluster.is_empty() {
                    let sum: f32 = cluster.iter().sum();
                    centroids[idx] = sum / (cluster.len() as f32);
                }
            }
        }

        Self { centroids }
    }

    /// Map a value to its nearest centroid index
    pub fn quantize(&self, x: f32) -> usize {
        let mut best_idx = 0;
        let mut min_dist = f32::INFINITY;
        for (idx, &c) in self.centroids.iter().enumerate() {
            let d = (x - c).abs();
            if d < min_dist {
                min_dist = d;
                best_idx = idx;
            }
        }
        best_idx
    }
}
