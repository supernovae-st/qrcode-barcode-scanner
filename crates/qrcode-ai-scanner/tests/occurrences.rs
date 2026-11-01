//! Physical occurrences (spec/01 § Occurrences): every copy of a content is
//! measured where it lies, and identical copies stay one content.
//!
//! The truth is independent of the readers: QR symbols are encoded by the
//! `qrcode` crate and the other symbols are committed fixtures, pasted at
//! known offsets (a QR's outer corners are its paste offset plus the quiet
//! zone; a linear read must lie on its copy's own bars, measured from the
//! pixels). Exact acceptance pins run each profile's schedule without a
//! wall-clock deadline: machine load must not turn a cut into a failure.

#![cfg(feature = "engine-rxing")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::many_single_char_names
)]

use qrcode::bits::Bits;
use qrcode::{EcLevel, QrCode, Version};
use qrcode_ai_scanner::{
    Detection, EngineKind, GeometryKind, ImageInput, Point, ScanProfile, ScanReport, Scanner,
    ScoreDepth, Symbology,
};

const BARCODE: &str = "STOCK-4827";
const QR: &str = "https://qrc-ai.com/occurrences";
/// Pixels per QR module.
const MODULE: u32 = 5;
/// The quiet zone the `qrcode` renderer draws, in modules.
const QUIET: u32 = 4;
const PROFILES: [ScanProfile; 3] = [ScanProfile::Fast, ScanProfile::Frame, ScanProfile::Full];

fn scan(image: &image::GrayImage, profile: &ScanProfile) -> ScanReport {
    scan_with(image, profile, |_| {})
}

fn scan_with(
    image: &image::GrayImage,
    profile: &ScanProfile,
    adjust: impl FnOnce(&mut qrcode_ai_scanner::ScanConfig),
) -> ScanReport {
    let mut config = profile.clone().config();
    config.budget_ms = None;
    adjust(&mut config);
    Scanner::builder()
        .profile(ScanProfile::Custom(config))
        .build()
        .scan(ImageInput::luma8(
            image.as_raw(),
            image.width(),
            image.height(),
        ))
        .unwrap()
}

fn fixture(bytes: &[u8]) -> image::GrayImage {
    image::load_from_memory(bytes).unwrap().to_luma8()
}

/// The console's Code 128 control, `STOCK-4827`.
fn code128() -> image::GrayImage {
    fixture(include_bytes!(
        "../../../fixtures/symbology/code128-stock-4827.png"
    ))
}

fn qr(text: &str) -> QrCode {
    QrCode::with_error_correction_level(text, EcLevel::M).unwrap()
}

/// The symbol with its quiet zone, `MODULE` pixels per module.
fn render(code: &QrCode) -> image::GrayImage {
    code.render::<image::Luma<u8>>()
        .module_dimensions(MODULE, MODULE)
        .quiet_zone(true)
        .build()
}

fn p(x: f32, y: f32) -> Point {
    Point { x, y }
}

/// The outer corners, clockwise from the top-left, of `code` rendered by
/// [`render`] and pasted with its top-left pixel at (`x`, `y`).
fn outer(code: &QrCode, x: u32, y: u32) -> [Point; 4] {
    let side = (code.width() as u32 * MODULE) as f32;
    let (x0, y0) = ((x + QUIET * MODULE) as f32, (y + QUIET * MODULE) as f32);
    [
        p(x0, y0),
        p(x0 + side, y0),
        p(x0 + side, y0 + side),
        p(x0, y0 + side),
    ]
}

fn canvas(width: u32, height: u32) -> image::GrayImage {
    image::GrayImage::from_pixel(width, height, image::Luma([255]))
}

fn paste(canvas: &mut image::GrayImage, symbol: &image::GrayImage, x: u32, y: u32) {
    image::imageops::replace(canvas, symbol, i64::from(x), i64::from(y));
}

fn only<'a>(report: &'a ScanReport, symbology: Symbology, text: &str) -> &'a Detection {
    let found: Vec<&Detection> = report
        .detections
        .iter()
        .filter(|d| d.symbology == symbology && d.content.text == text)
        .collect();
    assert_eq!(
        found.len(),
        1,
        "one {symbology:?} {text:?}: {:?}",
        report.detections
    );
    found[0]
}

/// Whether `points` are `want` within `tolerance` pixels, point for point.
fn near(points: &[Point], want: &[Point], tolerance: f32) -> bool {
    points.len() == want.len()
        && points
            .iter()
            .zip(want)
            .all(|(a, b)| (a.x - b.x).abs() <= tolerance && (a.y - b.y).abs() <= tolerance)
}

