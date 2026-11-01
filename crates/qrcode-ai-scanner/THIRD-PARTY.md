# Third-party source notices

The scanner is licensed under AGPL-3.0-or-later. Two isolated parts of its rxing
adapter derive from rxing 0.9.3 (`rxing-core/rxing`), a Rust port of ZXing, and
retain their original Apache-2.0 copyright and license notices. The Apache
license is included as `LICENSE-APACHE-2.0`.

The upstream repository is <https://github.com/rxing-core/rxing>.
The versioned published crate source is available at
<https://docs.rs/crate/rxing/0.9.3/source/>.

## DataBar Expanded reader

`src/engine/databar_expanded.rs` and its four modules derive from
`src/oned/rss/expanded/`.

Local modifications recover finder candidates rejected by the upstream spacing
test while preserving checksum, complete finder-sequence validation and the
existing acceptance checks. The rows stored by the upright pass are cleared
before the rotated pass, so the reader never assembles a symbol from rows of
two frames. The positive BWIPP vectors and illegal finder sequence regression
are retained in the scanner tests. Remove this local copy when an upstream
release passes both sets of vectors through the scanner's adapter; do not
remove the acceptance guards merely to improve recall.

## Multiple-barcode reader

`src/engine/multi_reader.rs` derives from `src/multi/generic_multiple_barcode_reader.rs`
and `src/multi_use_multi_format_reader.rs`. It keeps the upstream recursion
around each symbol found, with five local modifications documented at the top
of the file:

1. one fixed reader dispatch order, so a restricted pass reads, and crops,
   the same way on every run;
2. every read's points normalized into the image's frame, and points no
   reader measured in that image dropped, never guessed;
3. a read without points no longer ends its node: the node is read again
   past that read's format;
4. a fresh reader state for each read, so a stateful reader never assembles
   a symbol from another crop's rows;
5. duplicates decided by kind of symbol and by pixels, not by point count:
   two equal linear reads are one only when the image proves them the same
   physical symbol, so equal printed copies are no longer collapsed.

Remove this local copy when an upstream release preserves equal linear copies
and stable reads through the scanner's adapter and its occurrence tests.

This notice applies to the copied reader source. It does not change the license
of the scanner or infer licensing terms for a consuming application.
