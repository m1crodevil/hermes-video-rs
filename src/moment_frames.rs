use crate::output::FrameInfo;
use crate::moments::KeyMoment;

/// Maximum gap (in seconds) between moments to deduplicate them.
const MOMENT_DEDUP_TOLERANCE: f64 = 1.0;

/// Maximum gap (in seconds) when matching a moment to a frame.
const FRAME_MATCH_TOLERANCE: f64 = 2.0;

/// Extract timestamps from key moments, optionally filtered by max priority.
///
/// Filters moments by priority (keeping those with priority <= `max_priority`),
/// parses their string timestamps to `f64`, sorts them, and deduplicates
/// entries within `MOMENT_DEDUP_TOLERANCE` seconds.
pub fn get_timestamps_from_moments(
    moments: &[KeyMoment],
    max_priority: Option<u32>,
) -> Vec<f64> {
    let mut timestamps: Vec<f64> = moments
        .iter()
        .filter(|m| max_priority.map_or(true, |mp| m.priority <= mp))
        .filter_map(|m| {
            // timestamp is already f64, but we keep the original string
            // as backup via parse_timestamp
            Some(m.timestamp)
        })
        .filter(|t| *t >= 0.0)
        .collect();

    timestamps.sort_by(|a, b| a.partial_cmp(b).unwrap());

    // Deduplicate within MOMENT_DEDUP_TOLERANCE
    dedup_timestamps(&timestamps, MOMENT_DEDUP_TOLERANCE)
}

/// Deduplicate sorted timestamps within a tolerance, keeping the first.
fn dedup_timestamps(timestamps: &[f64], tolerance: f64) -> Vec<f64> {
    if timestamps.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::with_capacity(timestamps.len());
    let mut last = timestamps[0];
    result.push(last);

    for &t in &timestamps[1..] {
        if (t - last).abs() >= tolerance {
            result.push(t);
            last = t;
        }
    }

    result
}

/// Match each key moment to the nearest frame within FRAME_MATCH_TOLERANCE.
///
/// Builds an internal map of frame timestamps → paths, then for each moment
/// finds the closest frame. Sets `moment.frame_path` to the matched path,
/// or leaves it as `None` if no frame is within tolerance.
pub fn update_moments_with_frames(moments: &mut Vec<KeyMoment>, frames: &[FrameInfo]) {
    if frames.is_empty() || moments.is_empty() {
        return;
    }

    // Frames should already be sorted by timestamp, but ensure it
    let mut sorted_frames = frames.to_vec();
    sorted_frames.sort_by(|a, b| a.timestamp.partial_cmp(&b.timestamp).unwrap());

    for moment in moments.iter_mut() {
        moment.frame_path = find_closest_frame(moment.timestamp, &sorted_frames);
    }
}