/// The occurrences of `detection` as quads matched one to one with `want`,
/// in any order: each expected symbol measured once, nothing else.
fn assert_quads(detection: &Detection, want: &[[Point; 4]], tolerance: f32) {
    assert_eq!(
        detection.occurrences.len(),
        want.len(),
        "{:?}",
        detection.occurrences
    );
    let mut unmatched: Vec<&[Point; 4]> = want.iter().collect();
    for occurrence in &detection.occurrences {
        let geometry = &occurrence.geometry;
        assert_eq!(geometry.kind, GeometryKind::Quad, "{geometry:?}");
        assert!(
            matches!(geometry.reader, Some(EngineKind::Rxing | EngineKind::Rqrr)),
            "{geometry:?}"
        );
        let at = unmatched
            .iter()
            .position(|quad| near(&geometry.points, &quad[..], tolerance))
            .unwrap_or_else(|| panic!("{geometry:?} is none of {unmatched:?}"));
        unmatched.remove(at);
    }
}

/// Columns of the dark pixels on `row` of `image` inside `x0..x1`: where a
/// scan line on that row may lie, measured from the pixels alone.
fn dark_extent(image: &image::GrayImage, row: u32, x0: u32, x1: u32) -> Option<(u32, u32)> {
    let dark: Vec<u32> = (x0..x1.min(image.width()))
        .filter(|&x| image.get_pixel(x, row).0[0] < 128)
        .collect();
    Some((*dark.first()?, *dark.last()?))
}

/// Rows a scan line read on a downscaled plane may lie off its original
/// row once mapped back (the plane's scale, at most a few pixels here).
const RESCALE_ROWS: i64 = 3;

/// A segment occurrence lying on the bars of the copy pasted in `region`
/// (x, y, width, height): both points on one row, that row (within
/// [`RESCALE_ROWS`]) crossing the copy's bars, the points inside their
/// dark extent and spanning at least half of it.
fn on_bars(image: &image::GrayImage, points: &[Point], (x, y, w, h): (u32, u32, u32, u32)) -> bool {
    let [a, b] = points else {
        return false;
    };
    if (a.y - b.y).abs() > 1.0 {
        return false;
    }
    let row = a.y.floor() as i64;
    (row - RESCALE_ROWS..=row + RESCALE_ROWS)
        .filter(|r| (i64::from(y)..i64::from(y + h)).contains(r))
        .filter_map(|r| dark_extent(image, r as u32, x, x + w))
        .any(|(left, right)| {
            let span = (left as f32 - 1.0)..=(right as f32 + 2.0);
            span.contains(&a.x)
                && span.contains(&b.x)
                && (b.x - a.x).abs() >= 0.5 * (right - left) as f32
        })
}

/// Every segment of `detection` on the bars of a distinct region, every
/// region measured once.
fn assert_segments(
    image: &image::GrayImage,
    detection: &Detection,
    regions: &[(u32, u32, u32, u32)],
    context: &str,
) {
    assert_eq!(
        detection.occurrences.len(),
        regions.len(),
        "{context}: {:?}",
        detection.occurrences
    );
    let mut unmatched: Vec<&(u32, u32, u32, u32)> = regions.iter().collect();
    for occurrence in &detection.occurrences {
        let geometry = &occurrence.geometry;
        assert_eq!(geometry.kind, GeometryKind::Segment, "{geometry:?}");
        assert_eq!(geometry.reader, Some(EngineKind::Rxing), "{geometry:?}");
        let at = unmatched
            .iter()
            .position(|region| on_bars(image, &geometry.points, **region))
            .unwrap_or_else(|| panic!("{context}: {geometry:?} lies on none of {unmatched:?}"));
        unmatched.remove(at);
    }
}

fn assert_unsaturated(report: &ScanReport) {
    let saturation = report.trace.saturation.unwrap();
    assert!(!saturation.any(), "{saturation:?}");
}

#[test]
fn qr_twins_and_stock_4827_are_two_contents_and_three_occurrences_in_every_profile() {
    let code = qr(QR);
    let symbol = render(&code);
    let barcode = code128();
    let side = symbol.width();
    let mut image = canvas(
        2 * side + barcode.width() + 120,
        side.max(barcode.height()) + 80,
    );
    let (first, second) = ((40, 40), (side + 80, 40));
    let bar = (2 * side + 100, 60);
    paste(&mut image, &symbol, first.0, first.1);
    paste(&mut image, &symbol, second.0, second.1);
    paste(&mut image, &barcode, bar.0, bar.1);
    for profile in &PROFILES {
        let report = scan(&image, profile);
        assert_eq!(
            report.detections.len(),
            2,
            "{profile:?}: {:?}",
            report.detections
        );
        // the primary detection is the QR content, whichever copy came first
        let primary = &report.detections[0];
        assert_eq!(
            (primary.symbology, primary.content.text.as_str()),
            (Symbology::QrCode, QR)
        );
        assert_quads(
            primary,
            &[
                outer(&code, first.0, first.1),
                outer(&code, second.0, second.1),
            ],
            2.0,
        );
        let stock = only(&report, Symbology::Code128, BARCODE);
        assert_eq!(stock.content.raw, BARCODE.as_bytes());
        assert_segments(
            &image,
            stock,
            &[(bar.0, bar.1, barcode.width(), barcode.height())],
            &format!("{profile:?}"),
        );
        // the score judges the primary content (a frame scan computes none)
        assert_eq!(
            report.score.is_some(),
            *profile != ScanProfile::Frame,
            "{profile:?}"
        );
        assert_unsaturated(&report);
    }
}

