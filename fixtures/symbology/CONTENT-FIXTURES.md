# Content fixtures — provenance

Synthetic symbols for the exact-content rows of
`crates/qrcode-ai-scanner/tests/exact_content.rs` (spec/01 § Content): non-QR
`raw` (the decoding engine's reading), Micro QR and rMQR Kanji. Every payload
is public test text. They are test inputs, not corpus entries: corpus.toml
does not list them.

Writers: zxing-cpp 3.1.1 (Python binding, `zxingcpp.create_barcode`, then
`to_image` saved as PNG through Pillow) and segno 1.6.6
(`segno.make_micro(text, mode='kanji')`, `save(path, scale=10, border=4)`).
Text written as UTF-8 bytes means `create_barcode(text.encode('utf-8'), …)`.

| File | Writer | Content | Parameters | sha256 |
|---|---|---|---|---|
| `datamatrix-utf8-url-japanese.png` | zxing-cpp, DataMatrix | UTF-8 bytes of `https://qrc-ai.com/日本語テスト` | `to_image(scale=8)` | `2cb024fb858ff68857631e08bc152132f0b9fe61f58971ac146ee7ce7d2a7efe` |
| `datamatrix-utf8-cafe.png` | zxing-cpp, DataMatrix | UTF-8 bytes of `Café crème brûlée` | `to_image(scale=8)` | `32f99b137033977c7559092b22dc425634fe7fd0c842a9db47cc14e1c9e9e7e7` |
| `datamatrix-utf8-url-cafe.png` | zxing-cpp, DataMatrix | UTF-8 bytes of `https://qrc-ai.com/café` | `to_image(scale=8)` | `4186f4f8b4e5db4d168e21100f3ef35a763368c2c27c08c94fe51ed6d4d730b6` |
| `datamatrix-pair-japanese.png` | zxing-cpp, DataMatrix ×2 | UTF-8 bytes of `https://a.example/日本語テスト` and `https://b.example/日本語テスト` | each `to_image(scale=8)`, converted to luma; canvas `w₁ + w₂ + 80` × `max(h) + 40`, white; pasted at (20, 20) and (w₁ + 60, 20) | `82e9db9525008438852995c8ec12bc061b8af4b4a4b4196f15e2e98404879755` |
| `pdf417-utf8-cafe.png` | zxing-cpp, PDF417 | UTF-8 bytes of `Café crème brûlée` | `to_image(scale=4)` | `3c3bf3c004abc7e2028ea3b18a22499c06cf0c4f0b35e04048cade8990e447c4` |
| `databar-gtin.png` | zxing-cpp, DataBar | `09506000134352` | `to_image(scale=4, add_quiet_zones=True)` | `026567259bfd62ffeccad16af1db7baff651436a4e6746ce795f8ad8a0614157` |
| `rmqr-url-kanji.png` | zxing-cpp, RMQRCode | `https://qrc-ai.com/日本` (the writer encodes 日本 in Kanji mode, no ECI) | `to_image(scale=6, add_quiet_zones=True)` | `97bcf495768b09cbcdd870deeac2497c03d7d4bebc90adfba81b5e667797995b` |
| `microqr-kanji-nihongo.png` | zxing-cpp, MicroQRCode | `日本語` (Kanji mode) | `to_image(scale=6, add_quiet_zones=True)` | `037e356ac1b9727bcba76ae4efc3c80f4e8b8bd6f2dfc3fe174cb44ab3153a7a` |
| `microqr-kanji-yamada.png` | segno, M3-M | `山田` (Shift JIS 8E 52 93 63) | `make_micro('山田', mode='kanji')` | `88be141cda9d6c27fe5c2044205b79b5261b46adb11a206385530ec5324c08d1` |
| `microqr-kanji-yamamoto.png` | segno, M3-M | `山本` (8E 52 96 7B) | `make_micro('山本', mode='kanji')` | `d6e6d7590f4374a031146bf7ba908c8aa3755639cb19a95a23e64cfe15f70cea` |
| `microqr-kanji-nihon.png` | segno, M3-M | `日本` (93 FA 96 7B) | `make_micro('日本', mode='kanji')` | `0f946999b2d65a63f4d6540dca0bd8d11ac91813cd4e8374cb1456732e8e5cd9` |
| `microqr-kanji-tesuto.png` | segno, M3-M | `テスト` (83 65 83 58 83 67) | `make_micro('テスト', mode='kanji')` | `554ff99dc319d0e793d5d34abab9f8bd30decc4a92d1b5080829e56b3ee131ed` |
| `datamatrix-gs1-fnc1.png` | zxing-cpp, DataMatrix, GS1 (FNC1 in first position; zxing-cpp reads it back as `]d2`) | `(01)09506000134352(10)AB12` | `create_barcode(…, gs1=True)`, `write_barcode_to_image(scale=8)`, 16 px white border, 8-bit greyscale PNG | `11b2ba0fb1b9d4cd7e547055c09d6b8cf2e6c4c771a7d5bb6fea28e62bbe1228` |

`datamatrix-gs1.png` (a corpus fixture) is not GS1 mode: it carries the
element string after a literal GS character, with no FNC1 codeword.

## Occurrence fixtures

Inputs of `crates/qrcode-ai-scanner/tests/occurrences.rs` (spec/01
§ Occurrences): where a reader's geometry lies, or why it publishes none.
Same writer, zxing-cpp 3.1.1: `create_barcode(text, format)`, then
`to_image(scale=s, add_quiet_zones=True)` converted to luma. Two symbols
*beside* each other: canvas `w₁ + w₂ + 80` × `max(h) + 40`, white, pasted at
(20, 20) and (w₁ + 60, 20). *Stacked*: canvas `max(w) + 40` × `h₁ + h₂ + 60`,
pasted at (20, 20) and (20, h₁ + 40). Saved by Pillow with `optimize=True`.

| File | Symbols | Layout | sha256 |
|---|---|---|---|
| `maxicode-over-code128.png` | MaxiCode `MAXI-4827` (scale 2) above Code 128 `STOCK-4827` (scale 2) | stacked, 359 × 478 | `27c8f262a060e2f0b384e2e2523cbc35a99c7d3851afa6237103408a1c82a0d8` |
| `ean13-addon5-code128.png` | EAN-13 `5901234123457` with the add-on `12345` (written as `5901234123457+12345`, scale 3), then Code 128 `STOCK-4827` (scale 3) | beside, 1037 × 205 | `58e6e142bde63671684b931258a1f32618184e27565b99330c995696e4733440` |
| `codabar.png` | Codabar `A40156B` (scale 3; read back as `40156`) | alone, 276 × 150 | `5efff3c933372e70262d054d6ff12d674e50172468ed6d12694805c9c8e0486b` |
| `telepen.png` | Telepen `TELEPEN-4827` (scale 3) | alone, 780 × 150 | `f54a1bb0deb2ae7239bb71df262f9bb7c0bb4f03e13145ee6981bf0c347ccbff` |

Bars measured from the pixels (columns of dark pixels, inclusive): in
`ean13-addon5-code128.png` the main symbol 53–337, its add-on 359–499 and the
Code 128 585–986; in `maxicode-over-code128.png` the MaxiCode rows 30–327 and
the Code 128 rows 358–457.

## Linear and mixed-family regression fixtures

Inputs of `crates/qrcode-ai-scanner/tests/linear_formats.rs`,
`tests/mixed_symbols.rs`, the engine's unit tests and the packed WASM
consumer. Writer: bwip-js 4.11.4, `bwipjs.toBuffer(options)`. Each bwip-js
row was regenerated with those options and matched the committed bytes.

| File | Writer | Content | Parameters | sha256 |
|---|---|---|---|---|
| `code128-stock-4827.png` | bwip-js, `code128` | `STOCK-4827` | `scale: 3, height: 25, padding: 16, backgroundcolor: 'FFFFFF'` (498 × 310) | `a40f82c781ed634c99abbcac1801b7c5601d60819e39ecac537d4abbc0ab3b21` |
| `mixed-code128-qr.png` | the row above and a QR (`https://qrc-ai.com/76xMa`), composed in Chrome | `STOCK-4827` and `https://qrc-ai.com/76xMa` | white canvas `w₁ + w₂ + 144` × `max(h) + 96`; the Code 128 at (48, ⌊(H − h₁) / 2⌋), the QR at (w₁ + 96, ⌊(H − h₂) / 2⌋); `canvas.toDataURL('image/png')`, stored as RGB (1666 × 1120) | `c636e8d56df483ff1ec06dbe9b5bcba226c579f8fc1c3d10c4ccf4e100762ec8` |
| `code39-short-j.png` | bwip-js, `code39` | `J`, no optional check character | `scale: 3, height: 16, padding: 16, backgroundcolor: 'FFFFFF'` (238 × 233) | `190388184b2851d2b93e0187a585836b857387367df5b5bb7a4a34434c2f7340` |
| `code93-bwipp-checked.png` | bwip-js, `code93` | `SCANNER-93` with the C and K check characters | `scale: 4, padding: 12, backgroundcolor: 'FFFFFF', includecheck: true` (604 × 385) | `2aaec1b0c11ab80b3387be94f40386403ce3ddf3917510e83bbcf93a6db9f3b4` |
| `databar-expanded-bwipp-regression.png` | bwip-js, `databarexpanded` | `(01)09506000134352(10)DEMO` | `scale: 4, padding: 12, backgroundcolor: 'FFFFFF'` (892 × 233) | `1a25822c10eb5aaef326cacbc665d0c7b9e9ab7777ad14a99c2c5f8b90d7c08d` |
| `databar-small-regression.png` | zxing-cpp, DataBar: a byte copy of `databar-gtin.png` | `09506000134352` | as `databar-gtin.png` (384 × 200) | `026567259bfd62ffeccad16af1db7baff651436a4e6746ce795f8ad8a0614157` |

The composite's PNG bytes depend on the browser's encoder: a fresh render
gives the same pixels and other bytes, so only this sha256 names the file.
