//! # QR Code AI Scanner
//!
//! Decoding + scannability validation for artistic, AI-generated, and
//! photo-captured QR codes — the codes that break standard scanners.
//!
//! Contract (the normative wire format lives in the `spec/` directory):
//!
//! - **No QR found is `Ok`** with empty detections — `Err` is reserved for real
//!   faults (corrupt image, invalid buffer, cancellation).
//! - **Deterministic**: same bytes + same config + same scanner build ⇒ the
//!   same report, bit for bit on a given platform (wall clock aside) — no
//!   RNG anywhere. Three caveats, all bounded:
//!   - a wall-clock budget makes the CUT point machine-dependent. A cut
//!     report says so (`trace.budget_exhausted: true`), one that does not
//!     is the unbounded report, and a judgment the budget cannot complete
//!     is absent, never partial. Set `budget_ms: None` for strictly
//!     identical reports; a cancelled scan stays `QRS-005`;
//!   - CROSS-platform the libm transcendentals (warp kernels, gaussian
//!     blur) may differ in the last ulp — a stress cell sitting exactly on
//!     a knee can flip between platforms;
//!   - rxing's PDF417 decoder (0.9.1–0.9.3) breaks codeword-confidence ties
//!     in randomized `HashMap` order, so a PDF417 symbol near a decode
//!     threshold can score, and a marginal one decode, differently between
//!     runs.
//! - **Sync by design**: async belongs to the bindings (napi/wasm), never here.
//! - **Engine-isolated**: third-party decoder panics are caught at the engine
//!   boundary and recorded in the trace; the ladder continues.

#[cfg(not(any(feature = "engine-rxing", feature = "engine-rqrr")))]
compile_error!(
    "qrcode-ai-scanner requires at least one engine feature: `engine-rxing` or `engine-rqrr`"
);

mod alpha;
mod engine;
mod error;
mod gs1;
mod input;
mod ladder;
mod matrix;
pub mod options;
mod payload;
mod report;
mod rescue;
mod score;
mod transform;

pub use error::{Result, ScanError};
pub use gs1::Gs1Element;
pub use input::{ImageInput, Limits};
pub use ladder::{
    AlphaBackground, CancelToken, ScanConfig, ScanProfile, ScoreCheck, ScoreDepth, ScorePreset,
};
pub use payload::Payload;
pub use report::{
    AlphaEnvelope, AlphaMode, AlphaPaletteProbe, AlphaPlacement, AlphaProbe, AlphaReport,
    AxisScore, Charset, DecodedContent, Detection, EcLevel, EciSegment, EngineKind, Fnc1Position,
    GeometryKind, Grade, Hint, Iso15415Report, IsoGrade, IsoParameter, Occurrence,
    OccurrenceGeometry, PipelineTrace, PlateColor, Point, QrMeta, Saturation, ScanReport, Score,
    StageTrace, StressAxis, StructuralReport, Symbology, UecGrade, UecReport, Versions,
};

/// Fuzz-only entry points — cargo-fuzz builds the whole graph with
/// `--cfg fuzzing`; this surface does not exist in normal builds.
#[cfg(fuzzing)]
#[doc(hidden)]
pub mod fuzzing {
    /// Drive every text classifier (plain, FNC1 first, FNC1 second) —
    /// they parse attacker-controlled decoded content and must never panic.
    pub fn classify_text(text: &str) {
        let _ = crate::payload::classify(text);
        let _ = crate::payload::classify_fnc1(text);
        let _ = crate::classify_aim_application(text);
    }

    /// Drive the shared segment parser over attacker-controlled data
    /// codewords, then the content interpretation over what it parsed. A
    /// rescued stream never reaches the engines, so these own the decode
    /// and must never panic (refusal is the correct answer).
    pub fn parse_bitstream(data: &[u8], version: u8) {
        crate::rescue::fuzz_api::parse_bitstream(data, version);
    }

    /// Drive the errors-and-erasures RS corrector over an adversarial block,
    /// parity count, and erasure set — must never panic (`None` on garbage).
    pub fn correct_block(block: &mut [u8], npar: usize, erasures: &[usize]) {
        let _ = crate::rescue::fuzz_api::correct_block(block, npar, erasures);
    }
}

/// Reusable QR scanner — configure once, scan many. `Send + Sync`, no
/// interior state: share one instance across threads freely.
#[derive(Debug, Clone)]
pub struct Scanner {
    config: ScanConfig,
    limits: Limits,
}

