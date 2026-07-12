use watch_rs::config::{suggest_subtitle_language, get_language_name};

#[test]
fn test_get_language_name_indonesian() {
    assert_eq!(get_language_name("id"), "Indonesian");
}

#[test]
fn test_get_language_name_english() {
    assert_eq!(get_language_name("en"), "English");
}

#[test]
fn test_get_language_name_french() {
    assert_eq!(get_language_name("fr"), "French");
}

#[test]
fn test_get_language_name_unknown() {
    assert_eq!(get_language_name("xx"), "Unknown");
}

#[test]
fn test_get_language_name_empty() {
    assert_eq!(get_language_name(""), "Unknown");
}

#[test]
fn test_suggest_subtitle_video_lang_manual_available() {
    // Video in Indonesian, manual Indonesian available → use id
    let manual = vec!["en".to_string(), "id".to_string()];
    let auto = vec!["en".to_string()];
    assert_eq!(
        suggest_subtitle_language(Some("id"), &manual, &auto),
        "id"
    );
}

#[test]
fn test_suggest_subtitle_video_lang_not_manual_fallback_en() {
    // Video in French, no manual French, manual English available → use en
    let manual = vec!["en".to_string(), "id".to_string()];
    let auto = vec!["en".to_string()];
    assert_eq!(
        suggest_subtitle_language(Some("fr"), &manual, &auto),
        "en"
    );
}

#[test]
fn test_suggest_subtitle_no_video_lang_default_en() {
    // No video language → default to en
    let manual = vec!["en".to_string(), "id".to_string()];
    let auto = vec!["en".to_string()];
    assert_eq!(
        suggest_subtitle_language(None, &manual, &auto),
        "en"
    );
}

#[test]
fn test_suggest_subtitle_video_lang_auto_available() {
    // Video in Indonesian, no manual Indonesian, auto Indonesian available → use id
    let manual = vec!["en".to_string()];
    let auto = vec!["id".to_string(), "en".to_string()];
    assert_eq!(
        suggest_subtitle_language(Some("id"), &manual, &auto),
        "id"
    );
}

#[test]
fn test_suggest_subtitle_no_match_fallback_video_lang() {
    // Video in Japanese, no manual or auto Japanese, no English → use ja
    let manual = vec!["ko".to_string()];
    let auto = vec!["zh".to_string()];
    assert_eq!(
        suggest_subtitle_language(Some("ja"), &manual, &auto),
        "ja"
    );
}

#[test]
fn test_suggest_subtitle_no_video_lang_no_en_fallback() {
    // No video language, no English subs → vid_lang defaults to "en", but
    // no manual or auto "en" → returns "en" (the default)
    let manual = vec!["ko".to_string()];
    let auto = vec!["zh".to_string()];
    assert_eq!(
        suggest_subtitle_language(None, &manual, &auto),
        "en"
    );
}

#[test]
fn test_suggest_subtitle_manual_en_only() {
    // Only manual English available, video in Spanish
    let manual = vec!["en".to_string()];
    let auto: Vec<String> = vec![];
    assert_eq!(
        suggest_subtitle_language(Some("es"), &manual, &auto),
        "en"
    );
}

#[test]
fn test_suggest_subtitle_auto_en_only() {
    // Only auto English available, video in Spanish
    let manual: Vec<String> = vec![];
    let auto = vec!["en".to_string()];
    assert_eq!(
        suggest_subtitle_language(Some("es"), &manual, &auto),
        "en"
    );
}
