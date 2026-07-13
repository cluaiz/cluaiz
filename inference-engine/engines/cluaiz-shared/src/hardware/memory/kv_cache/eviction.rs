/// 🛡️ UniversalEvictor: Global LRU and memory cleanup logic.
pub struct UniversalEvictor;

impl UniversalEvictor {
    /// Identifies which blocks should be recycled based on last access timestamps.
    pub fn find_victims(access_map: &[(usize, u64)], count: usize) -> Vec<usize> {
        let mut sorted = access_map.to_vec();
        sorted.sort_by_key(|&(_, time)| time);
        
        sorted.iter().take(count).map(|&(idx, _)| idx).collect()
    }
}
