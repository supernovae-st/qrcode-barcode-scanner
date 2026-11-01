//! Scanner copy of rxing 0.9.3's multiple-barcode reader
//! (`multi/generic_multiple_barcode_reader.rs`) and of its format dispatch
//! (`multi_use_multi_format_reader.rs`), Apache-2.0, notice below: the same
//! recursion around each symbol found, with these local changes (see
//! `THIRD-PARTY.md`):
//!
//! 1. **One dispatch order.** [`FormatReader`] tries the readers of a format
//!    set in the order rxing uses for an unrestricted pass — the QR family,
//!    Data Matrix, Aztec, PDF417, `MaxiCode`; linear readers first without
//!    `TryHarder`, last with it — never in a hash set's iteration order,
//!    which differs from one set to the next and made the first read (and so
//!    the crops) of a restricted pass vary from run to run.
//! 2. **Points in the image's frame.** Each read's points are normalized in
//!    its crop before anything uses them ([`normalized`]): a rotated PDF417
//!    reports them in its detector's rotated bit matrix; linear readers place
//!    them on pixel indices and map reversed or rotated rows back with a
//!    pixel-index formula, move only two points of an upside-down read, and
//!    Code 39 halves its stop character's end. Points no reader measured in
//!    this image are dropped, never guessed.
//! 3. **A read without points does not end its node.** The node is read
//!    again past that read's format ([`NodeReader::pass_over`]): upstream
//!    stopped there, and a `MaxiCode` (rxing reports none of its points) hid
//!    every other symbol of its region.
//! 4. **One node, one fresh reader.** The reader is reset before each read:
//!    a stateful reader (`DataBar` Expanded keeps the rows it saw) never
//!    assembles a symbol out of another crop's rows.
//! 5. **Duplicates by kind of symbol, not by number of points.** Two linear
//!    reads are one when they carry the same content and the attempt's pixels
//!    prove them the same physical symbol ([`occurrence::compare_linear`]),
//!    within a bound on those comparisons; a pair the pixels leave undecided
//!    stays two reads, which publication groups; a linear read lying inside a
//!    matrix symbol's quad is that symbol's texture; two matrix reads are one
//!    when they carry the same content and their boxes meet. Upstream decided
//!    by point count: a linear read with four points (an EAN add-on, a
//!    `DataBar`) was taken for a quad, and equal linear copies collapsed to
//!    one wherever they lay.