#[test]
fn equal_code128_copies_side_by_side_and_stacked_are_one_content_two_occurrences() {
    let barcode = code128();
    let (w, h) = (barcode.width(), barcode.height());
    let mut side_by_side = canvas(2 * w + 60, h);
    paste(&mut side_by_side, &barcode, 0, 0);
    paste(&mut side_by_side, &barcode, w + 60, 0);
    let mut stacked = canvas(w, 2 * h + 40);
    paste(&mut stacked, &barcode, 0, 0);
    paste(&mut stacked, &barcode, 0, h + 40);
    for (image, regions) in [
        (&side_by_side, [(0, 0, w, h), (w + 60, 0, w, h)]),
        (&stacked, [(0, 0, w, h), (0, h + 40, w, h)]),
    ] {
        for profile in &PROFILES {
            let report = scan(image, profile);
            assert_eq!(
                report.detections.len(),
                1,
                "{profile:?}: {:?}",
                report.detections
            );
            let stock = only(&report, Symbology::Code128, BARCODE);
            assert_segments(image, stock, &regions, &format!("{profile:?} {regions:?}"));
            assert_unsaturated(&report);
        }
    }
}

#[test]
fn one_tall_code128_read_on_several_lines_is_one_occurrence() {
    let barcode = code128();
    let (w, h) = (barcode.width(), barcode.height());
    // bars three times as tall: the multiple reader reads it again above
    // and below its first scan line, on bars that never break
    let tall = image::imageops::resize(&barcode, w, 3 * h, image::imageops::FilterType::Nearest);
    for profile in &PROFILES {
        let report = scan(&tall, profile);
        let stock = only(&report, Symbology::Code128, BARCODE);
        assert_segments(&tall, stock, &[(0, 0, w, 3 * h)], &format!("{profile:?}"));
        assert_unsaturated(&report);
    }
}

#[test]
fn equal_text_from_different_bytes_stays_two_contents_each_where_it_lies() {
    // 'Ą' twice: byte A1 under ECI 4 (ISO/IEC 8859-2), bytes C4 84 under
    // ECI 26 (UTF-8) — one text, two contents, two places
    let symbol = |eci: u32, bytes: &[u8]| {
        let mut bits = Bits::new(Version::Normal(3));
        bits.push_eci_designator(eci).unwrap();
        bits.push_byte_data(bytes).unwrap();
        bits.push_terminator(EcLevel::M).unwrap();
        QrCode::with_bits(bits, EcLevel::M).unwrap()
    };
    let (latin2, utf8) = (symbol(4, &[0xA1]), symbol(26, &[0xC4, 0x84]));
    let (left, right) = (render(&latin2), render(&utf8));
    let mut image = canvas(left.width() + right.width() + 120, left.height() + 80);
    paste(&mut image, &left, 40, 40);
    paste(&mut image, &right, left.width() + 80, 40);
    for profile in &PROFILES {
        let report = scan(&image, profile);
        assert_eq!(
            report.detections.len(),
            2,
            "{profile:?}: {:?}",
            report.detections
        );
        for (bytes, code, x) in [
            (&[0xA1][..], &latin2, 40),
            (&[0xC4, 0x84][..], &utf8, left.width() + 80),
        ] {
            let detection = report
                .detections
                .iter()
                .find(|d| d.content.raw == bytes)
                .unwrap_or_else(|| panic!("{profile:?}: {bytes:02X?}: {:?}", report.detections));
            assert_eq!(detection.content.text, "Ą");
            assert_quads(detection, &[outer(code, x, 40)], 2.0);
        }
        assert_unsaturated(&report);
    }
}

#[test]
fn data_matrix_and_pdf417_twins_side_by_side_are_two_occurrences() {
    for (bytes, symbology) in [
        (
            &include_bytes!("../../../fixtures/symbology/datamatrix.png")[..],
            Symbology::DataMatrix,
        ),
        (
            &include_bytes!("../../../fixtures/symbology/pdf417.png")[..],
            Symbology::Pdf417,
        ),
    ] {
        let symbol = fixture(bytes);
        let single = scan(&symbol, &ScanProfile::Full);
        let [alone] = &single.detections[..] else {
            panic!("{symbology:?} alone: {:?}", single.detections);
        };
        let [footprint] = &alone.occurrences[..] else {
            panic!("{symbology:?} alone: {:?}", alone.occurrences);
        };
        assert_eq!(footprint.geometry.kind, GeometryKind::Quad, "{footprint:?}");
        let (w, h) = (symbol.width(), symbol.height());
        let mut twins = canvas(2 * w + 60, h);
        paste(&mut twins, &symbol, 0, 0);
        paste(&mut twins, &symbol, w + 60, 0);
        // the copy's quad is the single read's, moved by the paste offset
        let moved: Vec<Point> = footprint
            .geometry
            .points
            .iter()
            .map(|q| p(q.x + (w + 60) as f32, q.y))
            .collect();
        let alone: [Point; 4] = footprint.geometry.points.clone().try_into().unwrap();
        let moved: [Point; 4] = moved.try_into().unwrap();
        for profile in &PROFILES {
            let report = scan(&twins, profile);
            let [detection] = &report.detections[..] else {
                panic!("{symbology:?} {profile:?}: {:?}", report.detections);
            };
            assert_eq!(detection.symbology, symbology);
            assert_eq!(detection.content.raw, alone_raw(&single));
            assert_quads(detection, &[alone, moved], 2.0);
            assert_unsaturated(&report);
        }
    }
}

