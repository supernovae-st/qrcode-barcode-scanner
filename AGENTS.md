# AGENTS.md — QR Code & Barcode Scanner

Instructions for AI coding agents (Codex · Cursor · opencode · any
harness) working in this repo. Humans: start at [README.md](README.md);
releases follow [RELEASING.md](RELEASING.md) to the letter. Cursor also
loads `.cursor/rules/scanner.mdc` (condensed always-on laws).

## Scanner identity and ownership

Read [SOURCE-AND-EXTRACTION.md](SOURCE-AND-EXTRACTION.md). Maintain one multiformat
scanner engine and one package family: **QR Code & Barcode Scanner**.
QR/barcode modes and SEO pages do not authorize
separate scanner engines, product extractions or alias/wrapper packages.

The source repository is `qrcode-ai-com/qrcode-barcode-scanner`. Its repository
identity, history and tags are retained from its previous names. Published
packages still use `qrcode-ai-scanner`; do not replace their install coordinates
with the repository name. QR Code AI's SaaS and its creation engine have separate
sources and release qualification. A mobile scanner binding is not a consumer
app. Component licenses do not by themselves determine the SaaS's license.

Preserve historical package names, tags, schema IDs, licenses and wire contracts.
Newly named packages need real builds and consumer qualification. A repository
rename or documentation amendment does not publish or retire any artifact.

## Current implementation

A Rust workspace producing EIGHT publish surfaces off one core —
QR/barcode decoding + scannability scoring, specialty: artistic/
AI-generated codes. Everything runs **locally** (no network calls
anywhere in the library).

| Path | Artifact | Registry |
|---|---|---|
| `crates/qrcode-ai-scanner` | core library | crates.io |
| `crates/qrcode-ai-scanner-cli` | `qrscan` binary | crates.io |
| `crates/qrcode-ai-scanner-node` | `@supernovae-st/qrcode-ai-scanner` (napi) | npm |
| `crates/qrcode-ai-scanner-wasm` | `@supernovae-st/qrcode-ai-scanner-wasm` | npm |
| `crates/qrcode-ai-scanner-py` | `qrcode-ai-scanner` (PyO3, workspace-excluded) | PyPI |
| `bindings/flutter` | `qrcode_ai_scanner` (FRB + cargokit) | pub.dev |
| `bindings/kotlin` | `com.github.supernovae-st:qrcode-ai-scanner` | JitPack |
| `bindings/swift` | `QrcodeAiScanner` (xcframework, tag-pinned) | SwiftPM |

**Downstream, this crate is "the judge"**: the generator's e2e floors,
the qrt templates layer and the 306-template factory all score through
the published version (pinned as `doctor::JUDGE_VERSION` in
qrcode-ai-templates). A release here makes those repos re-measure —
score honesty is a cross-repo contract.

## Ground truth, in priority order

1. **`spec/`** — the NORMATIVE contract (wire format, errors, score,
   payloads, hints, pipeline). If code and spec disagree, one has a bug.
2. `spec/scan-report.schema.json` + `spec/examples/*.json` — machine
   ground truth, produced by the real binary, validated in CI.
3. `docs/` — the Mintlify site (human-friendly; `docs/guides/agents.mdx`
   is written for you).
4. `bindings/report-types.d.ts` — the ONE TypeScript contract (both npm
   packages ship copies synced by `scripts/sync-report-types.mjs`).

## Commands

