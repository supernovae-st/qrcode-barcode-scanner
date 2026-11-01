# Contributing

Thanks for considering a contribution! `qrcode-ai-scanner` is one Rust core
(`crates/qrcode-ai-scanner`) with thin façades on top (CLI, Node/napi, browser/WASM,
Python/PyO3, and Kotlin+Swift via UniFFI). Most changes land in the core; the façades
stay thin.

By contributing you agree that your work is **dual-licensed** under both
**AGPL-3.0-or-later AND** SuperNovae Studio's commercial license (inbound = outbound,
dual) — see [`LICENSING.md`](LICENSING.md). This keeps the project free for the
community and sustainable.

## Build & test

Read [Local development checks](RELEASING.md#local-development-checks) for the
exact compiler, locked workspace gates and excluded binding roots. The compiler
pin comes from `RUST_TOOLCHAIN` in `.github/workflows/ci.yml`; the manifest's
`rust-version` is the separately tested minimum. No repository toolchain file
overrides a contributor's default, so use the explicit compiler selector.

A first core check with the current pin is:

```bash
cargo +1.99.1 test -p qrcode-ai-scanner --all-targets --locked
cargo +1.99.1 fmt --all --check
```

This core check does not execute all bindings, doctests or installed consumers.
Before a PR, run the applicable workspace, type, MSRV, Wasm and package gates
from the linked runbook. Source/build/packed-consumer checks have different
scope; CI exercises the selected gates on Linux / macOS / Windows. Apply
formatting to owned files when editing, then verify it with the check above.

## Code standards

- **No `.unwrap()` / `.expect()` in `src/`** — the workspace lints deny them;
  return a typed error instead.
- Keep `clippy` (with `pedantic`) warning-free, and document public items.
- **Deterministic by contract**: same bytes + same config ⇒ the same result.
  Don't introduce hidden RNG or wall-clock-dependent output.

## The wire contract lives in `spec/`

`spec/` is the **normative** `ScanReport` contract (wire format + JSON Schema +
golden examples), validated in CI against the real types. If your change alters
the output shape, update `spec/` and regenerate the golden examples in the same
PR — CI will fail otherwise.

## Pull requests

1. Fork, then branch from `main`.
2. Use [Conventional Commits](https://www.conventionalcommits.org/)
   (`feat:`, `fix:`, `docs:`, …).
3. Add or adjust tests, and update `CHANGELOG.md` under the unreleased section.
4. Open the PR, fill in the template, and make sure CI is green.

Larger ideas? Open a feature-request issue first so we can align on the design.