/// Find the closest frame to a given timestamp within FRAME_MATCH_TOLERANCE.
///
/// Uses binary search for efficiency. Returns `None` if no frame is within
/// tolerance.
fn find_closest_frame(target: f64, frames: &[FrameInfo]) -> Option<String> {
    if frames.is_empty() {
        return None;
    }

    // Binary search for the insertion point
    let idx = frames.partition_point(|f| f.timestamp < target);

    // Check the frame at `idx` (first frame >= target)
    // and the frame at `idx - 1` (last frame < target)
    let candidates: Vec<&FrameInfo> = {
        let mut c = Vec::with_capacity(2);
        if idx < frames.len() {
            c.push(&frames[idx]);
        }
        if idx > 0 {
            c.push(&frames[idx - 1]);
        }
        c
    };

    let mut best: Option<(&FrameInfo, f64)> = None;

    for frame in candidates {
        let dist = (frame.timestamp - target).abs();
        if dist <= FRAME_MATCH_TOLERANCE {
            if best.is_none() || dist < best.unwrap().1 {
                best = Some((frame, dist));
            }
        }
    }

    best.map(|(f, _)| f.path.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::moments::KeyMoment;

    fn make_moment(timestamp: f64, word: &str, priority: u32) -> KeyMoment {
        KeyMoment {
            timestamp,
            timestamp_fmt: crate::moments::format_timestamp(timestamp),
            word: word.to_string(),
            context: "test context".to_string(),
            reason: "claim".to_string(),
            question: "test?".to_string(),
            priority,
            frame_path: None,
        }
    }

    fn make_frame(timestamp: f64, path: &str) -> FrameInfo {
        FrameInfo {
            path: path.to_string(),
            timestamp,
            reason: "test".to_string(),
            scene_score: None,
        }
    }

    #[test]
    fn test_get_timestamps_basic() {
        let moments = vec![
            make_moment(10.0, "a", 1),
            make_moment(30.0, "b", 2),
            make_moment(60.0, "c", 3),
        ];
        let ts = get_timestamps_from_moments(&moments, None);
        assert_eq!(ts, vec![10.0, 30.0, 60.0]);
    }

    #[test]
    fn test_get_timestamps_with_priority_filter() {
        let moments = vec![
            make_moment(10.0, "a", 1),
            make_moment(20.0, "b", 2),
            make_moment(30.0, "c", 3),
            make_moment(40.0, "d", 4),
        ];
        // max_priority=2 should keep priorities 1 and 2
        let ts = get_timestamps_from_moments(&moments, Some(2));
        assert_eq!(ts, vec![10.0, 20.0]);
    }

    #[test]
    fn test_get_timestamps_dedup_within_tolerance() {
        let moments = vec![
            make_moment(10.0, "a", 1),
            make_moment(10.3, "b", 1), // within 1.0s of 10.0
            make_moment(11.5, "c", 1), // > 1.0s from 10.3
            make_moment(50.0, "d", 1),
        ];
        let ts = get_timestamps_from_moments(&moments, None);
        // 10.3 is within 1.0s of 10.0, so deduped; 11.5 stays
        assert_eq!(ts, vec![10.0, 11.5, 50.0]);
    }

    #[test]
    fn test_get_timestamps_empty() {
        let moments: Vec<KeyMoment> = vec![];
        let ts = get_timestamps_from_moments(&moments, None);
        assert!(ts.is_empty());
    }

    #[test]
    fn test_get_timestamps_sorted() {
        let moments = vec![
            make_moment(60.0, "a", 1),
            make_moment(10.0, "b", 1),
            make_moment(30.0, "c", 1),
        ];
        let ts = get_timestamps_from_moments(&moments, None);
        assert_eq!(ts, vec![10.0, 30.0, 60.0]);
    }

    #[test]
    fn test_update_moments_with_frames_basic() {
        let frames = vec![
            make_frame(10.0, "/frames/f_01.jpg"),
            make_frame(20.0, "/frames/f_02.jpg"),
            make_frame(30.0, "/frames/f_03.jpg"),
        ];
        let mut moments = vec![
            make_moment(10.5, "a", 1),
            make_moment(20.3, "b", 2),
        ];
        update_moments_with_frames(&mut moments, &frames);
        assert_eq!(
            moments[0].frame_path.as_deref(),
            Some("/frames/f_01.jpg")
        );
        assert_eq!(
            moments[1].frame_path.as_deref(),
            Some("/frames/f_02.jpg")
        );
    }

    #[test]
    fn test_update_moments_no_frame_in_tolerance() {
        let frames = vec![
            make_frame(10.0, "/frames/f_01.jpg"),
            make_frame(20.0, "/frames/f_02.jpg"),
        ];
        let mut moments = vec![make_moment(15.0, "a", 1)]; // 5.0s away from nearest
        update_moments_with_frames(&mut moments, &frames);
        assert!(moments[0].frame_path.is_none());
    }

    #[test]
    fn test_update_moments_empty_frames() {
        let frames: Vec<FrameInfo> = vec![];
        let mut moments = vec![make_moment(10.0, "a", 1)];
        update_moments_with_frames(&mut moments, &frames);
        assert!(moments[0].frame_path.is_none());
    }

    #[test]
    fn test_find_closest_frame_exact_match() {
        let frames = vec![
            make_frame(10.0, "/f1.jpg"),
            make_frame(20.0, "/f2.jpg"),
        ];
        assert_eq!(
            find_closest_frame(20.0, &frames),
            Some("/f2.jpg".to_string())
        );
    }

    #[test]
    fn test_find_closest_frame_between_two() {
        let frames = vec![
            make_frame(10.0, "/f1.jpg"),
            make_frame(20.0, "/f2.jpg"),
        ];
        // 11.5 is closer to 10.0 (dist=1.5) than 20.0 (dist=8.5)
        assert_eq!(
            find_closest_frame(11.5, &frames),
            Some("/f1.jpg".to_string())
        );
    }

    #[test]
    fn test_find_closest_frame_empty() {
        let frames: Vec<FrameInfo> = vec![];
        assert!(find_closest_frame(10.0, &frames).is_none());
    }
}
