//! Independent rqrr regressions: qrcode 0.14.1 supplies encoding, Reed-Solomon
//! parity and placement. rqrr is only the decoder under test. Component
//! success and the scanner's stricter publication contract are separate pins.

#![cfg(feature = "engine-rqrr")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use qrcode::{Color, EcLevel, Version, bits::Bits, canvas::Canvas};
use qrcode_ai_scanner::{EngineKind, ImageInput, ScanProfile, Scanner, Symbology};

struct Symbol {
    colors: Vec<Color>,
    width: usize,
}

impl Symbol {
    fn from_codewords(version: Version, ec: EcLevel, data: &[u8], parity: &[u8]) -> Self {
        let mut canvas = Canvas::new(version, ec);
        canvas.draw_all_functional_patterns();
        canvas.draw_data(data, parity);
        Self {
            colors: canvas.apply_best_mask().into_colors(),
            width: usize::try_from(version.width()).unwrap(),
        }
    }

    fn grid(&self) -> rqrr::Grid<rqrr::SimpleGrid> {
        rqrr::Grid::new(rqrr::SimpleGrid::from_func(self.width, |x, y| {
            self.colors[y * self.width + x] == Color::Dark
        }))
    }

    fn image(&self) -> image::GrayImage {
        let side = u32::try_from(self.width + 8).unwrap() * 6;
        image::GrayImage::from_fn(side, side, |x, y| {
            let (x, y) = (
                usize::try_from(x / 6).unwrap(),
                usize::try_from(y / 6).unwrap(),
            );
            let dark = (4..self.width + 4).contains(&x)
                && (4..self.width + 4).contains(&y)
                && self.colors[(y - 4) * self.width + x - 4] == Color::Dark;
            image::Luma([if dark { 0 } else { 255 }])
        })
    }
}

fn damaged_mixed_blocks() -> (Symbol, Vec<u8>) {
    // ISO version 7-Q: two short blocks of 14 data codewords and four
    // long blocks of 15, each with 18 parity codewords. Fill the capacity
    // with distinguishable ASCII bytes so the long blocks' final bytes
    // cannot accidentally agree under an incorrect deinterleave.
    let version = Version::Normal(7);
    let payload: Vec<_> = (32u8..118).collect();
    let mut bits = Bits::new(version);
    bits.push_byte_data(&payload).unwrap();
    bits.push_terminator(EcLevel::Q).unwrap();
    assert_eq!(bits.len(), 88 * 8);
    let (mut data, parity) =
        qrcode::ec::construct_codewords(&bits.into_bytes(), version, EcLevel::Q).unwrap();
    assert_eq!((data.len(), parity.len()), (88, 108));
    // First long block is block 2 among 6. Damage exactly nine of its
    // common-column codewords after encoding: its errors-only RS limit.
    for column in 0..9 {
        data[column * 6 + 2] ^= 0xFF;
    }
    (
        Symbol::from_codewords(version, EcLevel::Q, &data, &parity),
        payload,
    )
}

fn truncated_count() -> Symbol {
    // Two valid byte segments use 120 of version 1-M's 128 data bits.
    // The final byte is mode 0100 then only four zero count bits, while
    // byte mode requires eight. Its RS parity is valid; its grammar is not.
    let version = Version::Normal(1);
    let mut bits = Bits::new(version);
    bits.push_byte_data(b"AAAAAA").unwrap();
    bits.push_byte_data(b"BBBBBB").unwrap();
    assert_eq!(bits.len(), 120);
    let mut raw = bits.into_bytes();
    raw.push(0x40);
    assert_eq!(raw.len(), 16);
    let (data, parity) = qrcode::ec::construct_codewords(&raw, version, EcLevel::M).unwrap();
    Symbol::from_codewords(version, EcLevel::M, &data, &parity)
}

#[test]
fn rqrr_recovers_a_long_block_at_its_error_correction_limit() {
    let (symbol, expected) = damaged_mixed_blocks();
    let mut decoded = Vec::new();
    let meta = symbol
        .grid()
        .decode_to(&mut decoded)
        .expect("nine errors are correctable");
    assert_eq!((meta.version.0, meta.ecc_level), (7, 3));
    assert_eq!(decoded, expected);
}

#[test]
fn rqrr_rejects_a_truncated_count_after_valid_segments() {
    let mut decoded = Vec::new();
    let result = truncated_count().grid().decode_to(&mut decoded);
    assert_eq!(result.map(|_| ()), Err(rqrr::DeQRError::DataUnderflow));
    // decode_to documents partial writes on failure. The caller must heed
    // the result rather than publishing this legitimate-looking prefix.
    assert_eq!(decoded, b"AAAAAABBBBBB");
}

fn scan(symbol: &Symbol) -> qrcode_ai_scanner::ScanReport {
    let image = symbol.image();
    let mut config = ScanProfile::Frame.config();
    config.budget_ms = None;
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

#[test]
fn scanner_preserves_exact_content_and_rqrr_consensus_on_the_damaged_symbol() {
    let (symbol, expected) = damaged_mixed_blocks();
    let report = scan(&symbol);
    assert_eq!(report.detections.len(), 1, "{:?}", report.trace);
    let detection = &report.detections[0];
    assert_eq!(detection.symbology, Symbology::QrCode);
    assert_eq!(detection.content.raw, expected);
    assert_eq!(detection.content.text.as_bytes(), expected);
    assert_eq!(detection.content.fnc1, None);
    assert_eq!(detection.content.eci, None);
    assert!(detection.engines.contains(&EngineKind::Rqrr));
    assert_eq!(detection.meta.version, Some(7));
    assert_eq!(detection.meta.ec_level, Some(qrcode_ai_scanner::EcLevel::Q));
    assert_eq!(report.trace.engine_panics, 0);
    assert_eq!(report.trace.budget_exhausted, Some(false));
}

#[test]
fn scanner_does_not_publish_a_prefix_from_a_truncated_count() {
    let report = scan(&truncated_count());
    assert_eq!(report.detections.len(), 0, "{:?}", report.trace);
    assert_eq!(report.trace.engine_panics, 0);
    assert_eq!(report.trace.budget_exhausted, Some(false));
}
