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

fn small_data_matrix() -> image::GrayImage {
    // Independently encoded with bwip-js 4.11.4 (BWIPP), not rxing:
    // raw({bcid: "datamatrix", text: "DM-4827", scale: 1}).
    // Store modules rather than a resampled image: exact 3 px/module,
    // with a four-module quiet zone on every side.
    const ROWS: [&str; 12] = [
        "101010101010",
        "101110110011",
        "100110010000",
        "111001101101",
        "100010101010",
        "110011010111",
        "110101001000",
        "111010101101",
        "100110011010",
        "100000110101",
        "110110111010",
        "111111111111",
    ];
    image::GrayImage::from_fn(60, 60, |x, y| {
        let (x, y) = (x / 3, y / 3);
        let dark = (4..16).contains(&x)
            && (4..16).contains(&y)
            && ROWS[(y - 4) as usize].as_bytes()[(x - 4) as usize] == b'1';
        image::Luma([if dark { 0 } else { 255 }])
    })
}

fn image_with_small_data_matrix(matrix_first: bool) -> image::GrayImage {
    let matrix = small_data_matrix();
    let qr = qrcode::QrCode::with_error_correction_level(QR, qrcode::EcLevel::M)
        .unwrap()
        .render::<image::Luma<u8>>()
        .module_dimensions(12, 12)
        .build();
    let mut mixed = image::GrayImage::from_pixel(1_600, 600, image::Luma([255]));
    let (qr_x, matrix_x) = if matrix_first {
        (1_100, 200)
    } else {
        (40, 1_300)
    };
    image::imageops::replace(&mut mixed, &qr, qr_x, 40);
    image::imageops::replace(&mut mixed, &matrix, matrix_x, 240);
    mixed
}

fn exact_contents(report: &qrcode_ai_scanner::ScanReport) -> Vec<(Symbology, &str, &[u8])> {
    let mut contents: Vec<_> = report
        .detections
        .iter()
        .map(|d| {
            (
                d.symbology,
                d.content.text.as_str(),
                d.content.raw.as_slice(),
            )
        })
        .collect();
    contents.sort_by_key(|(_, text, _)| *text);
    contents
}

#[test]
fn a_small_data_matrix_survives_beside_a_pyramid_qr_in_every_profile() {
    let expected = [
        (Symbology::DataMatrix, "DM-4827", "DM-4827".as_bytes()),
        (Symbology::QrCode, QR, QR.as_bytes()),
    ];
    for matrix_first in [false, true] {
        let mixed = image_with_small_data_matrix(matrix_first);
        let input = || ImageInput::luma8(mixed.as_raw(), mixed.width(), mixed.height());

        // This is readable at native resolution. At thumbnail resolution
        // exactly the QR survives, and direct=false must still skip completion.
        for pyramid in [false, true] {
            let mut config = ScanProfile::Frame.config();
            config.pyramid = pyramid;
            config.direct = !pyramid;
            config.budget_ms = None;
            let control = Scanner::builder()
                .profile(ScanProfile::Custom(config))
                .build()
                .scan(input())
                .unwrap();
            let expected = if pyramid {
                &expected[1..]
            } else {
                &expected[..]
            };
            assert_eq!(exact_contents(&control), expected, "{:?}", control.trace);
            assert_eq!(control.trace.budget_exhausted, Some(false));
            assert_eq!(control.trace.stages.len(), 1);
            assert_eq!(
                control.trace.stages[0].stage,
                if pyramid { "pyramid" } else { "direct" }
            );
            assert_eq!(control.trace.stages[0].transforms_tried, 1);
        }

        for profile in [ScanProfile::Fast, ScanProfile::Frame, ScanProfile::Full] {
            let report = scanner(&profile).scan(input()).unwrap();
            assert_eq!(
                exact_contents(&report),
                expected,
                "{profile:?}: {:?}",
                report.trace
            );
            let stages: Vec<_> = report
                .trace
                .stages
                .iter()
                .map(|stage| (stage.stage.as_str(), stage.transforms_tried))
                .collect();
            assert_eq!(stages, [("pyramid", 1), ("direct", 1)]);
            assert_eq!(report.trace.budget_exhausted, Some(false));
            assert_eq!(report.trace.engine_panics, 0);
            let matrix = report
                .detections
                .iter()
                .find(|d| d.symbology == Symbology::DataMatrix)
                .unwrap();
            assert_eq!(matrix.meta.version, None);
            assert_eq!(matrix.meta.modules, None);
            assert_eq!(matrix.meta.ec_level, None);
        }
    }
}

#[test]
fn standalone_small_data_matrix_has_no_qr_structure_in_every_profile() {
    let matrix = small_data_matrix();
    for profile in [ScanProfile::Fast, ScanProfile::Frame, ScanProfile::Full] {
        let report = scanner(&profile)
            .scan(ImageInput::luma8(
                matrix.as_raw(),
                matrix.width(),
                matrix.height(),
            ))
            .unwrap();
        assert_eq!(
            exact_contents(&report),
            [(Symbology::DataMatrix, "DM-4827", "DM-4827".as_bytes())]
        );
        // Stress survival is supported for non-QR content too. QR-specific
        // structural, UEC and ISO claims require measured QR geometry.
        if let Some(score) = &report.score {
            assert!(score.structural.is_none());
            assert!(score.uec.is_none());
            assert!(score.iso15415.is_none());
        }
        if profile == ScanProfile::Frame {
            assert!(report.score.is_none());
        }
        assert!(report.detections.iter().all(|d| d.meta.version.is_none()
            && d.meta.modules.is_none()
            && d.meta.ec_level.is_none()));
    }
}
