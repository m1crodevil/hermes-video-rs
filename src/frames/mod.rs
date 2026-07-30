pub mod metadata;
pub mod timestamp;

pub const MAX_FPS: f32 = 2.0;
const MAX_READ_DIMENSION: u32 = 1998;

pub struct VideoMetadata {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub codec: String,
}

pub struct FrameMeta {
    pub engine: String,
    pub candidate_count: usize,
    pub selected_count: usize,
    pub deduped_count: u32,
    pub fallback: bool,
    pub dropped_out_of_window: usize,
}

pub use metadata::get_metadata;
pub use timestamp::extract_at_timestamps;

/// Pick `n` evenly-spaced items from a slice (always first + last).
pub(crate) fn even_indices(count: usize, n: usize) -> Vec<usize> {
    if n >= count {
        return (0..count).collect();
    }
    if n <= 1 {
        return vec![0];
    }
    (0..n)
        .map(|i| (i * (count - 1) / (n - 1)) as usize)
        .collect()
}

pub(crate) fn scale_filter(resolution: u32) -> String {
    format!(
        "scale=w='min({resolution},iw)':h='min({MAX_READ_DIMENSION},ih)':force_original_aspect_ratio=decrease:force_divisible_by=2"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn even_indices_zero_count() {
        assert_eq!(even_indices(0, 5), Vec::<usize>::new());
    }

    #[test]
    fn even_indices_zero_n() {
        assert_eq!(even_indices(5, 0), vec![0]);
    }

    #[test]
    fn even_indices_n_gte_count() {
        assert_eq!(even_indices(3, 10), vec![0, 1, 2]);
    }

    #[test]
    fn even_indices_three_of_ten() {
        assert_eq!(even_indices(10, 3), vec![0, 4, 9]);
    }

    #[test]
    fn even_indices_ten_of_hundred() {
        let result = even_indices(100, 10);
        assert_eq!(result.len(), 10);
        assert_eq!(result[0], 0);
        assert_eq!(result[9], 99);
        for i in 1..result.len() {
            assert!(result[i] > result[i - 1]);
        }
    }

    #[test]
    fn even_indices_one_of_one() {
        assert_eq!(even_indices(1, 1), vec![0]);
    }

    #[test]
    fn scale_filter_contains_expected_values() {
        let filter = scale_filter(512);
        assert!(filter.starts_with("scale="));
        assert!(filter.contains("512"));
        assert!(filter.contains("1998"));
    }

    #[test]
    fn even_indices_count1_n1() {
        assert_eq!(even_indices(1, 1), vec![0]);
    }

    #[test]
    fn scale_filter_minimum_resolution() {
        let filter = scale_filter(128);
        assert!(filter.contains("128"));
        assert!(filter.contains("1998"));
        assert!(filter.starts_with("scale="));
    }

    #[test]
    fn scale_filter_maximum_resolution() {
        let filter = scale_filter(4096);
        assert!(filter.contains("4096"));
        assert!(filter.contains("1998"));
        assert!(filter.starts_with("scale="));
    }
}