fn alone_raw(report: &ScanReport) -> &[u8] {
    &report.detections[0].content.raw
}

/// The projective map taking the unit square's corners (0,0), (1,0),
/// (1,1), (0,1) to `quad`, as a 3×3 matrix (Heckbert's closed form).
fn square_to_quad(quad: [Point; 4]) -> [[f64; 3]; 3] {
    let [(x0, y0), (x1, y1), (x2, y2), (x3, y3)] = quad.map(|q| (f64::from(q.x), f64::from(q.y)));
    let (dx1, dx2, dx3) = (x1 - x2, x3 - x2, x0 - x1 + x2 - x3);
    let (dy1, dy2, dy3) = (y1 - y2, y3 - y2, y0 - y1 + y2 - y3);
    let den = dx1 * dy2 - dx2 * dy1;
    let g = (dx3 * dy2 - dx2 * dy3) / den;
    let h = (dx1 * dy3 - dx3 * dy1) / den;
    [
        [x1 - x0 + g * x1, x3 - x0 + h * x3, x0],
        [y1 - y0 + g * y1, y3 - y0 + h * y3, y0],
        [g, h, 1.0],
    ]
}

fn apply(m: &[[f64; 3]; 3], x: f64, y: f64) -> (f64, f64) {
    let w = m[2][0] * x + m[2][1] * y + m[2][2];
    (
        (m[0][0] * x + m[0][1] * y + m[0][2]) / w,
        (m[1][0] * x + m[1][1] * y + m[1][2]) / w,
    )
}

fn inverse(m: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let [[a, b, c], [d, e, f], [g, h, i]] = *m;
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    [
        [
            (e * i - f * h) / det,
            (c * h - b * i) / det,
            (b * f - c * e) / det,
        ],
        [
            (f * g - d * i) / det,
            (a * i - c * g) / det,
            (c * d - a * f) / det,
        ],
        [
            (d * h - e * g) / det,
            (b * g - a * h) / det,
            (a * e - b * d) / det,
        ],
    ]
}

#[test]
fn a_qr_seen_in_perspective_is_measured_on_its_projected_corners() {
    let code = qr(QR);
    let symbol = render(&code);
    let quiet = f64::from(QUIET * MODULE);
    let side = f64::from(code.width() as u32 * MODULE);
    // the symbol's own square (inside the quiet zone) goes to a known
    // perspective quad; the quiet zone follows the same map
    let target = [
        p(160.0, 90.0),
        p(520.0, 140.0),
        p(560.0, 470.0),
        p(120.0, 520.0),
    ];
    let to_target = square_to_quad(target);
    let from_target = inverse(&to_target);
    let mut image = canvas(700, 620);
    for y in 0..image.height() {
        for x in 0..image.width() {
            let (u, v) = apply(&from_target, f64::from(x) + 0.5, f64::from(y) + 0.5);
            let (sx, sy) = (quiet + u * side, quiet + v * side);
            if sx >= 0.0
                && sy >= 0.0
                && sx < f64::from(symbol.width())
                && sy < f64::from(symbol.height())
            {
                let value = symbol.get_pixel(sx as u32, sy as u32).0[0];
                image.put_pixel(x, y, image::Luma([value]));
            }
        }
    }
    for profile in &PROFILES {
        let report = scan(&image, profile);
        let detection = only(&report, Symbology::QrCode, QR);
        assert_quads(detection, &[target], 3.0);
        assert_unsaturated(&report);
    }
}

/// A scan read at full resolution only: under test is a capacity, not how
/// many small symbols a thumbnail keeps readable.
fn full_resolution(image: &image::GrayImage) -> ScanReport {
    scan_with(image, &ScanProfile::Full, |config| config.pyramid = false)
}

/// Pixels between grid cells (beyond each symbol's own quiet zone).
const GRID_GAP: u32 = 60;

