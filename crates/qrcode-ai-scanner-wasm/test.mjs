// Smoke test for the BUILT pkg/ (run scripts/build-wasm.sh first; node ≥18).
// Exercises the full exported surface — scan_image, scan_frame, version and
// every positional argument — through the same JS the browser loads. Every
// assertion is exact or relational and none reads a clock: a judgment the
// wall-clock budget cuts is absent (score: null), so every scan that asserts
// a score runs unbudgeted (budget_ms 0).
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
const variant = process.env.WASM_VARIANT === "compatible" ? "compatible/" : "";
const { initSync, scan_image, scan_frame, version } = await import(`./pkg/${variant}qrcode-ai-scanner.js`);

initSync({ module: readFileSync(new URL(`./pkg/${variant}qrcode-ai-scanner_bg.wasm`, import.meta.url)) });

const here = (rel) => new URL(rel, import.meta.url);
const pkg = JSON.parse(readFileSync(here("./pkg/package.json"), "utf8"));
const markers = JSON.parse(readFileSync(here("../../spec/scan-report.schema.json"), "utf8")).$defs.Versions.properties;
const fixture = (rel) => new Uint8Array(readFileSync(here(`../../fixtures/${rel}`)));
const attempts = (report) => report.trace.stages.reduce((n, stage) => n + stage.transforms_tried, 0);
const texts = (report) => report.detections.map((d) => d.content.text);
const AXES = ["resolution", "blur", "contrast", "perspective", "rotation", "lighting"];

// A readable matrix code must not end the pyramid before a neighboring 1D
// symbol gets a sufficient-resolution read. Run in both package variants.
for (const profile of ["fast", "frame", "full"]) {
  const control = scan_image(fixture("symbology/code128-stock-4827.png"), profile, undefined, undefined, 0);
  assert.deepEqual(control.detections.map(d => [d.symbology, d.content.text]), [["code128", "STOCK-4827"]]);
  const mixed = scan_image(fixture("symbology/mixed-code128-qr.png"), profile, undefined, undefined, 0);
  assert.deepEqual(mixed.detections.map(d => [d.symbology, d.content.text]).sort(),
    [["code128", "STOCK-4827"], ["qr_code", "https://qrc-ai.com/76xMa"]]);
  const barcode = mixed.detections.find(d => d.symbology === "code128");
  assert.equal(barcode.meta.version, null, "linear read must not receive QR metadata");
  const shortCode39 = scan_image(fixture("symbology/code39-short-j.png"), profile, undefined, undefined, 0);
  assert.deepEqual(shortCode39.detections.map(d => [d.symbology, d.content.text]), [["code39", "J"]]);
  const artistic = scan_image(fixture("artistic/github-orb-code39-regression.png"), profile, undefined, undefined, 0);
  assert.deepEqual(artistic.detections.map(d => [d.symbology, d.content.text]), [["qr_code", "https://qrc-ai.com/QVR9a"]],
    "QR texture must not become a false Code39 J");
}
console.log(`PASS ${process.env.WASM_VARIANT ?? 'simd'} mixed QR + Code128: fast, frame, full`);

// one version end to end: the generated manifest, the module, the report
assert.equal(version(), pkg.version);

// scan_image: clean symbol → exact content + a complete unbudgeted judgment
const cleanBytes = fixture("clean/OK_68ms_100_4e875a2c.png");
const clean = scan_image(cleanBytes, "full", undefined, undefined, 0);
assert.equal(clean.trace.budget_exhausted, false, "an unbounded scan is final");
assert.deepEqual(clean.versions, {
  scanner: pkg.version,
  pipeline: markers.pipeline.const,
  score_contract: markers.score_contract.const,
});
assert.equal(clean.detections.length, 1);
const [det] = clean.detections;
assert.equal(det.symbology, "qr_code");
assert.equal(det.content.text, "https://qrc-ai.com/76xMa");
assert.equal(det.content.charset, "utf8");
assert.equal(det.payload.kind, "url");
assert.equal(det.payload.url, "https://qrc-ai.com/76xMa");
assert.equal(det.meta.inverted, false);
assert.equal(clean.score.weights_run, 100);
assert.deepEqual(clean.score.axes.map((a) => a.axis), AXES);
assert.ok(Number.isInteger(clean.score.value) && clean.score.value >= 70 && clean.score.value <= 100,
  `score ${clean.score.value}`);
assert.equal(clean.score.iso15415.overall, "a");

// every f32 field (corners, the UEC margin, ISO values, finder integrity,
// alpha coverage) carries the decimal the JSON report prints — 0.612, never
// the f32 widened to 0.6119999885559082. The trace's wall clock is f64.
const f32Decimal = (value) => {
  const single = Math.fround(value);
  for (let digits = 1; digits <= 9; digits++) {
    const text = single.toPrecision(digits);
    if (Math.fround(Number(text)) === single) return Number(text);
  }
  return single;
};
const floats = (node) =>
  typeof node === "number"
    ? (Number.isInteger(node) ? [] : [node])
    : node !== null && typeof node === "object"
      ? Object.values(node).flatMap(floats)
      : [];
