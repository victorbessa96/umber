# stencil-core-port-annotation — the binary-era port flagged, byte-intact

2026-10-19, session local-3d2151f3, Bessa's steer ("anything you
can do meanwhile?"). Kill-lock: stencil-core-port-annotate
(docs/loop/kill-reports/2026-10-17-stencil-core.md + this
report).

## Why

The stencil-core kill report embeds a full Python port of the
rasterize path with its mutation matrix — binary-era
expectations (`255 if v >= 127.5 else 0`). Soft edges landed the
same week (35ad8bd): rasterize_gate writes the rounded bilinear
sample, no threshold, and the boundary expectations moved. A
reader re-running the old port against current code hits
boundary mismatches with no explanation in the report they're
standing in.

## The change (additive only)

A dated NOTE after the port block: the port pins the binary-era
shape, soft edges superseded it (the ramp reads 64/128/191, the
matrix is soft), the current-shape port + re-derivation live in
2026-10-19-stencil-soft-edges.md — and the block stays
byte-intact as the original kill's evidence. No history rewritten;
the reader is pointed at the current truth from both ends.

Census: the NOTE present (stencil-core-port-annotation), the
port's threshold line byte-identical, the soft-edges report
cross-referenced from both.