/// `symbols` on a grid of `columns`, `gap` pixels apart, each row shifted
/// by half a cell so no finder pattern lines up with its neighbours'.
fn grid(symbols: &[image::GrayImage], columns: u32, gap: u32) -> image::GrayImage {
    let side = symbols[0].width();
    let cell = side + gap;
    let rows = (symbols.len() as u32).div_ceil(columns);
    let mut image = canvas(columns * cell + cell / 2 + gap, rows * cell + gap);
    for (i, symbol) in (0u32..).zip(symbols) {
        let (column, row) = (i % columns, i / columns);
        paste(
            &mut image,
            symbol,
            gap + column * cell + (row % 2) * cell / 2,
            gap + row * cell,
        );
    }
    image
}

#[test]
fn copies_past_sixteen_are_left_out_and_the_report_says_so() {
    // eighteen copies of one small symbol: sixteen occurrences published
    let code = QrCode::with_version("Q", Version::Normal(1), EcLevel::L).unwrap();
    let symbols = vec![render(&code); 18];
    let image = grid(&symbols, 6, GRID_GAP);
    let report = full_resolution(&image);
    let detection = only(&report, Symbology::QrCode, "Q");
    assert_eq!(
        detection.occurrences.len(),
        16,
        "{:?}",
        detection.occurrences
    );
    let saturation = report.trace.saturation.unwrap();
    assert!(saturation.occurrences, "{saturation:?}");
    assert!(!saturation.detections, "{saturation:?}");
}

#[test]
fn contents_past_sixteen_are_left_out_and_the_report_says_so() {
    // eighteen distinct small symbols: sixteen detections published
    let symbols: Vec<image::GrayImage> = (0..18)
        .map(|i| {
            render(
                &QrCode::with_version(format!("C{i:02}"), Version::Normal(1), EcLevel::L).unwrap(),
            )
        })
        .collect();
    let image = grid(&symbols, 6, GRID_GAP);
    let report = full_resolution(&image);
    assert_eq!(report.detections.len(), 16, "{:?}", report.detections);
    let saturation = report.trace.saturation.unwrap();
    assert!(saturation.detections, "{saturation:?}");
    for detection in &report.detections {
        assert_eq!(detection.occurrences.len(), 1, "{detection:?}");
    }
}

/// `code` turned `degrees` clockwise about (`cx`, `cy`) on a white
/// `width` × `height` canvas, `module` pixels per module (each pixel takes
/// the module under its centre), with the outer corners clockwise from the
/// symbol's first.
fn turned(
    code: &QrCode,
    module: f64,
    degrees: f64,
    (cx, cy): (f64, f64),
    (width, height): (u32, u32),
) -> (image::GrayImage, [Point; 4]) {
    let n = code.width();
    let colors = code.to_colors();
    let half = n as f64 * module / 2.0;
    let (sin, cos) = degrees.to_radians().sin_cos();
    let image = image::GrayImage::from_fn(width, height, |i, j| {
        let (dx, dy) = (f64::from(i) + 0.5 - cx, f64::from(j) + 0.5 - cy);
        let (u, v) = (cos * dx + sin * dy + half, -sin * dx + cos * dy + half);
        let dark = u >= 0.0
            && v >= 0.0
            && u < 2.0 * half
            && v < 2.0 * half
            && colors[(v / module) as usize * n + (u / module) as usize] == qrcode::Color::Dark;
        image::Luma([if dark { 0 } else { 255 }])
    });
    let corner = |u: f64, v: f64| {
        let (u, v) = (u - half, v - half);
        p(
            (cx + cos * u - sin * v) as f32,
            (cy + sin * u + cos * v) as f32,
        )
    };
    let side = 2.0 * half;
    (
        image,
        [
            corner(0.0, 0.0),
            corner(side, 0.0),
            corner(side, side),
            corner(0.0, side),
        ],
    )
}

#[test]
fn a_small_copy_in_the_empty_corner_of_a_turned_copy_is_another_occurrence() {
    // the large copy turned 45° about (260, 260): its outer corners' box
    // reaches 164 px from the centre on every side; the small upright copy
    // is centred 140 px right and above it — inside that box, clear of the
    // large symbol and its quiet zone
    let code = qr(QR);
    let (mut image, large) = turned(&code, 8.0, 45.0, (260.0, 260.0), (520, 520));
    let small_module = 2;
    let small = code
        .render::<image::Luma<u8>>()
        .module_dimensions(small_module, small_module)
        .quiet_zone(true)
        .build();
    let (x, y) = (400 - small.width() / 2, 120 - small.height() / 2);
    paste(&mut image, &small, x, y);
    let side = (code.width() as u32 * small_module) as f32;
    let (x0, y0) = (
        (x + QUIET * small_module) as f32,
        (y + QUIET * small_module) as f32,
    );
    let upright = [
        p(x0, y0),
        p(x0 + side, y0),
        p(x0 + side, y0 + side),
        p(x0, y0 + side),
    ];
    for profile in &PROFILES {
        let report = scan(&image, profile);
        let detection = only(&report, Symbology::QrCode, QR);
        assert_quads(detection, &[large, upright], 3.0);
        assert_unsaturated(&report);
    }
}