Use the compiler pin and locked gates in
[Local development checks](RELEASING.md#local-development-checks). The workflow
pin and the MSRV are separate contracts; an ambient Cargo compiler is not proof
of CI parity. Inspect `RUST_TOOLCHAIN` in `.github/workflows/ci.yml` when the pin
changes, and update command examples and condensed agent contexts together.

```bash
cargo +1.99.1 nextest run --workspace --locked  # count = actual executed tests
cargo +1.99.1 test --doc --workspace --locked
RUSTFLAGS="-D warnings" cargo +1.99.1 clippy --workspace --all-targets --all-features --locked
cargo +1.99.1 fmt --all --check
cargo +1.88 check -p qrcode-ai-scanner --all-features --locked  # MSRV floor
python3 scripts/check-type-parity.py # Rust ↔ TS ↔ JSON-Schema drift gate
cargo +1.99.1 run --locked -p xtask -- corpus-report
cargo +1.99.1 run --locked -p xtask -- sync-version --check
./scripts/build-wasm.sh              # wasm pkg (NEEDS binaryen ≥130 on PATH)
node crates/qrcode-ai-scanner-node/test.mjs   # node smoke (build first: npm run build)
node crates/qrcode-ai-scanner-wasm/test.mjs   # wasm smoke (build first)
```

Regeneration commands write files: `xtask gen-fixtures`, `xtask sync-version`
without `--check`, and `cargo fmt --all`. Use them for the intended owned change.
Dependency/core changes also require the runbook's four excluded lock-root
checks; a workspace pass does not cover Python, UniFFI, Flutter Rust or fuzz.

Deep checks (weekly `deep-checks.yml`): cargo-mutants in 85 shards, judged
by the `mutants-summary` job — red on a baseline failure or on any missed /
timed-out mutant absent from `.cargo/mutants-dispositions.toml` (after an
edit there: `python3 scripts/check-mutants-outcome.py --self-test`) · fuzz
×4 · rescue-stress (every emitted engine/rescue/extra QR and non-QR output is
judged against the single-symbol truth, exact output dispositions and pinned
recall/panic floors; rescue-path wrong output is forbidden in both the uncapped
and capped passes) · advisories. Judge-bump moments add the external
truth, `xtask corpus-report --external` (the `corpus-external.tsv` pins):
it runs locally with the corpus present — CI has no corpus, parses the
manifest only and reports the gate as NOT VERIFIED, never as a pass.

## Hard invariants (breaking these fails CI or review)

- **Zero `unwrap`/`expect`/`panic!` in `src/`** (tests are exempt). Clippy
  denies it.
- **No RNG anywhere** — the pipeline is deterministic by contract (known
  dependency exception: rxing's PDF417 decoder breaks confidence ties in
  randomized `HashMap` order — spec/04 § Determinism; the threaded
  determinism pin excludes the PDF417 fixtures).
- **A budget cut is flagged, never partial**: `trace.budget_exhausted` is
  `true` exactly when the deadline prevented a planned unit (spec/02 §
  Budget semantics); a judgment it interrupts is `score: null`. Bounded
  checks with no budget (grid re-derivations, the re-read, the end-of-walk
  pass, the confirmation decodes) never set it. A test that asserts a score
  runs unbounded, or asserts "absent with the flag set, or the unbounded
  value".
- **Exact content, keyed by identity**: a QR-family `content.raw` is the
  original payload (the scanner's own parse decides it wherever it can
  error-correct the grid); `unknown` is the only lossy `charset`, forever;
  reads become detections by content identity (symbology, `raw`, FNC1
  position, compatible `eci`), never by `text` (spec/01 § Content,
  07-pipeline property 6). `tests/exact_content.rs` pins the rows.
- **Refuse what the evidence contradicts**: a malformed stream is refused
  whichever decoder reads it; a QR Code content whose every measured
  correction margin is negative (past the Annex B reserve, or an rqrr
  decode of a grid the scanner cannot error-correct, at any version), and
  an invention-prone read no independent read confirmed, are not
  published (07-pipeline § Acceptance). The margin rule covers measured
  reads only: Micro QR, rMQR and other QR Code reads no parse measured are
  published as read, and a correction inside the reserve can still be
  wrong — never
  document the rule as infallible. Prefer absent to wrong; a recall
  trade-off is named in the CHANGELOG, not hidden.
- **Additive wire evolution only**: never rename/remove a JSON field or
  enum value. New variants are fine — consumers parse leniently.
- **Every contract change cascades** through ALL of: the Rust types →
  `bindings/report-types.d.ts` → `spec/scan-report.schema.json` → the
  relevant `spec/*.md` → `docs/`. The parity gate + `tests/spec_golden.rs`
  enforce most of it; the prose is on you.
- Wire names are **pinned by tests** (snapshot + unit). Changing serde
  casing is a breaking change.
- "No QR found" is `Ok` with empty detections — NEVER an error.
- Only the QR family carries `meta`/UEC/iso15415/rescue — other
  symbologies must never fabricate those fields.
- Decoded QR text is **attacker-controlled**: any new parser over it must
  be boundary-safe (see `gs1.rs` `split_head`) and fuzz-covered
  (`fuzz/fuzz_targets/fuzz_classify_text.rs`); any human-facing echo
  strips terminal controls (`sanitize_terminal` in the CLI).
- Colour only through `crates/qrcode-ai-scanner-cli/src/term.rs` (one
  seam per binary · `NO_COLOR > CLICOLOR_FORCE > TTY` · JSON and
  `--score-only` never carry an escape).

## Gotchas that have bitten before

- `crates/qrcode-ai-scanner-wasm/pkg/` is **generated** by wasm-pack —
  never hand-edit; `scripts/patch-wasm-pkg.mjs` owns package.json fields
  (incl. the `files` allowlist — wasm-pack wipes manual edits).
- wasm-pack's bundled wasm-opt is too old for Rust ≥1.87 output — use
  binaryen ≥130 (`brew install binaryen`; CI pins version_130).
- `corpus.toml` is TOML: control bytes (e.g. GS 0x1D) must be written
  as `\u001D` escapes, never as literal bytes.
- The score probes decode ONLY the scored symbology
  (`engine::FormatFilter::Only`) — calling `decode_all` in a stress cell
  reintroduces a ~5× cost regression AND false cross-symbology survivals.
- rqrr rejects FNC1 symbols (mode 0x5/0x9) — rxing is the GS1 engine; the
  scanner re-derives the grid rqrr detected to read its declarations.
- The goldens (`spec/examples/`) carry this build's `versions`, and
  `spec_golden` pins them: a marker or version bump regenerates the whole
  set from its committed fixtures (`qrscan <fixture> --profile full
  --budget-ms 0`, the table in `spec/README.md` § Examples); review every
  diff.
- `cargo test --workspace --lib` style commands: prefer
  `cargo nextest run`; doc tests via `cargo test --doc`.

## Style

Edition 2024, MSRV 1.88. Comments explain WHY/constraints, not what the
next line does. Doc comments on every public item (`missing_docs` warns).
Conventional commits, lowercase descriptions.

## Consumers — don't break them

- qrcode-ai.com landing (Nuxt): verify flow + import flow consume the
  wasm package — integration pattern in `docs/guides/nuxt-verify.mdx`.
- Future: Nika vision workflows consume the core crate.
