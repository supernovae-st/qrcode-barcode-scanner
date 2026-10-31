//! Pixel-level regressions for linear formats, through the public scanner.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use qrcode_ai_scanner::{ImageInput, ScanProfile, Scanner, Symbology};

fn assert_read(bytes: &[u8], symbology: Symbology, text: &str) {
    let mut config = ScanProfile::Full.config();
    config.budget_ms = None;
    let report = Scanner::builder()
        .profile(ScanProfile::Custom(config))
        .build()
        .scan(ImageInput::encoded(bytes))
        .expect("the regression image is valid");
    assert_eq!(report.trace.budget_exhausted, Some(false));
    assert_eq!(report.trace.engine_panics, 0);
    assert_eq!(report.detections.len(), 1, "{report:#?}");
    let detection = &report.detections[0];
    assert_eq!(detection.symbology, symbology);
    assert_eq!(detection.content.text, text);
    assert_eq!(detection.content.raw, text.as_bytes());
}

#[test]
fn small_databar_preserves_its_gtin() {
    assert_read(
        include_bytes!("../../../fixtures/symbology/databar-small-regression.png"),
        Symbology::DataBar,
        "09506000134352",
    );
}

#[test]
fn bwipp_code93_preserves_exact_content() {
    assert_read(
        include_bytes!("../../../fixtures/symbology/code93-bwipp-checked.png"),
        Symbology::Code93,
        "SCANNER-93",
    );
}

#[test]
fn code93_without_required_check_characters_is_refused() {
    let mut config = ScanProfile::Full.config();
    config.budget_ms = None;
    let report = Scanner::builder()
        .profile(ScanProfile::Custom(config))
        .build()
        .scan(ImageInput::encoded(include_bytes!(
            "../../../fixtures/invalid/code93-missing-checks.png"
        )))
        .expect("the image is valid, its symbol is not");
    assert_eq!(report.trace.budget_exhausted, Some(false));
    assert_eq!(report.trace.engine_panics, 0);
    assert!(report.detections.is_empty(), "{report:#?}");
}

#[test]
fn databar_expanded_with_an_illegal_finder_sequence_is_refused() {
    // A,C,F,D is not a legal sequence. The check character was recomputed
    // to 867, so a checksum-only acceptance would still decode the payload.
    let mut config = ScanProfile::Full.config();
    config.budget_ms = None;
    let report = Scanner::builder()
        .profile(ScanProfile::Custom(config))
        .build()
        .scan(ImageInput::encoded(include_bytes!(
            "../../../fixtures/invalid/databar-expanded-acfd.png"
        )))
        .expect("valid image with an invalid symbol");
    assert_eq!(report.trace.budget_exhausted, Some(false));
    assert_eq!(report.trace.engine_panics, 0);
    assert!(report.detections.is_empty(), "{report:#?}");
}

#[test]
fn bwipp_databar_expanded_preserves_its_element_string() {
    assert_read(
        include_bytes!("../../../fixtures/symbology/databar-expanded-bwipp-regression.png"),
        Symbology::DataBarExpanded,
        "(01)09506000134352(10)DEMO",
    );
}