#[test]
fn identically_damaged_twins_only_the_rescue_reads_are_two_occurrences() {
    // the occluded symbol both engines fail on, twice: one rescue per place
    // (the deep rungs off: one of them reads a single copy outright)
    let symbol = fixture(include_bytes!(
        "../../../fixtures/degraded/logo-occluded-rescue.png"
    ));
    let unscored = |c: &mut qrcode_ai_scanner::ScanConfig| {
        c.deep = false;
        c.score_depth = ScoreDepth::Off;
    };
    let single = scan_with(&symbol, &ScanProfile::Full, unscored);
    let [alone] = &single.detections[..] else {
        panic!("alone: {:?}", single.detections);
    };
    assert_eq!(alone.engines, vec![EngineKind::Rescue]);
    let [footprint] = &alone.occurrences[..] else {
        panic!("alone: {:?}", alone.occurrences);
    };
    let first: [Point; 4] = footprint.geometry.points.clone().try_into().unwrap();
    let gap = 40;
    let mut twins = canvas(2 * symbol.width() + gap, symbol.height());
    paste(&mut twins, &symbol, 0, 0);
    paste(&mut twins, &symbol, symbol.width() + gap, 0);
    let shift = (symbol.width() + gap) as f32;
    let second = first.map(|q| p(q.x + shift, q.y));
    let report = scan_with(&twins, &ScanProfile::Full, unscored);
    let [detection] = &report.detections[..] else {
        panic!("twins: {:?}", report.detections);
    };
    assert_eq!(detection.engines, vec![EngineKind::Rescue]);
    assert_eq!(detection.content.raw, alone.content.raw);
    assert_quads(detection, &[first, second], 2.0);
    assert_unsaturated(&report);
}

/// The one occurrence of `detection`, which no reader measured.
fn assert_unavailable(detection: &Detection, context: &str) {
    let [occurrence] = &detection.occurrences[..] else {
        panic!("{context}: {:?}", detection.occurrences);
    };
    let geometry = &occurrence.geometry;
    assert_eq!(geometry.kind, GeometryKind::Unavailable, "{context}");
    assert_eq!(geometry.reader, None, "{context}");
    assert!(geometry.points.is_empty(), "{context}: {geometry:?}");
}

#[test]
fn a_maxicode_read_without_points_leaves_the_code128_below_it_found_and_measured() {
    // fixtures/symbology/CONTENT-FIXTURES.md § Occurrence fixtures: the
    // MaxiCode (rows 30..=327) above the Code 128 (rows 358..=457), small
    // enough that no profile reads it on a downscaled plane
    let image = fixture(include_bytes!(
        "../../../fixtures/symbology/maxicode-over-code128.png"
    ));
    let barcode = (0, 340, image.width(), image.height() - 340);
    for profile in &PROFILES {
        let report = scan(&image, profile);
        assert_eq!(
            report.detections.len(),
            2,
            "{profile:?}: {:?}",
            report.detections
        );
        let maxi = only(&report, Symbology::MaxiCode, "MAXI-4827");
        assert_unavailable(maxi, &format!("{profile:?} MaxiCode"));
        let stock = only(&report, Symbology::Code128, BARCODE);
        assert_segments(&image, stock, &[barcode], &format!("{profile:?}"));
        assert_unsaturated(&report);
    }
}

#[test]
fn an_ean13_and_its_add_on_are_one_partial_scan_line_beside_a_code128() {
    // the bars measured from the pixels: the main symbol 53..=337, its
    // five-digit add-on 359..=499, the Code 128 585..=986
    let image = fixture(include_bytes!(
        "../../../fixtures/symbology/ean13-addon5-code128.png"
    ));
    let h = image.height();
    let (main, add_on, barcode) = ((40, 0, 308, h), (345, 0, 175, h), (560, 0, 460, h));
    for profile in &PROFILES {
        let report = scan(&image, profile);
        assert_eq!(
            report.detections.len(),
            2,
            "{profile:?}: {:?}",
            report.detections
        );
        let retail = only(&report, Symbology::Ean13, "5901234123457");
        let [occurrence] = &retail.occurrences[..] else {
            panic!("{profile:?}: {:?}", retail.occurrences);
        };
        let geometry = &occurrence.geometry;
        // four collinear points, no segment: the main symbol's two, then
        // the add-on's two, all on one row crossing both
        assert_eq!(geometry.kind, GeometryKind::Partial, "{profile:?}");
        assert_eq!(geometry.reader, Some(EngineKind::Rxing), "{profile:?}");
        let points = &geometry.points;
        assert_eq!(points.len(), 4, "{profile:?}: {geometry:?}");
        assert!(
            points.windows(2).all(|w| w[0].x < w[1].x),
            "{profile:?}: {geometry:?}"
        );
        assert!(
            on_bars(&image, &points[..2], main) && on_bars(&image, &points[2..], add_on),
            "{profile:?}: {geometry:?}"
        );
        assert!(
            (points[0].y - points[3].y).abs() <= 1.0,
            "{profile:?}: {geometry:?}"
        );
        let stock = only(&report, Symbology::Code128, BARCODE);
        assert_segments(&image, stock, &[barcode], &format!("{profile:?}"));
        assert_unsaturated(&report);
    }
}

