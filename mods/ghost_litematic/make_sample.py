#!/usr/bin/env python3
"""Write a tiny valid .litematic fixture: one 3x2x3 region with a stone/dirt pattern.

Usage: python3 make_sample.py [output.litematic]
The output is the real format: gzipped, named-root NBT with a block-state palette.
"""
import gzip
import struct
import sys
from pathlib import Path

END, BYTE, INT, STRING, LIST, COMPOUND, INT_ARRAY, LONG_ARRAY = 0, 1, 3, 8, 9, 10, 11, 12


def name(text: str) -> bytes:
    raw = text.encode("utf-8")
    return struct.pack(">H", len(raw)) + raw


def field(tag: int, key: str, payload: bytes) -> bytes:
    return bytes([tag]) + name(key) + payload


def string(value: str) -> bytes:
    raw = value.encode("utf-8")
    return struct.pack(">H", len(raw)) + raw


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


if __name__ == "__main__":
    main()
