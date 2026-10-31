# Third-party source notices

The scanner is licensed under AGPL-3.0-or-later. The isolated DataBar Expanded
reader in `src/engine/databar_expanded.rs` and its four modules derives from
rxing 0.9.3 (`rxing-core/rxing`), a Rust port of ZXing. Those files retain their
original Apache-2.0 copyright and license notices. The Apache license is included
as `LICENSE-APACHE-2.0`.

The upstream repository is <https://github.com/rxing-core/rxing>.
The versioned published crate source is available at
<https://docs.rs/crate/rxing/0.9.3/source/src/oned/rss/expanded/>.

Local modifications recover finder candidates rejected by the upstream spacing
test while preserving checksum, complete finder-sequence validation and the
existing acceptance checks. The positive BWIPP vectors and illegal finder
sequence regression are retained in the scanner tests. Remove this local copy
when an upstream release passes both sets of vectors through the scanner's
adapter; do not remove the acceptance guards merely to improve recall.

This notice applies to the copied reader source. It does not change the license
of the scanner or infer licensing terms for a consuming application.