/// The one detection of `symbol` scanned alone in `profile`: the relative
/// truth a composed scene is held to.
fn alone(symbol: &image::GrayImage, profile: &ScanProfile, context: &str) -> Detection {
    let report = scan(symbol, profile);
    let [detection] = &report.detections[..] else {
        panic!("{context} alone {profile:?}: {:?}", report.detections);
    };
    detection.clone()
}

/// The detection of `image` carrying `like`'s symbology and raw bytes.
fn same_content<'a>(report: &'a ScanReport, like: &Detection, context: &str) -> &'a Detection {
    let found: Vec<&Detection> = report
        .detections
        .iter()
        .filter(|d| d.symbology == like.symbology && d.content.raw == like.content.raw)
        .collect();
    let [detection] = &found[..] else {
        panic!("{context}: {:?}", report.detections);
    };
    detection
}

/// `points` moved by (`dx`, `dy`).
fn moved(points: &[Point], dx: u32, dy: u32) -> Vec<Point> {
    points
        .iter()
        .map(|q| p(q.x + dx as f32, q.y + dy as f32))
        .collect()
}

/// A `partial` scan-line measurement on the copy pasted in `region`: every
/// point on one row crossing the copy's bars and inside their dark extent.
fn assert_partial_on_row(
    image: &image::GrayImage,
    detection: &Detection,
    (x, y, w, h): (u32, u32, u32, u32),
    context: &str,
) {
    let [occurrence] = &detection.occurrences[..] else {
        panic!("{context}: {:?}", detection.occurrences);
    };
    let geometry = &occurrence.geometry;
    assert_eq!(
        geometry.kind,
        GeometryKind::Partial,
        "{context}: {geometry:?}"
    );
    assert_eq!(geometry.reader, Some(EngineKind::Rxing), "{context}");
    let first = geometry.points[0];
    assert!(
        geometry.points.iter().all(|q| (q.y - first.y).abs() <= 1.0),
        "{context}: {geometry:?}"
    );
    let row = first.y.floor() as i64;
    let fits = (row - RESCALE_ROWS..=row + RESCALE_ROWS)
        .filter(|r| (i64::from(y)..i64::from(y + h)).contains(r))
        .filter_map(|r| dark_extent(image, r as u32, x, x + w))
        .any(|(left, right)| {
            let span = (left as f32 - 1.0)..=(right as f32 + 2.0);
            geometry.points.iter().all(|q| span.contains(&q.x))
        });
    assert!(
        fits,
        "{context}: {geometry:?} off the bars of {:?}",
        (x, y, w, h)
    );
}

/// `first` then `second` on one row, `ROW_GAP` pixels apart.
fn on_one_row(first: &image::GrayImage, second: &image::GrayImage) -> image::GrayImage {
    let mut image = canvas(
        first.width() + ROW_GAP + second.width(),
        first.height().max(second.height()),
    );
    paste(&mut image, first, 0, 0);
    paste(&mut image, second, first.width() + ROW_GAP, 0);
    image
}

const ROW_GAP: u32 = 60;

#[test]
fn a_databar_and_an_ean13_on_one_row_are_each_measured_on_their_own_bars() {
    // two linear symbols of different formats crossed by the same rows, in
    // both orders, read at input resolution: neither read is taken for the
    // other's copy (with the pyramid on, the 764 px scene meets the known
    // loss pinned by the next test)
    let databar = fixture(include_bytes!(
        "../../../fixtures/symbology/databar-gtin.png"
    ));
    let ean = fixture(include_bytes!("../../../fixtures/symbology/ean13.png"));
    for (left, right, databar_first) in [(&databar, &ean, true), (&ean, &databar, false)] {
        let image = on_one_row(left, right);
        let (databar_x, ean_x) = if databar_first {
            (0, databar.width() + ROW_GAP)
        } else {
            (ean.width() + ROW_GAP, 0)
        };
        for profile in &PROFILES {
            let context = format!("databar first {databar_first} {profile:?}");
            let report = scan_with(&image, profile, |c| c.pyramid = false);
            assert_eq!(
                report.detections.len(),
                2,
                "{context}: {:?}",
                report.detections
            );
            let gtin = same_content(&report, &alone(&databar, profile, "DataBar"), &context);
            assert_partial_on_row(
                &image,
                gtin,
                (databar_x, 0, databar.width(), databar.height()),
                &context,
            );
            let retail = same_content(&report, &alone(&ean, profile, "EAN-13"), &context);
            assert_segments(
                &image,
                retail,
                &[(ean_x, 0, ean.width(), ean.height())],
                &context,
            );
            assert_unsaturated(&report);
        }
    }
}