impl Scanner {
    /// Start building a scanner.
    #[must_use]
    pub fn builder() -> ScannerBuilder {
        ScannerBuilder::default()
    }

    /// Scan one input. "No QR found" is `Ok` with empty detections.
    ///
    /// # Errors
    /// Only real faults: an option outside its domain (`QRS-006`, checked
    /// before the input), invalid/oversized input (`QRS-001..004`) or
    /// cancellation (`QRS-005`) — never "nothing found".
    pub fn scan(&self, input: ImageInput<'_>) -> Result<ScanReport> {
        self.scan_cancellable(input, &CancelToken::new())
    }

    /// Scan with a cooperative cancellation token (checked between decode
    /// attempts — a running engine call is not interruptible).
    ///
    /// # Errors
    /// Same contract as [`Scanner::scan`], plus `QRS-005` once `cancel` fires.
    pub fn scan_cancellable(
        &self,
        input: ImageInput<'_>,
        cancel: &CancelToken,
    ) -> Result<ScanReport> {
        // Options first: a configuration no scan can honour is QRS-006
        // whatever the input, before a byte of it is decoded.
        self.config.validate()?;
        self.limits.validate()?;
        let normalized =
            transform::normalize_with(&input, &self.limits, self.config.alpha_background)?;
        let mut planes = normalized.planes;
        let mut alpha_context = normalized.alpha;
        // ONE wall-clock budget for the whole scan — decode ladder AND
        // scoring share it (attempt/cell-granular: an in-flight engine call
        // is not interruptible, which is why engine inputs are size-capped).
        let deadline = deadline_after(self.config.budget_ms)?;
        let mut outcome = ladder::run(&planes, &self.config, cancel, deadline)?;
        // Alpha `auto` retry: a zero-detection scan re-flattens over the
        // OPPOSITE background and walks the ladder once more (within the
        // same budget) — a mis-called content mean must never turn a
        // decodable design into a false "no detection". Both walks stay in
        // the trace; `fallback_used` marks a RESCUE, not an attempt. A
        // stream the first walk refused stays refused in the second: it
        // starts from the first walk's refusals and parsed contents.
        let mut fallback_used = false;
        if outcome.merged.is_empty()
            && let Some(context) = &mut alpha_context
            && let Some((alt_planes, alt_background)) = context.opposite_planes()
        {
            // the fold carries the second walk's cut into the trace
            let alt_outcome =
                ladder::run_seeded(&alt_planes, &self.config, cancel, deadline, outcome.seed())?;
            if outcome.fold_fallback(alt_outcome) {
                planes = alt_planes;
                context.adopt_background(alt_background);
                fallback_used = true;
            }
        }
        // Score the primary (first) detection when the profile asks for it;
        // its stress cells and envelope probes match its content (aligned
        // with it in `contents`), never its text. Each stage runs under its
        // own bound on the scan's deadline, which also tallies the decoder
        // panics its decodes caught (`trace.engine_panics`).
        // The scoring stage shares the engine panic posture: a panic in the
        // math path degrades to "no score", never a crash (native unwind;
        // on wasm panics trap regardless — documented limitation). A
        // contained panic is a fault, not a cut: the judgment is absent
        // whatever the budget, and no envelope is planned after it.
        let primary = outcome
            .merged
            .first()
            .zip(outcome.contents.first().map(|(key, _)| key));
        // Every planned unit the deadline prevents after the walks — the
        // judgment's, the envelope's — leaves the report partial
        // (`trace.budget_exhausted`); the walks' own cuts are in the trace.
        let mut cut = false;
        let mut probe_panics = 0u8;
        let mut faulted = false;
        let scored = match (primary, self.config.score_depth) {
            (Some((detection, key)), depth @ (ScoreDepth::Reduced | ScoreDepth::Full)) => {
                let mut bound = score::Bound::new(cancel, deadline);
                let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    score::evaluate(
                        &planes.luma,
                        detection,
                        key,
                        depth,
                        &self.config.score_skip_axes,
                        &self.config.score_skip_checks,
                        &mut bound,
                    )
                }));
                probe_panics = probe_panics.saturating_add(bound.panics());
                cut |= bound.cut();
                if let Ok(result) = attempt {
                    result?
                } else {
                    faulted = true;
                    None
                }
            }
            _ => None,
        };
        // The placement envelope — Full depth only (the sweep is a quality-
        // gate diagnostic), skippable through the same seam as every other
        // report section, and not planned after a contained fault of the
        // judgment: reported absent like any envelope not computed, it can
        // never make that fault read as a cut. Same panic posture as
        // scoring: the sweep shares the score module's math path, so a panic
        // there degrades to "no envelope", never a crash out of the public
        // API.
        let envelope = match (&alpha_context, primary) {
            (Some(context), Some((detection, key)))
                if !faulted
                    && self.config.score_depth == ScoreDepth::Full
                    && !self
                        .config
                        .score_skip_checks
                        .contains(&ScoreCheck::AlphaEnvelope) =>
            {
                let mut bound = score::Bound::new(cancel, deadline);
                let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    context.envelope(
                        key,
                        detection.symbology,
                        &self.config.alpha_palette,
                        &mut bound,
                    )
                }));
                probe_panics = probe_panics.saturating_add(bound.panics());
                match attempt {
                    Ok(result) => match result? {
                        alpha::Envelope::Measured(envelope) => Some(envelope),
                        alpha::Envelope::Undecodable => None,
                        alpha::Envelope::Cut => {
                            cut = true;
                            None
                        }
                    },
                    Err(_) => None,
                }
            }
            _ => None,
        };
        outcome.trace.engine_panics = outcome.trace.engine_panics.saturating_add(probe_panics);
        if cut {
            outcome.trace.budget_exhausted = Some(true);
        }
        let alpha_report =
            alpha_context.map(|context| context.into_report(fallback_used, envelope));
        Ok(build_report(outcome, scored, alpha_report))
    }

    /// Scan a batch — the generator best-of-N gate. Parallel under the
    /// `parallel` feature (results identical to sequential by construction).
    /// Per-input results: one `Err` does not fail the batch.
    #[must_use]
    pub fn scan_batch(&self, inputs: &[ImageInput<'_>]) -> Vec<Result<ScanReport>> {
        #[cfg(feature = "parallel")]
        {
            use rayon::prelude::*;
            inputs.par_iter().map(|input| self.scan(*input)).collect()
        }
        #[cfg(not(feature = "parallel"))]
        {
            inputs.iter().map(|input| self.scan(*input)).collect()
        }
    }
}

