pub fn parse_time(value: Option<&str>) -> Option<f64> {
    let s = value?.trim();
    if s.is_empty() { return None; }
    let parts: Vec<&str> = s.split(':').collect();
    match parts.len() {
        1 => s.parse::<f64>().ok(),
        2 => {
            let min: f64 = parts[0].parse().ok()?;
            let sec: f64 = parts[1].parse().ok()?;
            Some(min * 60.0 + sec)
        }
        3 => {
            let hr: f64 = parts[0].parse().ok()?;
            let min: f64 = parts[1].parse().ok()?;
            let sec: f64 = parts[2].parse().ok()?;
            Some(hr * 3600.0 + min * 60.0 + sec)
        }
        _ => None,
    }
}

pub fn format_time(seconds: f64) -> String {
    let total = seconds.round() as u64;
    let (hr, rem) = (total / 3600, total % 3600);
    let (min, sec) = (rem / 60, rem % 60);
    if hr > 0 { format!("{}:{:02}:{:02}", hr, min, sec) }
    else { format!("{:02}:{:02}", min, sec) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn test_parse_seconds() { assert!((parse_time(Some("45.5")).unwrap() - 45.5).abs() < 0.01); }
    #[test] fn test_parse_mm_ss() { assert!((parse_time(Some("2:30")).unwrap() - 150.0).abs() < 0.01); }
    #[test] fn test_parse_hh_mm_ss() { assert!((parse_time(Some("1:30:00")).unwrap() - 5400.0).abs() < 0.01); }
    #[test] fn test_parse_none() { assert!(parse_time(None).is_none()); }
    #[test] fn test_format_time() { assert_eq!(format_time(90.0), "01:30"); }
    #[test] fn test_format_hours() { assert_eq!(format_time(3661.0), "1:01:01"); }
}