#[test]
fn an_ean13_read_on_the_pyramid_leaves_the_databar_beside_it_unreported() {
    // the known loss of spec/07 property 2, kept visible: the pyramid reads
    // the EAN-13 alone, which ends the walk, and the DataBar its downscale
    // erased gets no input-resolution pass, in every profile. Lifting this
    // loss must change the spec and this pin together.
    let databar = fixture(include_bytes!(
        "../../../fixtures/symbology/databar-gtin.png"
    ));
    let ean = fixture(include_bytes!("../../../fixtures/symbology/ean13.png"));
    let image = on_one_row(&ean, &databar);
    for profile in &PROFILES {
        let report = scan(&image, profile);
        let [retail] = &report.detections[..] else {
            panic!("{profile:?}: {:?}", report.detections);
        };
        assert_eq!(retail.symbology, Symbology::Ean13, "{profile:?}");
        let stages: Vec<&str> = report
            .trace
            .stages
            .iter()
            .map(|s| s.stage.as_str())
            .collect();
        assert_eq!(stages, ["pyramid"], "{profile:?}");
        assert_unsaturated(&report);
    }
}

#[test]
fn a_code39_beside_a_data_matrix_keeps_both_where_they_lie() {
    let code39 = fixture(include_bytes!("../../../fixtures/symbology/code39.png"));
    let matrix = fixture(include_bytes!("../../../fixtures/symbology/datamatrix.png"));
    let (gap, top) = (60, 30);
    let mut image = canvas(
        code39.width() + gap + matrix.width(),
        code39.height().max(matrix.height() + top),
    );
    paste(&mut image, &code39, 0, 0);
    let matrix_x = code39.width() + gap;
    paste(&mut image, &matrix, matrix_x, top);
    for profile in &PROFILES {
        let context = format!("{profile:?}");
        let report = scan(&image, profile);
        assert_eq!(
            report.detections.len(),
            2,
            "{context}: {:?}",
            report.detections
        );
        let linear = same_content(&report, &alone(&code39, profile, "Code 39"), &context);
        assert_segments(
            &image,
            linear,
            &[(0, 0, code39.width(), code39.height())],
            &context,
        );
        let single = alone(&matrix, profile, "Data Matrix");
        let quad: [Point; 4] = moved(&single.occurrences[0].geometry.points, matrix_x, top)
            .try_into()
            .unwrap();
        assert_quads(same_content(&report, &single, &context), &[quad], 2.0);
        assert_unsaturated(&report);
    }
}

#[test]
fn a_code39_upside_down_is_measured_on_its_bars() {
    let code39 = fixture(include_bytes!("../../../fixtures/symbology/code39.png"));
    let turned = image::imageops::rotate180(&code39);
    let upright = alone(&code39, &ScanProfile::Full, "Code 39");
    for profile in &PROFILES {
        let context = format!("{profile:?}");
        let report = scan(&turned, profile);
        let read = same_content(&report, &upright, &context);
        assert_segments(
            &turned,
            read,
            &[(0, 0, turned.width(), turned.height())],
            &context,
        );
        assert_unsaturated(&report);
    }
}

#[test]
fn a_turned_pdf417_off_centre_is_measured_where_it_lies() {
    let pdf417 = image::imageops::rotate90(&fixture(include_bytes!(
        "../../../fixtures/symbology/pdf417.png"
    )));
    let (x, y) = (37, 113);
    let mut image = canvas(pdf417.width() + 211, pdf417.height() + 263);
    paste(&mut image, &pdf417, x, y);
    for profile in &PROFILES {
        let context = format!("{profile:?}");
        let single = alone(&pdf417, profile, "PDF417");
        assert_eq!(
            single.occurrences[0].geometry.kind,
            GeometryKind::Quad,
            "{context}"
        );
        let quad: [Point; 4] = moved(&single.occurrences[0].geometry.points, x, y)
            .try_into()
            .unwrap();
        let report = scan(&image, profile);
        assert_quads(same_content(&report, &single, &context), &[quad], 2.0);
        assert_unsaturated(&report);
    }
}

#[test]
fn codabar_and_telepen_publish_their_place_as_unavailable() {
    // rxing measures both from the row's first light pixel and past the
    // stop character: no point on their bars to publish
    for (bytes, symbology, text) in [
        (
            &include_bytes!("../../../fixtures/symbology/codabar.png")[..],
            Symbology::Codabar,
            "40156",
        ),
        (
            &include_bytes!("../../../fixtures/symbology/telepen.png")[..],
            Symbology::Telepen,
            "TELEPEN-4827",
        ),
    ] {
        let image = fixture(bytes);
        for profile in &PROFILES {
            let report = scan(&image, profile);
            let [detection] = &report.detections[..] else {
                panic!("{symbology:?} {profile:?}: {:?}", report.detections);
            };
            assert_eq!(
                (detection.symbology, detection.content.text.as_str()),
                (symbology, text)
            );
            assert_unavailable(detection, &format!("{symbology:?} {profile:?}"));
            assert_unsaturated(&report);
        }
    }
}