impl Default for Scanner {
    fn default() -> Self {
        Self::builder().build()
    }
}

/// Builder for [`Scanner`].
#[derive(Debug, Clone, Default)]
pub struct ScannerBuilder {
    profile: ScanProfile,
    limits: Limits,
}

impl ScannerBuilder {
    /// Select a scan profile (default: [`ScanProfile::Full`]).
    #[must_use]
    pub fn profile(mut self, profile: ScanProfile) -> Self {
        self.profile = profile;
        self
    }

    /// Override anti-DoS input limits.
    #[must_use]
    pub fn limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// Build the scanner.
    #[must_use]
    pub fn build(self) -> Scanner {
        Scanner {
            config: self.profile.config(),
            limits: self.limits,
        }
    }
}

/// The scan's deadline, `budget_ms` from now. Checked: a deadline the clock
/// cannot represent is `QRS-006`, never a panic and never silently
/// unbounded. Every admitted budget (at most [`ScanConfig::MAX_BUDGET_MS`],
/// about 49.7 days) is representable on the supported clocks.
fn deadline_after(budget_ms: Option<u64>) -> Result<Option<web_time::Instant>> {
    let Some(ms) = budget_ms else {
        return Ok(None);
    };
    web_time::Instant::now()
        .checked_add(std::time::Duration::from_millis(ms))
        .map(Some)
        .ok_or_else(|| {
            ScanError::invalid_option(
                "budget_ms",
                format!("a deadline {ms} ms from now is not representable"),
            )
        })
}

/// The text classifier for FNC1-in-second-position data (spec/05): an
/// AIM-assigned application's data, never GS1 — the plain classifier
/// without its no-FNC1 GS1 sniff (the only route to `Gs1` it has).
fn classify_aim_application(text: &str) -> Payload {
    match payload::classify(text) {
        Payload::Gs1 { .. } => Payload::Text,
        payload => payload,
    }
}