const reported = floats({ ...clean, trace: null });
assert.ok(reported.length > 0, "a scored report carries f32 fields");
assert.deepEqual(reported.map(f32Decimal), reported);

// the blob template class (an editor OUTPUT) decodes in full, not fast —
// the exact reason the landing integration uses the full profile
const monkey = fixture("artistic/blob-style-monkey-logo.webp");
assert.deepEqual(texts(scan_image(monkey, "full", undefined, undefined, 0)), ["https://qrc-ai.com/Uo54C"],
  "full must decode the blob class");
assert.deepEqual(texts(scan_image(monkey, "fast", undefined, undefined, 0)), [], "fast skips deep (documented)");

// budget_ms is positional arg #5: a 1 ms budget stops the Full ladder after
// fewer attempts than the unbounded run of the same never-decoding image.
// The attempt counts come from the trace; no clock is read.
const degraded = fixture("degraded/FAIL_1491ms_0_584c998c.png");
const cut = scan_image(degraded, "full", undefined, undefined, 1);
const whole = scan_image(degraded, "full", undefined, undefined, 0);
assert.deepEqual(texts(cut), []);
assert.deepEqual(texts(whole), [], "the negative sample never decodes");
assert.ok(attempts(cut) < attempts(whole), `1 ms: ${attempts(cut)} attempts · unbounded: ${attempts(whole)}`);
// fewer attempts than the unbounded walk = planned attempts the budget
// prevented: the cut report says it is partial, the unbounded one final
assert.equal(cut.trace.budget_exhausted, true, "the empty 1 ms report is not a final no-QR");
assert.equal(whole.trace.budget_exhausted, false);
// 0 = UNBOUNDED, not a 0 ms budget (that would return empty before any attempt)
const v2l = fixture("clean/gen_v2_l.png");
assert.deepEqual(texts(scan_image(v2l, "fast", undefined, undefined, 0)), ["qrc.ai/v2l"], "budget 0 means unbounded");

// Option domains (spec/02-profiles.md § Option domains), the same table as the
// Node smoke: 0.10.1 turned budget_ms 1e300 into an unbounded scan, disabled
// the size cap on max_dimension -1 (ToUint32) and dropped max_pixels 0 / NaN /
// Infinity for the default or no cap at all. A zero-ms deadline has always
// passed, so the cut below reads no clock.
const fastScan = (dim, px, ms, bytes = v2l) => scan_image(bytes, "fast", dim, px, ms);
const outcome = (r) => [texts(r), r.trace.budget_exhausted];
// unbounded, or the 49.7-day maximum (no machine load reaches it)
for (const ms of [0, -0, -1, NaN, Infinity, -Infinity, 4294967295]) {
  assert.deepEqual(outcome(fastScan(undefined, undefined, ms)), [["qrc.ai/v2l"], false],
    `budget_ms ${ms}: unbounded or ample`);
}
for (const ms of [0.5, Number.MIN_VALUE]) {
  assert.deepEqual(outcome(fastScan(undefined, undefined, ms)), [[], true], `budget_ms ${ms} floors to a zero-ms budget`);
}
const refused = (option) => new RegExp(`invalid option \`${option}\`: .* \\[QRS-006\\]$`);
for (const ms of [4294967296, 4294967297, 1e300, Number.MAX_VALUE]) {
  assert.throws(() => fastScan(undefined, undefined, ms), refused("budget_ms"), `budget_ms ${ms} is above the maximum`);
}
for (const dim of [0, -0, -1, 0.5, 4096.7, NaN, Infinity, -Infinity, 4294967296]) {
  assert.throws(() => fastScan(dim, undefined, 0), refused("max_dimension"), `max_dimension ${dim}`);
}
for (const px of [0, -1, 0.5, 1.5, NaN, Infinity, 9007199254740992]) {
  assert.throws(() => fastScan(undefined, px, 0), refused("max_pixels"), `max_pixels ${px}`);
}
// each domain's extremes are caps, not option errors: 1 refuses this image
// under the cap's own code, the ceilings admit it
assert.throws(() => fastScan(1, undefined, 0), /\[QRS-002\]$/);
assert.throws(() => fastScan(undefined, 1, 0), /\[QRS-003\]$/);
for (const [dim, px] of [[4294967295, undefined], [undefined, 9007199254740991], [null, null]]) {
  assert.deepEqual(outcome(fastScan(dim, px, 0)), [["qrc.ai/v2l"], false], `caps ${dim} · ${px}`);
}
// the option is reported before the input is read; scan_frame shares the budget domain
assert.throws(() => fastScan(0, undefined, 0, new Uint8Array([1, 2, 3])), refused("max_dimension"));
const blank = new Uint8Array(16 * 16 * 4).fill(255);
assert.throws(() => scan_frame(blank, 16, 16, "fast", 4294967297), refused("budget_ms"));
assert.deepEqual(outcome(scan_frame(blank, 16, 16, "fast", 0.5)), [[], true], "a zero-ms frame budget is a cut");
assert.deepEqual(outcome(scan_frame(blank, 16, 16, "fast", NaN)), [[], false], "NaN is unbounded on frames too");

