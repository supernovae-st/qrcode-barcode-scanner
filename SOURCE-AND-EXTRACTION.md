# Scanner source and package identity

**QR Code & Barcode Scanner** is one Rust engine for QR codes, barcodes and
other supported 2D formats. Its runtime bindings share the versioned scan-report
contract. QR and barcode use cases do not require separate engine repositories.

The source repository is
[`qrcode-ai-com/qrcode-barcode-scanner`](https://github.com/qrcode-ai-com/qrcode-barcode-scanner).
The repository retains its identity, commits and tags across renames. Existing
crates, npm packages, Python packages, commands and OCI images keep their
`qrcode-ai-scanner` names. Use the coordinates in [README.md](README.md) and the
[distribution guide](distribution/README.md), not the repository name, to install.

## Development and delivery

1. Implement and test scanner changes in this repository.
2. Select an exact source revision and build the affected runtime artifacts.
3. Qualify the actual packages in clean consumers, including platform-specific
   requirements. A source tag or workspace test alone does not qualify a binding.
4. Publish through [RELEASING.md](RELEASING.md), then verify each artifact's
   version, source revision, installation and supported-platform evidence.

Preserve exact content, report/schema compatibility, advanced QR recovery,
scoring semantics and resource contracts. A repository rename does not release
or retire a package, rename its registry entry, or change its license. Published
artifact hashes, source snapshots, schema identifiers and dated verification
receipts retain their original identities.

QR Code AI is the application consuming this scanner; its creation engine has
its own source and release process. A mobile scanner binding is a developer
artifact, not an application-store release. See [LICENSING.md](LICENSING.md) for
this scanner's licensing terms.

The filename is retained for existing links. Earlier two-engine extraction
proposals are superseded; no extraction or alias/wrapper package is required.
Private operating plans and account data do not belong in this repository or
its distributable archives.