/// FNC1-in-first-position data without a verified text reading: the
/// element string is not parsed (spec/05).
fn gs1_not_parsed() -> Payload {
    Payload::Gs1 {
        elements: Vec::new(),
        gtin: None,
        conformant: false,
        issues: vec![
            "content has no verified text reading (charset unknown): element string not parsed"
                .to_owned(),
        ],
    }
}

/// What a content publishes of its account: the located designators,
/// `[]` for a declaration that was signalled but not located, no key
/// otherwise; and the FNC1 position when one was observed.
fn wire_account(
    key: &engine::content::ContentKey,
) -> (Option<Vec<EciSegment>>, Option<Fnc1Position>) {
    use engine::content::{EciEvidence, Fnc1};
    let eci = match &key.eci {
        EciEvidence::Located(marks) => Some(
            marks
                .iter()
                .map(|mark| EciSegment {
                    designator: mark.value,
                    start: u32::try_from(mark.start).unwrap_or(u32::MAX),
                })
                .collect(),
        ),
        EciEvidence::Signalled => Some(Vec::new()),
        EciEvidence::NoEci | EciEvidence::Unobserved => None,
    };
    let fnc1 = match key.fnc1 {
        Fnc1::First => Some(Fnc1Position::First),
        Fnc1::Second => Some(Fnc1Position::Second),
        Fnc1::None | Fnc1::Unobserved => None,
    };
    (eci, fnc1)
}

/// Assemble the public report from ladder output. Each detection's content
/// is the scanner's one reading of the reconciled content
/// (`engine::content::read`): `raw` the original payload, `eci` and `fnc1`
/// its observed account, `text` and `charset` per spec/01 rules 1-3. The
/// payload is routed by FNC1 position, symbology and charset (spec/05): no
/// structured reading is ever built from text the scanner could not read.
fn build_report(
    outcome: ladder::LadderOutcome,
    scored: Option<(Score, Vec<Hint>)>,
    alpha: Option<report::AlphaReport>,
) -> ScanReport {
    let (score, mut hints) = match scored {
        Some((score, hints)) => (Some(score), hints),
        None => (None, Vec::new()),
    };
    // A background-dependent envelope is generator-actionable exactly like
    // a low-contrast finding: pin a background layer, or constrain the
    // placement to the safe bands.
    if let Some(a) = alpha.as_ref()
        && let Some(envelope) = a.envelope.as_ref()
        && envelope.placement != report::AlphaPlacement::Any
    {
        hints.push(Hint::AlphaBackgroundDependent {
            placement: envelope.placement,
        });
        // …and the canonical remedy: an opaque plate behind the symbol —
        // robust everywhere, transparent look preserved around it.
        hints.push(Hint::AddBackgroundPlate {
            color: if a.content_luma < 128 {
                report::PlateColor::White
            } else {
                report::PlateColor::Black
            },
        });
    }
    let detections = outcome
        .merged
        .into_iter()
        .zip(outcome.contents)
        .map(|(m, (key, _))| {
            let (eci, fnc1) = wire_account(&key);
            // payload routing (spec/05): FNC1-first carriers are element
            // strings; retail 1D symbols ARE a GTIN (AI 01 semantics); text
            // the scanner could not read is text; FNC1-second data belongs
            // to an AIM application; everything else goes through the text
            // classifier
            let payload = if m.fnc1 && m.charset.is_lossy() {
                gs1_not_parsed()
            } else if m.fnc1 {
                payload::classify_fnc1(&m.text)
            } else if m.symbology.is_retail_gtin() {
                payload::classify_retail_gtin(&m.text, m.symbology)
            } else if m.charset.is_lossy() {
                Payload::Text
            } else if fnc1 == Some(Fnc1Position::Second) {
                classify_aim_application(&m.text)
            } else {
                payload::classify(&m.text)
            };
            Detection {
                symbology: m.symbology,
                content: DecodedContent {
                    text: m.text,
                    raw: m.raw,
                    charset: m.charset,
                    eci,
                    fnc1,
                },
                payload,
                corners: m.corners,
                meta: QrMeta {
                    version: m.version,
                    ec_level: m.ec,
                    mask: m.mask,
                    modules: m.version.and_then(QrMeta::modules_per_side),
                    inverted: m.corners.is_some().then_some(m.photometric_inverted),
                },
                engines: m.engines,
                occurrences: m.occurrences.into_iter().map(wire_occurrence).collect(),
            }
        })
        .collect();
    ScanReport {
        detections,
        score,
        hints,
        alpha,
        trace: outcome.trace,
        versions: Versions::current(),
    }
}

