#!/usr/bin/env python3
"""Generate the hostile-input corpus for crates/umber-mesh/tests/hostile_corpus.rs.

Every file emitted here is HOSTILE: each one must produce a typed
`ImportError` (never a panic, never a silent Ok) through every importer
entry point in fbx.rs / gltf.rs / usdc.rs / usd.rs.

Regenerate after editing:
    python3 crates/umber-mesh/tests/corpus/gen.py

Deterministic: no randomness, no timestamps, byte-for-byte reproducible.
"""

import base64
import os
import struct
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))


def write(rel: str, data: bytes) -> None:
    path = os.path.join(HERE, rel)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as fh:
        fh.write(data)
    print(f"  {rel} ({len(data)} bytes)")


# ---------------------------------------------------------------- usda (text)

QUAD = """#usda 1.0
(
    defaultPrim = "Mesh"
)

def Mesh "Quad"
{
    int[] faceVertexCounts = [4]
    int[] faceVertexIndices = [0, 1, 2, 3]
    point3f[] points = [(0, 0, 0), (1, 0, 0), (1, 1, 0), (0, 1, 0)]
}
"""


def usda(name: str, text: str) -> None:
    write(f"usda/{name}", text.encode("utf-8"))


def mesh_body(body: str) -> str:
    return f'#usda 1.0\ndef Mesh "M"\n{{\n{body}\n}}\n'


POINTS4 = "point3f[] points = [(0, 0, 0), (1, 0, 0), (0, 1, 0), (1, 1, 0)]"

usda("truncated_header.usda", "#usd")
usda("no_header.usda", 'def Mesh "M"\n{\n}\n')
usda(
    "counts_gt_indices.usda",
    mesh_body("int[] faceVertexCounts = [4]\nint[] faceVertexIndices = [0, 1, 2]\n" + POINTS4),
)
usda(
    "zero_count.usda",
    mesh_body("int[] faceVertexCounts = [0]\nint[] faceVertexIndices = []\n" + POINTS4),
)
usda(
    "negative_count.usda",
    mesh_body("int[] faceVertexCounts = [-5]\nint[] faceVertexIndices = []\n" + POINTS4),
)
usda(
    "huge_count.usda",
    mesh_body(
        "int[] faceVertexCounts = [9000000000000000000, 9000000000000000000, 9000000000000000000]\n"
        "int[] faceVertexIndices = [0, 1, 2]\n" + POINTS4
    ),
)
usda(
    "nan_token_points.usda",
    mesh_body(
        "int[] faceVertexCounts = [3]\nint[] faceVertexIndices = [0, 1, 2]\n"
        "point3f[] points = [(nan, 0, 0), (1, 0, 0), (0, 1, 0)]"
    ),
)
usda(
    "inf_points.usda",
    mesh_body(
        "int[] faceVertexCounts = [3]\nint[] faceVertexIndices = [0, 1, 2]\n"
        "point3f[] points = [(1e999, 0, 0), (1, 0, 0), (0, 1, 0)]"
    ),
)
usda(
    "out_of_range_index.usda",
    mesh_body("int[] faceVertexCounts = [3]\nint[] faceVertexIndices = [0, 1, 9]\n" + POINTS4),
)
usda(
    "negative_index.usda",
    mesh_body("int[] faceVertexCounts = [3]\nint[] faceVertexIndices = [-1, 0, 1]\n" + POINTS4),
)
cut = QUAD.find("(1, 0, 0), (1, 1")
usda("truncated_mid_array.usda", QUAD[: cut + 5])
usda("deep_nesting.usda", '#usda 1.0\ndef Mesh "M"\n{\n    int[] faceVertexCounts = [' + "[" * 300)
usda("unterminated_string.usda", '#usda 1.0\ndef Mesh "M"\n{\n    custom string doc = "never closed\n}\n')
write("usda/non_utf8.usda", b"#usda 1.0\n\xff\xfe\x00\x01garbage")

# ---------------------------------------------------------------- gltf / glb


