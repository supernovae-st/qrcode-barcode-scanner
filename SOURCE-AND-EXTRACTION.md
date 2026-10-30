# Private source and autonomous scanner extraction

Accepted 30 October 2026. `supernovae-st/qrai-qrcode-barcode-scanner` is the
private development source, previously named `supernovae-st/qrcode-ai-scanner`.
Its GitHub repository identity, history and tags are retained.

| Responsibility | Repository | State |
| --- | --- | --- |
| Scanner development and fixes | `supernovae-st/qrai-qrcode-barcode-scanner` | Private upstream; rename verified |
| QR engine and its publications | `qr-code-scanner` | Source extraction and delivery planned |
| Barcode engine and its publications | `barcode-scanner` | Source extraction and delivery planned |

## Development and delivery

1. Implement and test improvements in the private upstream first.
2. Select an immutable source revision and explicit product extraction recipe.
3. Export the required code, licenses and build inputs into each affected product
   repository. Track upstream revision, recipe revision, output hashes and
   destination revision. Keep private operating plans and credentials out.
4. Build and test each extracted engine independently. A clean consumer must
   install and run it without the private upstream checkout, its credentials,
   or the other product's engine. Local shared path dependencies do not prove this.
5. Publish and verify each product independently, with its own artifact identity,
   version, supported platform evidence and public destinations.

A product-side correction is integrated upstream before the next extraction.
Do not maintain divergent fixes in generated downstream code. A shared origin
does not imply a shared runtime package or a Barcode dependency on the QR engine.
The exact capabilities of each extracted build need their own acceptance; the
repository split alone does not change supported symbologies or wire semantics.

## Existing source and evidence

The current tree still implements the combined scanner and its bindings.
Extracted repositories, export tooling and separate artifacts are not established
by this document. Retain exact-content, report/schema, scoring, malformed-input
and resource contracts while preparing extraction.

Existing crates, npm scopes, commands, OCI image names, schema identifiers,
versions and licenses are not renamed. Released artifacts and dated receipts
retain the source identity with which they were produced. GitHub source/release
URLs require repository access after privatization. Public registry availability
is a separate observation; never promote a past receipt to current evidence.
