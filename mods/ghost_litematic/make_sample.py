#!/usr/bin/env python3
"""Write a tiny valid .litematic fixture: one 3x2x3 region with a stone/dirt pattern.

Usage: python3 make_sample.py [output.litematic]
The output is the real format: gzipped, named-root NBT with a block-state palette.
"""
import gzip
import struct
import sys
from pathlib import Path

END, SHORT, INT, STRING, LIST, COMPOUND, BYTE_ARRAY, INT_ARRAY, LONG_ARRAY = (
    0, 2, 3, 8, 9, 10, 7, 11, 12,
)


def name(text: str) -> bytes:
    raw = text.encode("utf-8")
    return struct.pack(">H", len(raw)) + raw


def field(tag: int, key: str, payload: bytes) -> bytes:
    return bytes([tag]) + name(key) + payload


def string(value: str) -> bytes:
    raw = value.encode("utf-8")
    return struct.pack(">H", len(raw)) + raw


def short(value: int) -> bytes:
    return struct.pack(">h", value)


def byte_array(values) -> bytes:
    raw = bytes(values)
    return struct.pack(">i", len(raw)) + raw


def varint(value: int) -> bytes:
    out = bytearray()
    while True:
        byte = value & 0x7F
        value >>= 7
        out.append(byte | (0x80 if value else 0))
        if not value:
            return bytes(out)


def int_array(values) -> bytes:
    return struct.pack(">i", len(values)) + b"".join(struct.pack(">i", v) for v in values)


def long_array(values) -> bytes:
    return struct.pack(">i", len(values)) + b"".join(struct.pack(">q", v) for v in values)


def palette_entry(block: str) -> bytes:
    # A list element's type comes from the list header, so no type byte here.
    return field(STRING, "Name", string(block)) + bytes([END])


def palette_list(entries) -> bytes:
    return bytes([COMPOUND]) + struct.pack(">i", len(entries)) + b"".join(entries)


def main() -> None:
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "fixtures/sample.litematic")
    out.parent.mkdir(parents=True, exist_ok=True)

    # Palette: 0 = air, 1 = stone, 2 = dirt. 2 bits per cell, 18 cells.
    palette = ["minecraft:air", "minecraft:stone", "minecraft:dirt"]
    cells = [0] * 18
    for index in (4, 5, 6, 7, 10, 13, 14, 15, 16):
        cells[index] = 1 if index % 2 else 2
    packed = 0
    for shift, value in enumerate(cells):
        packed |= value << (shift * 2)

    region = b"".join([
        field(INT_ARRAY, "Size", int_array([3, 2, 3])),
        field(INT_ARRAY, "Position", int_array([0, 64, 0])),
        field(LIST, "BlockStatePalette", palette_list([palette_entry(b) for b in palette])),
        field(LONG_ARRAY, "BlockStates", long_array([packed])),
        bytes([END]),
    ])
    root = b"".join([
        field(INT, "Version", struct.pack(">i", 6)),
        field(COMPOUND, "Regions", field(COMPOUND, "sample", region) + bytes([END])),
        bytes([END]),
    ])
    document = bytes([COMPOUND]) + name("Minecraft") + root
    out.write_bytes(gzip.compress(document, mtime=0))
    print(f"wrote {out} ({len(document)} bytes of NBT)")

    # A matching Sponge v3 .schem: same 3x2x3 pattern, varint palette indices.
    palette_compound = b"".join(
        field(INT, state, struct.pack(">i", index)) for state, index in
        zip(["minecraft:air", "minecraft:stone", "minecraft:dirt"], range(3))
    ) + bytes([END])
    blocks = b"".join([
        field(COMPOUND, "Palette", palette_compound),
        field(BYTE_ARRAY, "Data", byte_array(b"".join(varint(v) for v in cells))),
        field(LIST, "BlockEntities", bytes([COMPOUND]) + struct.pack(">i", 0)),
        bytes([END]),
    ])
    schem = b"".join([
        field(INT, "Version", struct.pack(">i", 3)),
        field(SHORT, "Width", short(3)),
        field(SHORT, "Height", short(2)),
        field(SHORT, "Length", short(3)),
        field(INT_ARRAY, "Origin", int_array([0, 64, 0])),
        field(COMPOUND, "Blocks", blocks),
        bytes([END]),
    ])
    schem_document = bytes([COMPOUND]) + name("Schematic") + schem
    schem_path = out.with_suffix(".schem")
    schem_path.write_bytes(gzip.compress(schem_document, mtime=0))
    print(f"wrote {schem_path} ({len(schem_document)} bytes of NBT)")


if __name__ == "__main__":
    main()
