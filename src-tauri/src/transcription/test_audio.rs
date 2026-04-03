//! Bundled test audio samples used by the UI "Test" buttons.

/// English sample: "systems check, test test test"
pub fn english_test_wav() -> &'static [u8] {
    include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/test-audio/english-test.wav"
    ))
}

/// Hebrew sample: "בדיקת מערכות. ניסיון ניסיון ניסיון"
pub fn hebrew_test_wav() -> &'static [u8] {
    include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/test-audio/hebrew-test.wav"
    ))
}