/// A validated footprint as a published occurrence.
fn wire_occurrence(footprint: engine::occurrence::Footprint) -> report::Occurrence {
    use engine::occurrence::Kind;
    report::Occurrence {
        geometry: report::OccurrenceGeometry {
            kind: match footprint.kind {
                Kind::Quad => report::GeometryKind::Quad,
                Kind::Segment => report::GeometryKind::Segment,
                Kind::Partial => report::GeometryKind::Partial,
                Kind::Unavailable => report::GeometryKind::Unavailable,
            },
            points: footprint.points,
            reader: footprint.detector,
        },
    }
}

#[cfg(test)]
mod tests {
    // The public contract is exercised via the re-exported surface; the
    // workspace deny targets `src/`, so tests opt back into unwrap/expect.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::{ImageInput, Limits, ScanConfig, ScanProfile, Scanner};

    fn clean_png() -> Vec<u8> {
        std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../fixtures/clean/gen_v5_q.png"
        ))
        .unwrap()
    }

    fn scanner(config: ScanConfig, limits: Limits) -> Scanner {
        Scanner::builder()
            .profile(ScanProfile::Custom(config))
            .limits(limits)
            .build()
    }

    #[test]
    #[cfg(not(target_family = "wasm"))]
    fn the_overflow_hypothesis_does_not_reproduce_on_a_native_clock() {
        // The refuted witness, kept: `now + u64::MAX ms` IS representable on
        // a native clock (about 1.8e16 s against i64 seconds), so the former
        // unchecked deadline did not panic for any u64 budget here. The
        // budget ceiling is the portable option domain, not a crash fix; the
        // checked addition only keeps an unrepresentable deadline an error.
        let now = std::time::Instant::now();
        let largest = std::time::Duration::from_millis(u64::MAX);
        assert!(now.checked_add(largest).is_some());
    }

    #[test]
    fn the_largest_admitted_budget_is_a_representable_deadline() {
        let max = std::time::Duration::from_millis(ScanConfig::MAX_BUDGET_MS);
        assert!(web_time::Instant::now().checked_add(max).is_some());
    }

    #[test]
    fn an_option_out_of_its_domain_is_qrs_006_before_the_input_is_read() {
        // checked first: beside unreadable bytes, the option is reported
        let fast = ScanConfig::fast;
        let cases = [
            (
                "budget_ms",
                ScanConfig {
                    budget_ms: Some(u64::MAX),
                    ..fast()
                },
                Limits::default(),
            ),
            (
                "budget_ms",
                ScanConfig {
                    budget_ms: Some(ScanConfig::MAX_BUDGET_MS + 1),
                    ..fast()
                },
                Limits::default(),
            ),
            (
                "pyramid_side",
                ScanConfig {
                    pyramid_side: 0,
                    ..fast()
                },
                Limits::default(),
            ),
            (
                "max_engine_side",
                ScanConfig {
                    max_engine_side: 0,
                    ..fast()
                },
                Limits::default(),
            ),
            (
                "alpha_palette",
                ScanConfig {
                    alpha_palette: vec![[0, 0, 0]; ScanConfig::MAX_ALPHA_PALETTE + 1],
                    ..fast()
                },
                Limits::default(),
            ),
            (
                "max_dimension",
                fast(),
                Limits {
                    max_dimension: 0,
                    ..Limits::default()
                },
            ),
            (
                "max_pixels",
                fast(),
                Limits {
                    max_pixels: 0,
                    ..Limits::default()
                },
            ),
        ];
        let png = clean_png();
        for (option, config, limits) in cases {
            let scan = scanner(config, limits);
            for input in [
                ImageInput::encoded(b"not an image"),
                ImageInput::encoded(&png),
            ] {
                let err = scan.scan(input).expect_err(option);
                assert_eq!(err.code(), "QRS-006", "{option}: {err}");
                assert!(err.to_string().contains(option), "{err}");
            }
        }
    }

    #[test]
    fn the_largest_budget_and_the_smallest_sides_still_scan() {
        let config = ScanConfig {
            budget_ms: Some(ScanConfig::MAX_BUDGET_MS),
            alpha_palette: vec![[0, 0, 0]; ScanConfig::MAX_ALPHA_PALETTE],
            ..ScanConfig::fast()
        };
        let report = scanner(config, Limits::default())
            .scan(ImageInput::encoded(&clean_png()))
            .unwrap();
        assert_eq!(report.detections.len(), 1);
        assert_eq!(report.trace.budget_exhausted, Some(false));
        let tiny = ScanConfig {
            pyramid_side: 1,
            max_engine_side: 1,
            ..ScanConfig::fast()
        };
        // a 1-pixel engine side reads nothing, but it is a valid request:
        // a final empty report, not an error and not a cut
        let report = scanner(tiny, Limits::default())
            .scan(ImageInput::encoded(&clean_png()))
            .unwrap();
        assert_eq!(report.detections.len(), 0);
        assert_eq!(report.trace.budget_exhausted, Some(false));
    }

    #[test]
    fn a_raw_frame_whose_byte_count_overflows_is_qrs_003() {
        // 2^32-1 square at four bytes a pixel overflows u64: the limits
        // allow it, the byte count cannot exist
        let limits = Limits {
            max_dimension: u32::MAX,
            max_pixels: u64::MAX,
        };
        let err = scanner(ScanConfig::fast(), limits)
            .scan(ImageInput::rgba8(&[0; 16], u32::MAX, u32::MAX))
            .unwrap_err();
        assert_eq!(err.code(), "QRS-003", "{err}");
    }

    #[test]
    fn builder_threads_custom_limits_into_the_scan() {
        // Traps the builder-plumbing survivor (`ScannerBuilder::limits` →
        // Default::default(), shard-0): a builder that discarded the
        // configured Limits would fall back to the 10_000px / 64MP defaults
        // and wave this input straight through. Pin that BOTH limit fields
        // are threaded from the builder into input validation.
        let data = vec![255u8; 64 * 64];

        // (a) dimension cap: 64px per side > 16px limit → QRS-002
        let strict_dim = Scanner::builder()
            .limits(Limits {
                max_dimension: 16,
                ..Limits::default()
            })
            .build();
        let err = strict_dim
            .scan(ImageInput::luma8(&data, 64, 64))
            .expect_err("64px input must exceed the 16px builder limit");
        assert_eq!(err.code(), "QRS-002", "dimension cap threaded from builder");

        // (b) pixel cap: 4096px total > 100px limit → QRS-003
        let strict_pixels = Scanner::builder()
            .limits(Limits {
                max_pixels: 100,
                ..Limits::default()
            })
            .build();
        let err = strict_pixels
            .scan(ImageInput::luma8(&data, 64, 64))
            .expect_err("4096px input must exceed the 100px builder limit");
        assert_eq!(err.code(), "QRS-003", "pixel cap threaded from builder");

        // Control: the SAME input sails through DEFAULT limits — proving the
        // configured cap, not the input, is what rejected it above (so the
        // builder genuinely threaded the value rather than defaulting it away).
        let report = Scanner::default()
            .scan(ImageInput::luma8(&data, 64, 64))
            .expect("default 10_000px / 64MP limits accept a 64px plane");
        assert!(report.detections.is_empty(), "a blank plane carries no QR");
    }

    /// A transparent v3-M symbol as RGBA: dark modules opaque black, every
    /// other pixel fully transparent (6 px modules, 4-module margin) — the
    /// `auto` flatten reads it over white, the Full profile scores it and
    /// measures its placement envelope. Returns the buffer and its side.
    fn transparent_symbol() -> (Vec<u8>, u32) {
        let code = qrcode::QrCode::with_version(
            b"https://qrc-ai.com/probe",
            qrcode::Version::Normal(3),
            qrcode::EcLevel::M,
        )
        .unwrap();
        let width = code.width();
        let side = (width + 8) * 6;
        let mut rgba = vec![0u8; side * side * 4];
        for (i, color) in code.to_colors().iter().enumerate() {
            if *color == qrcode::Color::Dark {
                let (mx, my) = (i % width + 4, i / width + 4);
                for dy in 0..6 {
                    for dx in 0..6 {
                        let at = ((my * 6 + dy) * side + mx * 6 + dx) * 4;
                        rgba[at + 3] = 255;
                    }
                }
            }
        }
        (rgba, u32::try_from(side).unwrap())
    }

    #[cfg(feature = "engine-rxing")]
    #[test]
    fn decoder_panics_in_score_cells_and_envelope_probes_reach_the_trace() {
        use crate::engine::PROBE_PANICS;
        // the walk reads the symbol with no injected fault; the judgment's
        // cells and the envelope's probes decode it alone, and each of their
        // rxing passes panics — caught and tallied at the engine boundary.
        // The cells are judged as before (rqrr still reads them): only the
        // count is new
        let (rgba, side) = transparent_symbol();
        let mut config = super::ScanConfig::full();
        config.budget_ms = None;
        let scanner = Scanner::builder()
            .profile(super::ScanProfile::Custom(config))
            .build();
        let clean = scanner.scan(ImageInput::rgba8(&rgba, side, side)).unwrap();
        assert_eq!(clean.trace.engine_panics, 0, "no real decoder panic");
        PROBE_PANICS.with(|injected| injected.set(Some(0)));
        let report = scanner.scan(ImageInput::rgba8(&rgba, side, side));
        let injected = PROBE_PANICS
            .with(|injected| injected.replace(None))
            .unwrap();
        let report = report.unwrap();
        assert!(injected > 0, "the probes decoded");
        assert_eq!(
            u32::from(report.trace.engine_panics),
            injected.min(u32::from(u8::MAX)),
            "every caught probe panic is counted once"
        );
        assert_eq!(
            report.detections, clean.detections,
            "the walk reads as without the fault: only single-symbology passes panic"
        );
        assert!(report.score.is_some(), "the judgment completes");
        assert!(
            report
                .alpha
                .as_ref()
                .is_some_and(|alpha| alpha.envelope.is_some()),
            "the envelope completes: {:?}",
            report.alpha
        );
    }

    #[test]
    fn a_contained_judgment_fault_plans_no_envelope_and_cuts_nothing() {
        use crate::score::JUDGMENT_FAULT;
        // a judgment that faults is absent whatever the budget: no envelope
        // is planned after it, so no unit the deadline prevents can make the
        // report read as cut — an absent score with detections and a final
        // trace stays a fault
        let (rgba, side) = transparent_symbol();
        let mut config = super::ScanConfig::full();
        config.budget_ms = None;
        let scanner = Scanner::builder()
            .profile(super::ScanProfile::Custom(config))
            .build();
        let whole = scanner.scan(ImageInput::rgba8(&rgba, side, side)).unwrap();
        assert!(
            whole.score.is_some()
                && whole
                    .alpha
                    .as_ref()
                    .is_some_and(|alpha| alpha.envelope.is_some()),
            "precondition: judged, envelope measured"
        );
        JUDGMENT_FAULT.with(|fault| fault.set(true));
        let faulted = scanner.scan(ImageInput::rgba8(&rgba, side, side));
        JUDGMENT_FAULT.with(|fault| fault.set(false));
        let faulted = faulted.unwrap();
        assert_eq!(faulted.detections, whole.detections);
        assert!(faulted.score.is_none(), "the fault is contained");
        let alpha = faulted.alpha.as_ref().expect("the alpha block stays");
        assert!(alpha.envelope.is_none(), "no envelope after the fault");
        assert!(faulted.hints.is_empty(), "{:?}", faulted.hints);
        assert_eq!(faulted.trace.budget_exhausted, Some(false));
    }

    #[test]
    fn a_background_plate_is_black_from_content_luma_128() {
        // the plate contrasts with the content: white behind content darker
        // than mid-grey (luma below 128), black from 128 up — the symbol's
        // modules here are opaque grey 128
        let (mut rgba, side) = transparent_symbol();
        for px in rgba.as_chunks_mut::<4>().0 {
            if px[3] == 255 {
                px[..3].fill(128);
            }
        }
        let mut config = super::ScanConfig::full();
        config.budget_ms = None;
        let scanner = Scanner::builder()
            .profile(super::ScanProfile::Custom(config))
            .build();
        let report = scanner.scan(ImageInput::rgba8(&rgba, side, side)).unwrap();
        let alpha = report.alpha.as_ref().expect("a transparent input");
        assert_eq!(alpha.content_luma, 128);
        let plates: Vec<_> = report
            .hints
            .iter()
            .filter_map(|hint| match hint {
                super::Hint::AddBackgroundPlate { color } => Some(*color),
                _ => None,
            })
            .collect();
        assert_eq!(
            plates,
            [super::report::PlateColor::Black],
            "{:?} {:?}",
            report.hints,
            alpha.envelope
        );
    }
}
