//! Mixed-family reads must preserve every decoded content, independent of the
//! order and position in which the third-party reader encounters the symbols.
//! Exact acceptance pins use each profile's schedule without a wall-clock
//! deadline; machine load must not turn a legitimate budget cut into a failure.

#![cfg(feature = "engine-rxing")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use qrcode_ai_scanner::{ImageInput, ScanProfile, Scanner, Symbology};

const BARCODE: &str = "STOCK-4827";
const QR: &str = "https://qrc-ai.com/mixed-symbols";

fn code128() -> image::GrayImage {
    // The console's unchanged standalone control.
    image::load_from_memory(include_bytes!(
        "../../../fixtures/symbology/code128-stock-4827.png"
    ))
    .unwrap()
    .to_luma8()
}

fn scanner(profile: &ScanProfile) -> Scanner {
    let mut config = profile.clone().config();
    config.budget_ms = None;
    Scanner::builder()
        .profile(ScanProfile::Custom(config))
        .build()
}

#[test]
fn the_console_mixed_code128_regression_in_every_profile() {
    let image = image::load_from_memory(include_bytes!(
        "../../../fixtures/symbology/mixed-code128-qr.png"
    ))
    .unwrap()
    .to_luma8();
    for profile in [ScanProfile::Fast, ScanProfile::Frame, ScanProfile::Full] {
        let report = scanner(&profile)
            .scan(ImageInput::luma8(
                image.as_raw(),
                image.width(),
                image.height(),
            ))
            .unwrap();
        let mut found: Vec<_> = report
            .detections
            .iter()
            .map(|d| (d.symbology, d.content.text.as_str()))
            .collect();
        found.sort_by_key(|(_, text)| *text);
        assert_eq!(
            found,
            [
                (Symbology::Code128, BARCODE),
                (Symbology::QrCode, "https://qrc-ai.com/76xMa")
            ],
            "{profile:?}: {:?}",
            report.trace
        );
    }
}

#[test]
fn the_console_mixed_code128_is_read_at_original_resolution() {
    let bytes = include_bytes!("../../../fixtures/symbology/mixed-code128-qr.png");
    let mut config = ScanProfile::Full.config();
    config.pyramid = false;
    config.budget_ms = None;
    config.score_depth = qrcode_ai_scanner::ScoreDepth::Off;
    let report = Scanner::builder()
        .profile(ScanProfile::Custom(config))
        .build()
        .scan(ImageInput::encoded(bytes))
        .unwrap();
    let mut found: Vec<_> = report
        .detections
        .iter()
        .map(|d| (d.symbology, d.content.text.as_str()))
        .collect();
    found.sort_by_key(|(_, text)| *text);
    assert_eq!(
        found,
        [
            (Symbology::Code128, BARCODE),
            (Symbology::QrCode, "https://qrc-ai.com/76xMa")
        ],
        "{:?}",
        report.trace
    );
}

fn image_with_qr(barcode_first: bool) -> image::GrayImage {
    let barcode = code128();
    let qr = qrcode::QrCode::with_error_correction_level(QR, qrcode::EcLevel::M)
        .unwrap()
        .render::<image::Luma<u8>>()
        .module_dimensions(5, 5)
        .build();
    let mut canvas = image::GrayImage::from_pixel(
        qr.width() + barcode.width() + 40,
        qr.height().max(barcode.height()) + 40,
        image::Luma([255]),
    );
    let (left, right) = if barcode_first {
        (&barcode, &qr)
    } else {
        (&qr, &barcode)
    };
    image::imageops::replace(&mut canvas, left, 20, 20);
    image::imageops::replace(&mut canvas, right, i64::from(left.width() + 20), 20);
    canvas
}

fn assert_reads(image: &image::GrayImage, profile: &ScanProfile, mixed: bool) {
    let width = image.width();
    let height = image.height();
    let report = scanner(profile)
        .scan(ImageInput::luma8(image.as_raw(), width, height))
        .unwrap();
    let mut found: Vec<_> = report
        .detections
        .iter()
        .map(|d| (d.symbology, d.content.text.as_str()))
        .collect();
    found.sort_by_key(|(_, text)| *text);
    let mut expected = vec![(Symbology::Code128, BARCODE)];
    if mixed {
        expected.push((Symbology::QrCode, QR));
    }
    expected.sort_by_key(|(_, text)| *text);
    assert_eq!(found, expected, "{profile:?}: {:?}", report.trace);
    let barcode = report
        .detections
        .iter()
        .find(|d| d.symbology == Symbology::Code128)
        .unwrap();
    assert_eq!(barcode.content.raw, BARCODE.as_bytes());
    assert_eq!(barcode.meta.version, None);
    assert_eq!(barcode.meta.modules, None);
    assert_eq!(barcode.meta.ec_level, None);
}

#[test]
fn standalone_code128_control_in_every_profile() {
    for profile in [ScanProfile::Fast, ScanProfile::Frame, ScanProfile::Full] {
        assert_reads(&code128(), &profile, false);
    }
}

#[test]
fn qr_and_code128_both_survive_in_every_profile_and_horizontal_order() {
    for profile in [ScanProfile::Fast, ScanProfile::Frame, ScanProfile::Full] {
        for barcode_first in [false, true] {
            assert_reads(&image_with_qr(barcode_first), &profile, true);
        }
    }
}
