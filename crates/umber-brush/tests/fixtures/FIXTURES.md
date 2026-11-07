# `.abr` test fixtures — provenance and licensing

These three files are **trimmed copies of real Photoshop `.abr` brush
files** from the `ag-psd` upstream test corpus:

- Upstream project: `Agamnentzar/ag-psd` (MIT License, Copyright (c) 2016
  Agamnentzar), file `test/abr-read/`.
- Upstream URLs:
  - `test/abr-read/sample-and-pattern/src.abr` → `abr_real_v6_2.abr`
  - `test/abr-read/special/src.abr` → `abr_real_v10_2.abr`
  - `test/abr-read/simple/src.abr` → `abr_real_v6_2_no_samples.abr`
- License: MIT (the upstream repository's license covers its test data).
  The copyright notice and permission text are reproduced below.

## What "trimmed" means

Each copy keeps the file's 16-bit version header and **only** the
`8BIM` sections whose kind the clean-room reader interprets (`samp`),
dropping `patt` (the pattern blobs, up to 1 MiB), `desc` (the descriptor
tree) and `phry`. The `samp` payload bytes are UNTOUCHED — the trimming
is a section-level filter, not a re-encoding. The upstream expected
values (`data.json` beside each file) are what the pins in
`crates/umber-brush/tests/abr_real_file.rs` assert, and the alpha plane
was additionally checked byte for byte against the upstream oracle PNG
(`sample-<id>.png`, whose alpha channel IS the plane).

| fixture | upstream | version | samples | bytes |
|---|---|---|---|---|
| `abr_real_v6_2.abr` | sample-and-pattern | 6.2 | 1 (112×105, RLE) | 7,828 |
| `abr_real_v10_2.abr` | special | 10.2 | 1 (23×20, RLE) | 740 |
| `abr_real_v6_2_no_samples.abr` | simple | 6.2 | 0 (computed brush only) | 662 |

## The MIT License (upstream)

```
The MIT License (MIT)

Copyright (c) 2016 Agamnentzar

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```