def f32(*vals: float) -> bytes:
    return b"".join(struct.pack("<f", v) for v in vals)


def u16(*vals: int) -> bytes:
    return b"".join(struct.pack("<H", v) for v in vals)


def gltf_doc(buffer: bytes, buffer_views: str, accessors: str, extra_mesh: str = "") -> str:
    uri = base64.b64encode(buffer).decode("ascii")
    return (
        "{\n"
        '  "asset": {"version": "2.0"},\n'
        '  "scene": 0,\n'
        '  "scenes": [{"nodes": [0]}],\n'
        '  "nodes": [{"mesh": 0}],\n'
        '  "meshes": [{"primitives": [{"attributes": {"POSITION": 0}, "indices": 1' + extra_mesh + "}]}],\n"
        '  "buffers": [{"byteLength": ' + str(len(buffer)) + ', "uri": "data:application/octet-stream;base64,' + uri + '"}],\n'
        '  "bufferViews": [' + buffer_views + '],\n'
        '  "accessors": [' + accessors + "]\n"
        "}"
    )


TRI_BUF = f32(0, 0, 0, 1, 0, 0, 0, 1, 0) + u16(0, 1, 2)  # 36 + 6 = 42 bytes
TRI_VIEWS = '{"buffer": 0, "byteOffset": 0, "byteLength": 36}, {"buffer": 0, "byteOffset": 36, "byteLength": 6}'
TRI_ACC = (
    '{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3", "min": [0,0,0], "max": [1,1,0]}, '
    '{"bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR"}'
)

write("gltf/truncated_header.glb", b"glT")
write("gltf/bad_magic.glb", b"XXXX" + b"\x00" * 16)


def glb(json_text: str, bin_payload: bytes, declared_total: int | None = None) -> bytes:
    json_bytes = json_text.encode("utf-8")
    json_len = (len(json_bytes) + 3) // 4 * 4
    total = 12 + 8 + json_len + 8 + len(bin_payload)
    declared = declared_total if declared_total is not None else total
    out = b"glTF" + struct.pack("<II", 2, declared)
    out += struct.pack("<I", json_len) + b"JSON" + json_bytes + b" " * (json_len - len(json_bytes))
    out += struct.pack("<I", len(bin_payload)) + b"BIN\x00" + bin_payload
    return out


# Header declares 1 MiB total but the BIN chunk never arrives.
doc = gltf_doc(TRI_BUF, TRI_VIEWS, TRI_ACC).replace(
    '"uri": "data:application/octet-stream;base64,' + base64.b64encode(TRI_BUF).decode() + '"', '"byteLength_PLACEHOLDER"'
)
glb_json = (
    '{"asset": {"version": "2.0"}, "scene": 0, "scenes": [{"nodes": [0]}], "nodes": [{"mesh": 0}],'
    ' "meshes": [{"primitives": [{"attributes": {"POSITION": 0}, "indices": 1}]}],'
    ' "buffers": [{"byteLength": 42}],'
    ' "bufferViews": [{"buffer": 0, "byteOffset": 0, "byteLength": 36}, {"buffer": 0, "byteOffset": 36, "byteLength": 6}],'
    ' "accessors": [{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"},'
    ' {"bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR"}]}'
)
write("gltf/length_past_eof.glb", glb(glb_json, TRI_BUF, declared_total=1 << 20))