// score_skip_axes (#6) + score_skip_checks (#7): skipped axes absent, skipped
// sections null, and a skipped check never moves the composite (spec/04)
const v2lFull = scan_image(v2l, "full", undefined, undefined, 0);
const skipped = scan_image(v2l, "full", undefined, undefined, 0, ["perspective", "rotation"]);
assert.deepEqual(skipped.score.axes.map((a) => a.axis), ["resolution", "blur", "contrast", "lighting"]);
assert.equal(skipped.score.weights_run, 70);
const noChecks = scan_image(v2l, "full", undefined, undefined, 0, undefined, ["uec", "iso15415"]);
assert.equal(noChecks.score.uec, null, "skipped uec must be null");
assert.equal(noChecks.score.iso15415, null, "skipped iso15415 must be null");
assert.equal(noChecks.score.value, v2lFull.score.value);
assert.deepEqual(noChecks.score.axes, v2lFull.score.axes);
assert.throws(
  () => scan_image(v2l, "full", undefined, undefined, undefined, undefined, ["margin"]),
  /unknown score check/,
  "typo'd check must throw loudly",
);

// scan_frame: raw RGBA path (browser ImageData shape) — default frame profile
const side = 64;
const rgba = new Uint8Array(side * side * 4).fill(255); // white field: valid, nothing to find
const frame = scan_frame(rgba, side, side);
assert.deepEqual(texts(frame), []);
assert.equal(frame.score, null, "frame profile skips scoring");
// explicit profile + budget through scan_frame (#4 · #5)
const framed = scan_frame(rgba, side, side, "fast", 0);
assert.deepEqual(texts(framed), []);
assert.equal(framed.score, null, "no detection, no judgment");

// alpha_background (#8): a transparent input carries the alpha block through
// the serde_wasm_bindgen boundary (the skip_serializing_if key must be ABSENT
// on the JS object, not nulled — the serializer's serialize_missing_as_null
// must not resurrect it on opaque reports)
const transparent = new Uint8Array(readFileSync(here("../../playground/public/samples/transparent.png")));
const flat = scan_image(transparent, "full", undefined, undefined, 0);
assert.deepEqual(texts(flat), ["https://qrc-ai.com/76xMa"], "the flatten rescues the canvas-export class");
assert.equal(flat.alpha.mode, "auto");
assert.equal(flat.alpha.background, "white");
assert.equal(flat.alpha.coverage, Number(flat.alpha.coverage.toFixed(3)),
  "coverage is the 3-decimal value the JSON prints");
assert.equal(flat.alpha.envelope.placement, "light_only");
assert.deepEqual(flat.alpha.envelope.safe_luma, [[32, 255]]);
assert.deepEqual(
  flat.hints.filter((h) => h.hint === "alpha_background_dependent" || h.hint === "add_background_plate"),
  [{ hint: "alpha_background_dependent", placement: "light_only" }, { hint: "add_background_plate", color: "white" }],
  "the narrow envelope drives the hint and its remedy",
);
assert.ok(!("alpha" in clean), "opaque reports omit the alpha key entirely");
const opaqueForced = scan_image(v2l, "full", undefined, undefined, 0, undefined, undefined, "white");
assert.deepEqual(texts(opaqueForced), ["qrc.ai/v2l"]);
assert.ok(!("alpha" in opaqueForced), "forced mode on an opaque input still omits the key");
const dropped = scan_image(transparent, "full", undefined, undefined, 0, undefined, undefined, "none");
assert.deepEqual(texts(dropped), [], "none = the pre-0.9 exporter-dependent path");
assert.throws(
  () => scan_image(v2l, "full", undefined, undefined, undefined, undefined, undefined, "transparent"),
  /unknown alpha background/,
  "typo'd alpha background must throw loudly",
);
// alpha_palette (#9): per-color verdicts inside one scan, request order
const themed = scan_image(transparent, "full", undefined, undefined, 0, undefined, undefined, undefined, ["#f3f4f6", "black"]);
assert.deepEqual(
  themed.alpha.envelope.palette.map(({ background, background_luma, decoded }) => ({ background, background_luma, decoded })),
  [
    { background: "#f3f4f6", background_luma: 244, decoded: true },
    { background: "black", background_luma: 0, decoded: false },
  ],
);
assert.throws(
  () => scan_image(v2l, "full", undefined, undefined, undefined, undefined, undefined, undefined, ["auto"]),
  /unknown palette color/,
  "modes are not colors — throw loudly",
);

// score_preset (#10): design is sugar over skipping perspective + rotation
const designed = scan_image(v2l, "full", undefined, undefined, 0, undefined, undefined, undefined, undefined, "design");
assert.deepEqual(designed.score, skipped.score);
assert.throws(
  () => scan_image(v2l, "full", undefined, undefined, 0, ["blur"], undefined, undefined, undefined, "design"),
  /mutually exclusive/,
  "preset + explicit list must throw loudly",
);

// invalid bytes → typed throw, not a crash
assert.throws(() => scan_image(new Uint8Array([1, 2, 3])), /QRS-001/);

console.log(`wasm pkg OK — ${version()}`);