/*
 * Copyright 2009 ZXing authors
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use rxing::BarcodeFormat as F;
use rxing::RXingResultMetadataType as MetaKey;
use rxing::RXingResultMetadataValue as MetaValue;
use rxing::aztec::AztecReader;
use rxing::common::Result;
use rxing::datamatrix::DataMatrixReader;
use rxing::maxicode::MaxiCodeReader;
use rxing::oned::MultiFormatOneDReader;
use rxing::pdf417::PDF417Reader;
use rxing::qrcode::QRCodeReader;
use rxing::qrcode::cpp_port::QrReader;
use rxing::{
    BarcodeFormat, Binarizer, BinaryBitmap, DecodeHints, Exceptions, RXingResult, Reader, point,
};

use super::occurrence;
use crate::input::LumaImage;
use crate::report::Point;

/// The recursion's limits, upstream's.
const MIN_DIMENSION_TO_RECUR: f32 = 100.0;
const MAX_DEPTH: u32 = 4;

/// Formats one node may pass over (change 3): more than a reader has.
const MAX_PASSED: usize = 8;

/// Pixel comparisons ([`occurrence::same_linear`]) one call's duplicate
/// filter makes; past them two equal linear reads are both kept, and the
/// occurrence grouping compares them under its own bound.
const MAX_FILTER_BRIDGES: u32 = 256;

/// The formats rxing reads along scan lines.
const LINEAR: [BarcodeFormat; 13] = [
    F::UPC_A,
    F::UPC_E,
    F::EAN_13,
    F::EAN_8,
    F::CODABAR,
    F::CODE_39,
    F::CODE_93,
    F::CODE_128,
    F::ITF,
    F::RSS_14,
    F::RSS_EXPANDED,
    F::TELEPEN,
    F::UPC_EAN_EXTENSION,
];

fn linear(format: BarcodeFormat) -> bool {
    LINEAR.contains(&format)
}

/// A reader the recursion can ask to read a node again past one format.
pub(super) trait NodeReader: Reader {
    /// Leave `format` out of the next reads until [`NodeReader::readmit`];
    /// `false`: this reader cannot, and the node ends (upstream's rule).
    fn pass_over(&mut self, _format: BarcodeFormat) -> bool {
        false
    }

    /// Read `format` again.
    fn readmit(&mut self, _format: BarcodeFormat) {}
}

impl NodeReader for rxing::MultiFormatReader {}

impl NodeReader for super::databar_expanded::RSSExpandedReader {}

/// rxing 0.9.3's `MultiUseMultiFormatReader` dispatch (`decode_formats`,
/// `decode_internal`) in one fixed order (change 1): linear readers first
/// when `TryHarder` is off; the zxing-cpp QR port then the `ZXing`-lineage
/// reader; Data Matrix; Aztec; PDF417; `MaxiCode`; linear readers last when
/// `TryHarder` is on; the whole again on the inverted image under
/// `AlsoInverted`. A format set restricts the readers exactly as upstream's
/// does: a QR-family set runs the QR readers, a set with any linear format
/// runs the linear readers (restricted to the set by their own hints), and
/// rMQR has no reader here (`decode_rmqr`).
#[derive(Default)]
pub(super) struct FormatReader {
    passed_over: Vec<BarcodeFormat>,
    qr_port: QrReader,
    qr_lineage: QRCodeReader,
    data_matrix: DataMatrixReader,
    aztec: AztecReader,
    pdf417: PDF417Reader,
    maxicode: MaxiCodeReader,
}

impl FormatReader {
    fn decode_formats<B: Binarizer>(
        &mut self,
        image: &mut BinaryBitmap<B>,
        hints: &DecodeHints,
    ) -> Result<RXingResult> {
        let allowed = |format: BarcodeFormat| {
            hints
                .PossibleFormats
                .as_ref()
                .is_none_or(|set| set.is_empty() || set.contains(&format))
        };
        let open = |format: BarcodeFormat| allowed(format) && !self.passed_over.contains(&format);
        let (qr, micro) = (open(F::QR_CODE), open(F::MICRO_QR_CODE));
        let (data_matrix, aztec, pdf417, maxicode) = (
            open(F::DATA_MATRIX),
            open(F::AZTEC),
            open(F::PDF_417),
            open(F::MAXICODE),
        );
        let one_d = LINEAR.iter().any(|format| allowed(*format));
        let try_harder = matches!(hints.TryHarder, Some(true));
        if one_d
            && !try_harder
            && let Ok(result) = MultiFormatOneDReader::new(hints).decode_with_hints(image, hints)
        {
            return Ok(result);
        }
        if qr || micro {
            if let Ok(result) = self.qr_port.decode_with_hints(image, hints) {
                return Ok(result);
            }
            if qr && let Ok(result) = self.qr_lineage.decode_with_hints(image, hints) {
                return Ok(result);
            }
        }
        if data_matrix && let Ok(result) = self.data_matrix.decode_with_hints(image, hints) {
            return Ok(result);
        }
        if aztec && let Ok(result) = self.aztec.decode_with_hints(image, hints) {
            return Ok(result);
        }
        if pdf417 && let Ok(result) = self.pdf417.decode_with_hints(image, hints) {
            return Ok(result);
        }
        if maxicode && let Ok(result) = self.maxicode.decode_with_hints(image, hints) {
            return Ok(result);
        }
        if one_d && try_harder {
            return MultiFormatOneDReader::new(hints).decode_with_hints(image, hints);
        }
        Err(Exceptions::NOT_FOUND)
    }
}

impl Reader for FormatReader {
    fn decode<B: Binarizer>(&mut self, image: &mut BinaryBitmap<B>) -> Result<RXingResult> {
        self.decode_with_hints(image, &DecodeHints::default())
    }

    fn decode_with_hints<B: Binarizer>(
        &mut self,
        image: &mut BinaryBitmap<B>,
        hints: &DecodeHints,
    ) -> Result<RXingResult> {
        if let Ok(result) = self.decode_formats(image, hints) {
            return Ok(result);
        }
        if matches!(hints.AlsoInverted, Some(true)) {
            // restored afterwards: the caller's bitmap is unchanged
            image.invert();
            let inverted = self.decode_formats(image, hints);
            image.invert();
            if let Ok(mut result) = inverted {
                result.putMetadata(MetaKey::IS_INVERTED, MetaValue::IsInverted(true));
                return Ok(result);
            }
        }
        Err(Exceptions::NOT_FOUND)
    }

    fn reset(&mut self) {
        self.qr_port.reset();
        self.qr_lineage.reset();
        self.data_matrix.reset();
        self.aztec.reset();
        self.pdf417.reset();
        self.maxicode.reset();
    }
}

impl NodeReader for FormatReader {
    fn pass_over(&mut self, format: BarcodeFormat) -> bool {
        // a linear read always has points: only the matrix readers are
        // passed over
        if linear(format) || self.passed_over.contains(&format) {
            return false;
        }
        self.passed_over.push(format);
        true
    }

    fn readmit(&mut self, format: BarcodeFormat) {
        self.passed_over.retain(|own| *own != format);
    }
}

/// Decode every symbol `reader` finds in `image` (the attempt's binarized
/// `luma`): the read, then the regions left of, above, right of and below
/// it, recursively; duplicates filtered (module doc).
pub(super) fn decode_multiple<R: NodeReader, B: Binarizer>(
    reader: &mut R,
    image: &mut BinaryBitmap<B>,
    hints: &DecodeHints,
    luma: &LumaImage,
) -> Result<Vec<RXingResult>> {
    let mut results = Vec::new();
    recurse(reader, image, hints, &mut results, (0, 0), 0);
    let mut bridges = MAX_FILTER_BRIDGES;
    let mut unique = Vec::new();
    for (i, own) in results.iter().enumerate() {
        let superseded = results
            .iter()
            .enumerate()
            .any(|(j, other)| j != i && supersedes(other, own, j > i, luma, &mut bridges));
        if !superseded {
            unique.push(own.clone());
        }
    }
    if unique.is_empty() {
        return Err(Exceptions::NOT_FOUND);
    }
    Ok(unique)
}

/// Whether `other` makes `own` a duplicate (module doc, change 5): of two
/// reads of one symbol the later one stays, and a linear read inside a
/// matrix symbol never does.
fn supersedes(
    other: &RXingResult,
    own: &RXingResult,
    later: bool,
    luma: &LumaImage,
    bridges: &mut u32,
) -> bool {
    let same_content =
        || own.getText() == other.getText() && own.getBarcodeFormat() == other.getBarcodeFormat();
    let (own_linear, other_linear) = (
        linear(*own.getBarcodeFormat()),
        linear(*other.getBarcodeFormat()),
    );
    let (mine, theirs) = (own.getPoints(), other.getPoints());
    if mine.is_empty() || theirs.is_empty() {
        // a read without points cannot be told apart: upstream
        return later && same_content();
    }
    match (own_linear, other_linear) {
        (true, true) => {
            later
                && same_content()
                && match (scan_line(mine), scan_line(theirs)) {
                    // only a proven duplicate goes: unproven ones stay for
                    // the grouping to keep together, flagged
                    (Some(a), Some(b)) => {
                        occurrence::compare_linear(luma, a, b, bridges) == occurrence::Verdict::Same
                    }
                    _ => true,
                }
        }
        (true, false) => matrix_quad(other)
            .is_some_and(|quad| mine.iter().all(|p| inside(&quad, Point { x: p.x, y: p.y }))),
        (false, true) => false,
        (false, false) => later && same_content() && boxes_meet(mine, theirs),
    }
}

/// The convex quad a matrix read's points describe, clockwise, in rxing
/// 0.9.3's orders: the QR port's [TL, TR, BL, BR] or the lineage reader's
/// [BL, TL, TR, alignment] (whichever closes a convex quad), Data Matrix and
/// Aztec as given, PDF417's eight vertices [TL, BL, TR, BR, …].
fn matrix_quad(result: &RXingResult) -> Option<[Point; 4]> {
    let p: Vec<Point> = result
        .getPoints()
        .iter()
        .map(|p| Point { x: p.x, y: p.y })
        .collect();
    let orders: &[[usize; 4]] = match (*result.getBarcodeFormat(), p.len()) {
        (F::PDF_417, 8) => &[[0, 2, 3, 1]],
        (_, 4) => &[[0, 1, 2, 3], [0, 1, 3, 2]],
        _ => &[],
    };
    orders
        .iter()
        .find_map(|order| occurrence::clockwise_quad(&order.map(|i| p[i])))
}

/// Whether `p` lies inside the clockwise convex `quad`, one pixel of
/// tolerance.
fn inside(quad: &[Point; 4], p: Point) -> bool {
    (0..4).all(|i| {
        let (a, b) = (quad[i], quad[(i + 1) % 4]);
        let (ex, ey) = (f64::from(b.x - a.x), f64::from(b.y - a.y));
        let length = ex.hypot(ey);
        let cross = ex * f64::from(p.y - a.y) - ey * f64::from(p.x - a.x);
        length > 0.0 && cross / length >= -1.0
    })
}

/// Whether the boxes of two point sets meet.
fn boxes_meet(a: &[rxing::Point], b: &[rxing::Point]) -> bool {
    let bounds = |points: &[rxing::Point]| {
        points.iter().fold(
            (
                f32::INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
            ),
            |(x0, y0, x1, y1), p| (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y)),
        )
    };
    let (ax0, ay0, ax1, ay1) = bounds(a);
    let (bx0, by0, bx1, by1) = bounds(b);
    ax0 <= bx1 && bx0 <= ax1 && ay0 <= by1 && by0 <= ay1
}

/// The two farthest points of a linear read.
fn scan_line(points: &[rxing::Point]) -> Option<[Point; 2]> {
    let mut best: Option<([Point; 2], f32)> = None;
    for (i, a) in points.iter().enumerate() {
        for b in &points[i + 1..] {
            let length = (a.x - b.x).hypot(a.y - b.y);
            if best.is_none_or(|(_, own)| length > own) {
                best = Some(([Point { x: a.x, y: a.y }, Point { x: b.x, y: b.y }], length));
            }
        }
    }
    best.filter(|(_, length)| *length >= 1.0)
        .map(|(line, _)| line)
}

/// Upstream's recursion (`GenericMultipleBarcodeReader::doDecodeMultiple`),
/// with changes 2 to 4 of the module doc.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "upstream's crop arithmetic on finite in-image point coordinates"
)]
fn recurse<R: NodeReader, B: Binarizer>(
    reader: &mut R,
    image: &mut BinaryBitmap<B>,
    hints: &DecodeHints,
    results: &mut Vec<RXingResult>,
    (x_offset, y_offset): (u32, u32),
    depth: u32,
) {
    if depth > MAX_DEPTH {
        return;
    }
    let width = image.get_width();
    let height = image.get_height();
    let mut passed = Vec::new();
    let extent = loop {
        reader.reset();
        let Ok(result) = reader.decode_with_hints(image, hints) else {
            break None;
        };
        let format = *result.getBarcodeFormat();
        let (result, extent) = normalized(result, width, height);
        results.push(translated(result, x_offset, y_offset));
        if !extent.is_empty() {
            break Some(extent);
        }
        // nothing to crop around: the node again, past this format
        if passed.len() >= MAX_PASSED || !reader.pass_over(format) {
            break None;
        }
        passed.push(format);
    };
    for format in passed {
        reader.readmit(format);
    }
    let Some(extent) = extent else {
        return;
    };
    let (mut min_x, mut min_y) = (width as f32, height as f32);
    let (mut max_x, mut max_y) = (0.0_f32, 0.0_f32);
    for p in extent {
        min_x = min_x.min(p.x);
        min_y = min_y.min(p.y);
        max_x = max_x.max(p.x);
        max_y = max_y.max(p.y);
    }
    // the normalized points can lie on the image's far edges
    let (max_x, max_y) = (max_x.min(width as f32), max_y.min(height as f32));
    if min_x > MIN_DIMENSION_TO_RECUR {
        recurse(
            reader,
            &mut image.crop(0, 0, min_x as usize, height),
            hints,
            results,
            (x_offset, y_offset),
            depth + 1,
        );
    }
    if min_y > MIN_DIMENSION_TO_RECUR {
        recurse(
            reader,
            &mut image.crop(0, 0, width, min_y as usize),
            hints,
            results,
            (x_offset, y_offset),
            depth + 1,
        );
    }
    if max_x < (width as f32) - MIN_DIMENSION_TO_RECUR {
        recurse(
            reader,
            &mut image.crop(max_x as usize, 0, width - max_x as usize, height),
            hints,
            results,
            (x_offset + max_x as u32, y_offset),
            depth + 1,
        );
    }
    if max_y < (height as f32) - MIN_DIMENSION_TO_RECUR {
        recurse(
            reader,
            &mut image.crop(0, max_y as usize, width, height - max_y as usize),
            hints,
            results,
            (x_offset, y_offset + max_y as u32),
            depth + 1,
        );
    }
}

/// A read's points in its `width` × `height` crop, in the edge convention
/// (pixel (i, j) spans [i, i+1) × [j, j+1)), and the points the recursion
/// crops around: the normalized ones, or rxing's own when none survive. The
/// QR family, Data Matrix and Aztec keep rxing's points (the QR readers'
/// conventions are the engine's: `rxing_engine::footprint`).
fn normalized(
    result: RXingResult,
    width: usize,
    height: usize,
) -> (RXingResult, Vec<rxing::Point>) {
    let raw = result.getPoints().to_vec();
    let orientation = match result.getRXingResultMetadata().get(&MetaKey::ORIENTATION) {
        Some(MetaValue::Orientation(degrees)) => *degrees,
        _ => 0,
    };
    #[expect(
        clippy::cast_precision_loss,
        reason = "crop dimensions bounded by the engine-capped image"
    )]
    let (w, h) = (width as f32, height as f32);
    let format = *result.getBarcodeFormat();
    let points = match format {
        F::PDF_417 => pdf417_points(&raw, orientation, (w, h)),
        // measured from the row's first light pixel, and past the stop
        // character on the right (rxing 0.9.3 `coda_bar_reader.rs`,
        // `telepen_reader.rs`): no point of the image
        F::CODABAR | F::TELEPEN => Vec::new(),
        _ if linear(format) => linear_points(format, orientation, &raw, (w, h)),
        _ => return (result, raw),
    };
    let extent = if points.is_empty() {
        raw
    } else {
        points.clone()
    };
    (with_points(&result, points), extent)
}

/// rxing's PDF417 detector reports its vertices in the bit matrix it rotated
/// (`pdf_417_detector.rs`, `applyRotation`, orientation 90, 180 or 270):
/// each goes back through the inverse of `BitMatrix::rotate`. Across the
/// rows its vertices are pattern edges; along them, row indices: a bottom
/// vertex (1, 3, 5 and 7, when all eight are present) is its last row's
/// index, one pixel above that row's lower edge.
fn pdf417_points(raw: &[rxing::Point], orientation: i32, (w, h): (f32, f32)) -> Vec<rxing::Point> {
    raw.iter()
        .enumerate()
        .map(|(i, p)| {
            let bottom = raw.len() == 8 && i % 2 == 1;
            let (x, y) = (p.x, if bottom { p.y + 1.0 } else { p.y });
            match orientation.rem_euclid(360) {
                90 => point(w - y, x),
                180 => point(w - x, h - y),
                270 => point(y, h - x),
                _ => point(x, y),
            }
        })
        .collect()
}

/// A linear read's points. rxing 0.9.3 places them on the index of the row
/// it read (across the line) and on pattern edges or their centres (along
/// it), and maps a read back with `n - x - 1`, a pixel-index formula: an
/// upside-down row (orientation 180, `one_d_reader.rs`) and a rotated image
/// (270, `multi_format_one_d_reader.rs`; 90 or 450 when both). Each point
/// goes to the centre of its row, and a coordinate mapped out of a reversed
/// row moves one pixel. An upside-down read moves only its first two
/// points: the others stay in the reversed row's frame and are dropped. A
/// `DataBar` (RSS-14) also mirrors its right finder (points 2 and 3) out of
/// a reversed row of its own (`rss_14_reader.rs`,
/// `parseFoundFinderPattern`). Code 39 reports half its stop character's
/// crop-local end (`code_39_reader.rs`: `(lastStart + lastPatternSize) / 2`):
/// that end is recovered exactly, in any orientation.
fn linear_points(
    format: BarcodeFormat,
    orientation: i32,
    raw: &[rxing::Point],
    (w, h): (f32, f32),
) -> Vec<rxing::Point> {
    let quarter = orientation.rem_euclid(360);
    let reversed = matches!(quarter, 90 | 180);
    let rotated = matches!(quarter, 90 | 270);
    let kept = if reversed {
        &raw[..raw.len().min(2)]
    } else {
        raw
    };
    kept.iter()
        .enumerate()
        .map(|(i, p)| {
            if format == F::CODE_39 && i == 1 {
                return match quarter {
                    180 => point(2.0 * p.x + 2.0 - w, p.y + 0.5),
                    270 => point(p.x + 0.5, 2.0 * p.y),
                    90 => point(p.x + 0.5, 2.0 * p.y + 2.0 - h),
                    _ => point(2.0 * p.x, p.y + 0.5),
                };
            }
            let mirrored = reversed || (format == F::RSS_14 && i >= 2);
            let along = if mirrored { 1.0 } else { 0.0 };
            if rotated {
                point(p.x + 0.5, p.y + along)
            } else {
                point(p.x + along, p.y + 0.5)
            }
        })
        .collect()
}

/// `result` with other points, everything else kept.
fn with_points(result: &RXingResult, points: Vec<rxing::Point>) -> RXingResult {
    let mut rebuilt = RXingResult::new_complex(
        result.getText(),
        result.getRawBytes().to_vec(),
        result.getNumBits(),
        points,
        *result.getBarcodeFormat(),
        result.getTimestamp(),
    );
    rebuilt.putAllMetadata(result.getRXingResultMetadata().clone());
    rebuilt
}

/// Upstream's `translatePoints`: the result in the uncropped image.
#[expect(
    clippy::cast_precision_loss,
    reason = "crop offsets bounded by the engine-capped image"
)]
fn translated(result: RXingResult, x_offset: u32, y_offset: u32) -> RXingResult {
    if result.getPoints().is_empty() || (x_offset, y_offset) == (0, 0) {
        return result;
    }
    let moved: Vec<rxing::Point> = result
        .getPoints()
        .iter()
        .map(|p| point(p.x + x_offset as f32, p.y + y_offset as f32))
        .collect();
    with_points(&result, moved)
}