write(
    "gltf/huge_accessor_count.gltf",
    gltf_doc(
        TRI_BUF,
        TRI_VIEWS,
        '{"bufferView": 0, "componentType": 5126, "count": 300000000, "type": "VEC3"}, '
        '{"bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR"}',
    ).encode(),
)
write(
    "gltf/zero_count.gltf",
    gltf_doc(
        TRI_BUF,
        TRI_VIEWS,
        '{"bufferView": 0, "componentType": 5126, "count": 0, "type": "VEC3"}, '
        '{"bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR"}',
    ).encode(),
)
oor_buf = f32(0, 0, 0, 1, 0, 0, 0, 1, 0) + u16(0, 1, 999)
write(
    "gltf/out_of_range_index.gltf",
    gltf_doc(
        oor_buf,
        TRI_VIEWS,
        '{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"}, '
        '{"bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR"}',
    ).encode(),
)
nan_buf = f32(float("nan"), 0, 0, 1, 0, 0, 0, 1, 0) + u16(0, 1, 2)
write(
    "gltf/nan_positions.gltf",
    gltf_doc(
        nan_buf,
        TRI_VIEWS,
        '{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"}, '
        '{"bufferView": 1, "componentType": 5123, "count": 3, "type": "SCALAR"}',
    ).encode(),
)
write(
    "gltf/bufferview_past_eof.gltf",
    gltf_doc(
        TRI_BUF,
        '{"buffer": 0, "byteOffset": 10000, "byteLength": 36}, {"buffer": 0, "byteOffset": 36, "byteLength": 6}',
        TRI_ACC,
    ).encode(),
)

# ---------------------------------------------------------------- fbx

write("fbx/truncated_header.fbx", b"Kaydara FB")
garbage = bytes(((i * 2654435761 + 97) >> 13) & 0xFF for i in range(256))
write("fbx/garbage.fbx", garbage)
write(
    "fbx/ascii_truncated.fbx",
    b"; FBX 7.4.0 project file\nFBXHeaderExtension:  {\n    FBXHeaderVersion: 1003\n    FBXVersion: 7400\n",
)

# ---------------------------------------------------------------- usdc

write("usdc/bad_magic.usdc", b"garbage that is not a crate")
write("usdc/truncated_magic.usdc", b"PXR-USD")
write("usdc/magic_plus_garbage.usdc", b"PXR-USDC" + b"\x00" * 32)

# ---------------------------------------------------------------- usdz (zip)


def local_header(name: bytes, method: int, crc: int, comp: int, uncomp: int, extra: bytes = b"") -> bytes:
    return (
        b"PK\x03\x04"
        + struct.pack("<HHHHH", 20, 0, method, 0, 0)
        + struct.pack("<III", crc, comp, uncomp)
        + struct.pack("<HH", len(name), len(extra))
        + name
        + extra
    )


def central_entry(
    name: bytes, method: int, crc: int, comp: int, uncomp: int, offset: int, extra: bytes = b""
) -> bytes:
    return (
        b"PK\x01\x02"
        + struct.pack("<HHHHHH", 20, 20, 0, method, 0, 0)
        + struct.pack("<III", crc, comp, uncomp)
        + struct.pack("<HHHHH", len(name), len(extra), 0, 0, 0)
        + struct.pack("<II", 0, offset)
        + name
        + extra
    )


def eocd(entries: int, cd_size: int, cd_offset: int) -> bytes:
    return b"PK\x05\x06" + struct.pack("<HHHHIIH", 0, 0, entries, entries, cd_size, cd_offset, 0)


def zip_bytes(entries: list[tuple[bytes, bytes, int, int, int, bytes]], payloads: list[bytes]) -> bytes:
    """entries: (name, local_extra, method, comp, uncomp, cd_extra); payloads: local payload bytes."""
    out = b""
    centrals = b""
    for (name, lextra, method, comp, uncomp, cextra), payload in zip(entries, payloads):
        crc = zlib.crc32(payload) & 0xFFFFFFFF
        offset = len(out)
        out += local_header(name, method, crc, comp, uncomp, lextra) + payload
        centrals += central_entry(name, method, crc, comp, uncomp, offset, cextra)
    cd_offset = len(out)
    out += centrals + eocd(len(entries), len(centrals), cd_offset)
    return out


NAME = b"root.usda"
SMALL = b"#usda 1.0\n"

# Bomb 1: stored entry, central directory declares 0xFFFFFFFF (~4 GiB) uncompressed,
# actual payload 16 bytes.
write(
    "usdz/bomb_stored_declared_4g.usdz",
    zip_bytes(
        [(NAME, b"", 0, 16, 0xFFFFFFFF, b"")],
        [b"x" * 16],
    ),
)

