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

#[test]
fn artistic_qr_texture_is_not_a_code39_payload() {
    // Unchanged console sample door-github-orb-custom-light.png, SHA-256
    // fb8a1094926910e261df6733ee5b39a7af1fde5e9464dd55c6e504860ceb7c20.
    let bytes = include_bytes!("../../../fixtures/artistic/github-orb-code39-regression.png");
    for profile in [ScanProfile::Fast, ScanProfile::Frame, ScanProfile::Full] {
        let mut config = profile.clone().config();
        config.budget_ms = None;
        let report = Scanner::builder()
            .profile(ScanProfile::Custom(config))
            .build()
            .scan(ImageInput::encoded(bytes))
            .unwrap();
        let found: Vec<_> = report
            .detections
            .iter()
            .map(|d| (d.symbology, d.content.text.as_str()))
            .collect();
        assert_eq!(
            found,
            [(Symbology::QrCode, "https://qrc-ai.com/QVR9a")],
            "{profile:?}: {:?}",
            report.trace
        );
        assert_eq!(report.trace.budget_exhausted, Some(false));
        assert_eq!(report.trace.engine_panics, 0);
    }
}

#[test]
fn a_genuine_single_character_code39_still_decodes() {
    // Independently encoded by bwip-js 4.11.4: Code39 "J", scale 3,
    // height 16, padding 16, no optional checksum.
    let bytes = include_bytes!("../../../fixtures/symbology/code39-short-j.png");
    for profile in [ScanProfile::Fast, ScanProfile::Frame, ScanProfile::Full] {
        let mut config = profile.clone().config();
        config.budget_ms = None;
        let report = Scanner::builder()
            .profile(ScanProfile::Custom(config))
            .build()
            .scan(ImageInput::encoded(bytes))
            .unwrap();
        let found: Vec<_> = report
            .detections
            .iter()
            .map(|d| (d.symbology, d.content.text.as_str()))
            .collect();
        assert_eq!(
            found,
            [(Symbology::Code39, "J")],
            "{profile:?}: {:?}",
            report.trace
        );
        assert_eq!(report.detections[0].content.raw, b"J");
        assert_eq!(report.detections[0].meta.version, None);
        assert_eq!(report.trace.engine_panics, 0);
    }
}

#[test]
fn a_genuine_single_character_code39_beside_a_qr_still_decodes() {
    let barcode = image::load_from_memory(include_bytes!(
        "../../../fixtures/symbology/code39-short-j.png"
    ))
    .unwrap()
    .to_luma8();
    let text = "https://qrc-ai.com/code39-neighbor";
    let qr = qrcode::QrCode::new(text.as_bytes())
        .unwrap()
        .render::<image::Luma<u8>>()
        .module_dimensions(12, 12)
        .build();
    let mut canvas = image::GrayImage::from_pixel(
        qr.width() + barcode.width() + 40,
        qr.height().max(barcode.height()) + 40,
        image::Luma([255]),
    );
    image::imageops::replace(&mut canvas, &qr, 20, 20);
    image::imageops::replace(&mut canvas, &barcode, i64::from(qr.width() + 20), 20);
    assert!(canvas.width() > 512);
    for profile in [ScanProfile::Fast, ScanProfile::Frame, ScanProfile::Full] {
        let mut config = profile.clone().config();
        config.budget_ms = None;
        let report = Scanner::builder()
            .profile(ScanProfile::Custom(config))
            .build()
            .scan(ImageInput::luma8(
                canvas.as_raw(),
                canvas.width(),
                canvas.height(),
            ))
            .unwrap();
        let found: Vec<_> = report
            .detections
            .iter()
            .map(|d| (d.symbology, d.content.text.as_str()))
            .collect();
        assert_eq!(
            found,
            [(Symbology::QrCode, text), (Symbology::Code39, "J")],
            "{profile:?}: {:?}",
            report.trace
        );
    }
}