# Bomb 2: deflate entry declaring 0xFFFFFFFE uncompressed with a tiny real payload.
comp = zlib.compress(b"x" * 16)
write(
    "usdz/bomb_deflate_declared_4g.usdz",
    zip_bytes(
        [(NAME, b"", 8, len(comp), 0xFFFFFFFE, b"")],
        [comp],
    ),
)

# Bomb 3: ZIP64 — size fields are 0xFFFFFFFF and the 0x0001 extra field carries 10 GiB.
zip64_extra = struct.pack("<HH", 0x0001, 28) + struct.pack("<QQQI", 10 << 30, len(comp), 0, 0)
write(
    "usdz/bomb_zip64_declared_10g.usdz",
    zip_bytes(
        [(NAME, zip64_extra, 8, 0xFFFFFFFF, 0xFFFFFFFF, zip64_extra)],
        [comp],
    ),
)

write("usdz/truncated_zip.usdz", b"PK\x03\x04" + b"\x00" * 6)
write("usdz/not_a_zip.usdz", b"this is not a zip archive at all, just text")

# Small, well-formed package whose root layer is not a USD layer at all.
write(
    "usdz/valid_zip_garbage_layer.usdz",
    zip_bytes([(NAME, b"", 8, len(zlib.compress(b"garbage, not usda")), len(b"garbage, not usda"), b"")], [zlib.compress(b"garbage, not usda")]),
)


# ------------------------------------------------- kill 2: fuzz battery
# Captured mutants from the 200k-iteration hostile_fuzz battery
# (2026-11-05, seed 0x156A_0001_2026_1105, UMBER_FUZZ_ITERS=200000).
# Every one panicked gltf::load_gltf; each is embedded verbatim so
# regeneration is byte-for-byte reproducible.
FUZZ_BATTERY_MUTANTS = {
    # iter 4513 (seed zero_count.gltf) -> panic at gltf-json mesh.rs:151
    "gltf/oob_position_index_iter4513.gltf": "ewogICJhc3NldCI6IHsidmVyc2lvbiI6ICIyLjAifSwKICAic2NlbmUiOiAwLAogICJzY2VuZXMiOiBbeyJub2RlcyI6IFswXX1dLAogICJub2RlcyI6IFt7Im1lc2giOiAwfV0sCiAgIm1lc2hlcyI6IFt7InByaW1pdGl2ZXMiOiBbeyJhdHRyaWJ1dGVzIjogeyJQT1NJVElPTiI6IDB9LCAiaW5kaWNlcyI6IDF9XX1dLAogICJidWZmZXJzIjogW3siYnl0ZUxlbmd0aCI6IDQyLCAidXJpIjogImRhdGE6YXBwbGljYXRpb24vb2N0ZXQtc3RyZWFtO2Jhc2U2NCxBQUFBQUFBQUFBQUFBQUFBQUFDQVB3QUFBQUFBQUFBQUFBQUFBQUFBZ0Q4QUFBQUFBQUFCQUFJQSJ9XSwKICAiYnVmZmVyVmlld3MiOiBbeyJidWZmZXIiOiAwLCAiYnl0ZU9mZnNldCI6IDAsICJieXRlTGVuZ3RoIjogMzZ9LCB7ImJ1ZmZlciI6IDAsICJieXRlT2Zmc2V0IjogMzYsICJieXRlTGVuZ3RoIjogNn1dLAogICJhY2pjZXNzb3JzIjogW3siYnVmZmVyVmlldyI6IDAsICJjb21wb25lbnRUeXBlIjogNTEyNiwgImNvdW50IjogMCwgInR5cGUiOiAiVkVDMyJ9LCB7ImJ1ZmZlclZpZXciOiAxLCAiY29tcG9uZW50VHlwZSI6IDUxMjMsICJjb3VudCI6IDMsICJ0eXBlIjogIlNDQUxBUiJ9XQp9",
    # iter 28744 (seed nan_positions.gltf) -> panic at gltf-json mesh.rs:151
    "gltf/oob_position_index_iter28744.gltf": "ewogICJhc3NldCI6IHsidmVyc2lvbiI6ICIyLjAifSwKICAic2NlbmUiOiAwLAogICJzY2VuZXMiOiBbeyJub2RlcyI6IFswXX1dLAogICJub2RlcyI6IFt7Im1lc2giOiAwfV0sCiAgIm1lc2hlcyI6IFt7InByaW1pdGl2ZXMiOiBbeyJhdHRyaWJ1dGVzIjogeyJQT1NJVElPTiI6IDB9LCAiaW5kaWNlcyI6IDF9XX1dLAogICJidWZmZXJzIjogW3siYnl0ZUxlbmd0aCI6IDQyLCAidXJpIjogImRhdGE6YXBwbGljYXRpb24vb2N0ZXQtc3RyZWFtO2Jhc2U2NCxBQURBZndBQUFBQUFBQUFBQUFDQVB3QUFBQUFBQUFBQUFBQUFBQUFBZ0Q4QUFBQUFBQUFCQUFJQSJ9XSwKICAiYnVmZmVyVmlld3MiOiBbeyJidWZmZXIiOiAwLCAiYnl0ZU9mZnNldCI6IDAsICJieXRlTGVuZ3RoIjogMzZ9LCB7ImJ1ZmZlciI6IDAsICJieXRlT2Zmc2V0IjogMzYsICJieXRlTGVuZ3RoIjogNn1dLAogICJhY2M1c3NvcnMiOiBbeyJidWZmZXJWaWV3IjogMCwgImNvbXBvbmVudFR5cGUiOiA1MTI2LCAiY291bnQiOiAzLCAidHlwZSI6ICJWRUMzIn0sIHsiYnVmZmVyVmlldyI6IDEsICJjb21wb25lbnRUeXBlIjogNTEyMywgImNvdW50IjogMywgInR5cGUiOiAiU0NBTEFSIn1dCn0=",
    # iter 61695 (seed huge_accessor_count.gltf) -> panic at gltf-json mesh.rs:151
    "gltf/oob_position_index_iter61695.gltf": "ewogICJhc3NldCI6IHsidmVyc2lvbiI6ICIyLjAifSwKICAic2NlbmUiOiAwLAogICJzY2VuZXMiOiBbeyJub2RlcyI6IFswXX1dLAogICJub2RlcyI6IFt7Im1lc2giOiAwfV0sCiAgIm1lc2hlcyI6IFt7InByaW1pdGl2ZXMiOiBbeyJhdHRyaWJ1dGVzIjogeyJQT1NJVElPTiI6IDB9LCAiaW5kaWNlcyI6IDF9XX1dLAogICJidWZmZXJzIjogW3siYnl0ZUxlbmd0aCI6IDQyLCAidXJpIjogImRhdGE6YXBwbGljYXRpb24vb2N0ZXQtc3RyZWFtO2Jhc2U2NCxBQUFBQUFBQUFBQUFBQUFBQUFDQVB3QUFBQUFBQUFBQUFBQUFBQUFBZ0Q4QUFBQUFBQUFCQUFJQSJ9XSwKICAiYnVmZmVyVmlld3MiOiBbeyJidWZmZXIiOiAwLCAiYnl0ZU9mZnNldCI6IDAsICJieXRlTGVuZ3RoIjogMzZ9LCB7ImJ1ZmZlciI6IDAsICJieXRlT2Zmc2V0IjogMzYsICJieXRlTGVuZ3RoIjogNn1dLAogICJhY2Nlc3NvfXMiOiBbeyJidWZmZXJWaWV3IjogMCwgImNvbXBvbmVudFR5cGUiOiA1MTI2LCAiY291bnQiOiAzMDAwMDAwMDAsICJ0eXBlIjogIlZFQzMifSwgeyJidWZmZXJWaWV3IjogMSwgImNvbXBvbmVudFR5cGUiOiA1MTIzLCAiY291bnQiOiAzLCAidHlwZSI6ICJTQ0FMQVIifV0KfQ==",
    # iter 66817 (seed zero_count.gltf) -> panic at gltf-json mesh.rs:151
    "gltf/oob_position_index_iter66817.gltf": "ewogICJhc3NldCI6IHsidmVyc2lvbiI6ICIyLjAifSwKICAic2NlbmUiOiAwLAogICJzY2VuZXMiOiBbeyJub2RlcyI6IFswXX1dLAogICJub2RlcyI6IFt7Im1lc2giOiAwfV0sCiAgIm1lc2hlcyI6IFt7InByaW1pdGl2ZXMiOiBbeyJhdHRyaWJ1dGVzIjogeyJQT1NJVElPTiI6IDB9LCAiaW5kaWNlcyI6IDF9XX1dLAogICJidWZmZXJzIjogW3siYnl0ZUxlbmd0aCI6IDQyLCAidXJpIjogImRhdGE6YXBwbGljYXRpb24vb2N0ZXQtc3RyZWFtO2Jhc2U2NCxBQUFBQUFBQUFBQUFBQUFBQUFDQVB3QUFBQUFBQUFBQUFBQUFBQUFBZ0Q4QUFBQUFBQUFCQUFJQSJ9XSwKICAiYnVmZmVyVmlld3MiOiBbeyJidWZmZXIiOiAwLCAiYnl0ZU9mZnNldCI6IDAsICJieXRlTGVuZ3RoIjogMzZ9LCB7ImJ1ZmZlciI6IDAsICJieXRlT2Zmc2V0IjogMzYsICJieXRlTGVuZ3RoIjogNn1dLAogICJhY2Nlc3NvQnMiOiBbeyJidWZmZXJWaWV3IjogMCwgImNvbXBvbmVudFR5cGUiOiA1MTI2LCAiY291bnQiOiAwLCAidHlwZSI6ICJWRUMzIn0sIHsiYnVmZmVyVmlldyI6IDEsICJjb21wb25lbnRUeXBlIjogNTEyMywgImNvdW50IjogMywgInR5cGUiOiAiU0NBTEFSIn1dCn0=",
    # iter 76816 (seed huge_accessor_count.gltf) -> panic at gltf-json mesh.rs:151
    "gltf/oob_position_index_iter76816.gltf": "ewogICJhc3NldCI6IHsidmVyc2lvbiI6ICIyLjAifSwKICAic2NlbmUiOiAwLAogICJzY2VuZXMiOiBbeyJub2RlcyI6IFswXX1dLAogICJub2RlcyI6IFt7Im1lc2giOiAwfV0sCiAgIm1lc2hlcyI6IFt7InByaW1pdGl2ZXMiOiBbeyJhdHRyaWJ1dGVzIjogeyJQT1NJVElPTiI6IDB9LCAiaW5kaWNlcyI6IDF9XX1dLAogICJidWZmZXJzIjogW3siYnl0ZUxlbmd0aCI6IDQyLCAidXJpIjogImRhdGE6YXBwbGljYXRpb24vb2N0ZXQtc3RyZWFtO2Jhc2U2NCxBQUFBQUFBQUFBQUFBQUFBQUFDQVB3QUFBQUFBQUFBQUFBQUFBQUFBZ0Q4QUFBQUFBQUFCQUFJQSJ9XSwKICAiYnVmZmVyVmlld3MiOiBbeyJidWZmZXIiOiAwLCAiYnl0ZU9mZnNldCI6IDAsICJieXRlTGVuZ3RoIjogMzZ9LCB7ImJ1ZmZlciI6IDAsICJieXRlT2Zmc2V0IjogMzYsICJieXRlTGVuZ3RoIjogNn1dLAogICJhY2Nlc3NvYHMiOiBbeyJidWZmZXJWaWV3IjogMCwgImNvbXBvbmVudFR5cGUiOiA1MTI2LCAiY291bnQiOiAzMDAwMDAwMDAsICJ0eXBlIjogIlZFQzMifSwgeyJidWZmZXJWaWV3IjogMSwgImNvbXBvbmVudFR5cGUiOiA1MTIzLCAiY291bnQiOiAzLCAidHlwZSI6ICJTQ0FMQVIifV0KfQ==",
    # iter 77505 (seed zero_count.gltf) -> panic at gltf-json mesh.rs:151
    "gltf/oob_position_index_iter77505.gltf": "ewogICJhc3NldCI6IHsidmVyc2lvbiI6ICIyLjAifSwKICAic2NlbmUiOiAwLAogICJzY2VuZXMiOiBbeyJub2RlcyI6IFswXX1dLAogICJub2RlcyI6IFt7Im1lc2giOiAwfV0sCiAgIm1lc2hlcyI6IFt7InByaW1pdGl2ZXMiOiBbeyJhdHRyaWJ1dGVzIjogeyJQT1NJVElPTiI6IDB9LCAiaW5kaWNlcyI6IDF9XX1dLAogICJidWZmZXJzIjogW3siYnl0ZUxlbmd0aCI6IDQyLCAidXJpIjogImRhdGE6YXBwbGljYXRpb24vb2N0ZXQtc3RyZWFtO2Jhc2U2NCxBQUFBQUFBQUFBQUFBQUFBQUFDQVB3QUFBQUFBQUFBQUFBQUFBQUFBZ0Q4QUFBQUFBQUFCQUFJQSJ9XSwKICAiYnVmZmVyVmlld3MiOiBbeyJidWZmZXIiOiAwLCAiYnl0ZU9mZnNldCI6IDAsICJieXRlTGVuZ3RoIjogMzZ9LCB7ImJ1ZmZlciI6IDAsICJieXRlT2Zmc2V0IjogMzYsICJieXRlTGVuZ3RoIjogNn1dLAogICJhY2Nlc3NvI3MiOiBbeyJidWZmZXJWaWV3IjogMCwgImNvbXBvbmVudFR5cGUiOiA1MTI2LCAiY291bnQiOiAwLCAidHlwZSI6ICJWRUMzIn0sIHsiYnVmZmVyVmlldyI6IDEsICJjb21wb25lbnRUeXBlIjogNTEyMywgImNvdW50IjogMywgInR5cGUiOiAiU0NBTEFSIn1dCn0=",
    # iter 104716 (seed bufferview_past_eof.gltf) -> panic at gltf-json mesh.rs:151
    "gltf/oob_position_index_iter104716.gltf": "ewogICJhc3NldCI6IHsidmVyc2lvbiI6ICIyLjAifSwKICAic2NlbmUiOiAwLAogICJzY2VuZXMiOiBbeyJub2RlcyI6IFswXX1dLAogICJub2RlcyI6IFt7Im1lc2giOiAwfV0sCiAgIm1lc2hlcyI6IFt7InByaW1pdGl2ZXMiOiBbeyJhdHRyaWJ1dGVzIjogeyJQT1NJVElPTiI6IDB9LCAiaW5kaWNlcyI6IDF9XX1dLAogICJidWZmZXJzIjogW3siYnl0ZUxlbmd0aCI6IDQyLCAidXJpIjogImRhdGE6YXBwbGljYXRpb24vb2N0ZXQtc3RyZWFtO2Jhc2U2NCxBQUFBQUFBQUFBQUFBQUFBQUFDQVB3QUFBQUFBQUFBQUFBQUFBQUFBZ0Q4QUFBQUFBQUFCQUFJQSJ9XSwKICAiYnVmZmVyVmlld3MiOiBbeyJidWZmZXIiOiAwLCAiYnl0ZU9mZnNldCI6IDEwMDAwLCAiYnl0ZUxlbmd0aCI6IDM2fSwgeyJidWZmZXIiOiAwLCAiYnl0ZU9mZnNldCI6IDM2LCAiYnl0ZUxlbmd0aCI6IDZ9XSwKICAiYWNjZVZzb3JzIjogW3siYnVmZmVyVmlldyI6IDAsICJjb21wb25lbnRUeXBlIjogNTEyNiwgImNvdW50IjogMywgInR5cGUiOiAiVkVDMyIsICJtaW4iOiBbMCwwLDBdLCAibWF4IjogWzEsMSwwXX0sIHsiYnVmZmVyVmlldyI6IDEsICJjb21wb25lbnRUeXBlIjogNTEyMywgImNvdW50IjogMywgInR5cGUiOiAiU0NBTEFSIn1dCn0=",
    # iter 114302 (seed huge_accessor_count.gltf) -> panic at gltf-json mesh.rs:151
    "gltf/oob_position_index_iter114302.gltf": "ewogICJhc3NldCI6IHsidmVyc2lvbiI6ICIyLjAifSwKICAic2NlbmUiOiAwLAogICJzY2VuZXMiOiBbeyJub2RlcyI6IFswXX1dLAogICJub2RlcyI6IFt7Im1lc2giOiAwfV0sCiAgIm1lc2hlcyI6IFt7InByaW1pdGl2ZXMiOiBbeyJhdHRyaWJ1dGVzIjogeyJQT1NJVElPTiI6IDB9LCAiaW5kaWNlcyI6IDF9XX1dLAogICJidWZmZXJzIjogW3siYnl0ZUxlbmd0aCI6IDQyLCAidXJpIjogImRhdEFBQUFBQUFBQUFBQUFBQUFBQWdEOEFBQUFBQUFBQkFBSUEifV0sCiAgImJ1ZmZlclZpZXdzIjogW3siYnVmZmVyIjogMCwgImJ5dGVPZmZzZXQiOiAwLCAiYnl0ZUxlbmd0aCI6IDM2fSwgeyJidWZmZXIiOiAwLCAiYnl0ZU9mZnNldCI6IDM2LCAiYnl0ZUxlbmd0aCI6IDZ9XSwKICAiPmNjZXNzb3JzIjogW3siYnVmZmVyVmlldyI6IDAsICJjb21wb25lbnRUeXBlIjogNTEyNiwgImNvdW50IjogMzAwMDAwMDAwLCAidHlwZSI6ICJWRUMzIn0sIHsiYnVmZmVyVmlldyI6IDEsICJjb21wb25DbnRUeXBlIjogNTEyMywgImNvdW50IjogMywgInR5cGUiOiAiU0NBTEFSIn1dCn0=",
    # iter 47382 (seed length_past_eof.glb) -> panic at gltf binary.rs:285
    "gltf/glb_len_underflow_iter47382.glb": "Z2xURgIAAAAAAAAAABAA4AEAAEpTT057ImFzc2V0IjogeyJ2ZXJzaW9uIjogIjIuMCJ9LCAic2NlbmUiOiAwLCAic2NlbmVzIjogW3sibm9vbiI6ICIyLjAifSwgInNjZW5lIjogMCwgInNjZW5lcyI6IFt7Im5vZGVzIjogWzBdfV0sICJub2RlcyI6IFt7Im1lc2giOiAwfV0sICJtZXNoZXMiOiBbeyJwcmltaXRpdmVzIjogW3siYXR0cmlidXRlcyI6IHsiUE9TSVRJT04=",
    # iter 104518 (seed length_past_eof.glb) -> panic at gltf binary.rs:285
    "gltf/glb_len_underflow_iter104518.glb": "Z2xURgIAAAAAAAAAAAAQAOABAABKU09OeyJOeyJhc3NldCIzIHsidmVy",
}
for _rel, _b64 in FUZZ_BATTERY_MUTANTS.items():
    write(_rel, base64.b64decode(_b64))
print("corpus generated under", HERE)
