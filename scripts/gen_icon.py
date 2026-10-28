#!/usr/bin/env python3
"""Generate the original OxyPixel icon (16 and 32 px) as ICO and raw RGBA.

The 32px image inside installer/oxypixel.ico is also written as the hicolor
PNG the Linux .deb installs, and the 16px art scaled 16 times as the 256px
PNG the AppImage and Flatpak use. `python3 scripts/gen_icon.py --check`
compares both PNGs to the art and does not write anything.
"""

import struct
import sys
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ART = [
    "DDDDDDDDDDDDDDDD",
    "D..............D",
    "D.AAA......TT..D",
    "D.AAA......TT..D",
    "D.AAA.III..TT..D",
    "D.....III......D",
    "D.....III......D",
    "D..RRRR........D",
    "D..RRRR........D",
    "D..RRRR..AAAA..D",
    "D........AAAA..D",
    "D..TTTT..AAAA..D",
    "D..TTTT........D",
    "D..TTTT........D",
    "D..............D",
    "DDDDDDDDDDDDDDDD",
]
COLORS = {
    "D": (18, 19, 26, 255),
    ".": (28, 31, 40, 255),
    "A": (232, 168, 56, 255),
    "T": (58, 161, 122, 255),
    "I": (244, 241, 234, 255),
    "R": (179, 58, 58, 255),
}


def pixels16():
    rows = []
    for line in ART:
        rows.append([COLORS[ch] for ch in line])
    return rows


def scale2(rows):
    out = []
    for row in rows:
        wide = []
        for px in row:
            wide.extend((px, px))
        out.append(wide)
        out.append(list(wide))
    return out


def rgba_bytes(rows):
    data = bytearray()
    for row in rows:
        for r, g, b, a in row:
            data.extend((r, g, b, a))
    return bytes(data)


def dib(rows):
    size = len(rows)
    xor = bytearray()
    for row in reversed(rows):
        for r, g, b, a in row:
            xor.extend((b, g, r, a))
    stride = ((size + 31) // 32) * 4
    mask = bytes(stride * size)
    header = bytearray()
    header += (40).to_bytes(4, "little")
    header += size.to_bytes(4, "little", signed=True)
    header += (size * 2).to_bytes(4, "little", signed=True)
    header += (1).to_bytes(2, "little")
    header += (32).to_bytes(2, "little")
    header += (0).to_bytes(4, "little")
    header += (len(xor) + len(mask)).to_bytes(4, "little")
    header += (0).to_bytes(4, "little")
    header += (0).to_bytes(4, "little")
    header += (0).to_bytes(4, "little")
    header += (0).to_bytes(4, "little")
    return header + xor + mask


def ico(images):
    count = len(images)
    blob = bytearray()
    blob += (0).to_bytes(2, "little")
    blob += (1).to_bytes(2, "little")
    blob += count.to_bytes(2, "little")
    payload = bytearray()
    entries = bytearray()
    offset = 6 + 16 * count
    for size, data in images:
        entries.append(size if size < 256 else 0)
        entries.append(size if size < 256 else 0)
        entries += (0).to_bytes(1, "little")
        entries += (0).to_bytes(1, "little")
        entries += (1).to_bytes(2, "little")
        entries += (32).to_bytes(2, "little")
        entries += len(data).to_bytes(4, "little")
        entries += offset.to_bytes(4, "little")
        payload += data
        offset += len(data)
    return bytes(blob + entries + payload)


def write_png(path, rows):
    width = len(rows[0])
    height = len(rows)

    def chunk(tag, data):
        crc = zlib.crc32(tag + data) & 0xFFFFFFFF
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", crc)

    raw = bytearray()
    for row in rows:
        raw.append(0)
        for pixel in row:
            raw.extend(pixel)
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", ihdr)
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")
    path.write_bytes(png)


def icon_rows_32(path):
    data = path.read_bytes()
    if data[:4] != b"\x00\x00\x01\x00":
        raise SystemExit(f"{path} is not an ICO")
    count = int.from_bytes(data[4:6], "little")
    for index in range(count):
        base = 6 + 16 * index
        width = data[base] or 256
        height = data[base + 1] or 256
        nbytes = int.from_bytes(data[base + 8 : base + 12], "little")
        offset = int.from_bytes(data[base + 12 : base + 16], "little")
        if width == 32 and height == 32:
            return rows_from_dib(data[offset : offset + nbytes])
    raise SystemExit(f"{path} has no 32x32 image")


def rows_from_dib(blob):
    if len(blob) < 40:
        raise SystemExit("ICO image is truncated")
    size = int.from_bytes(blob[4:8], "little", signed=True)
    if size <= 0:
        raise SystemExit("ICO image has no pixels")
    xor_len = size * size * 4
    xor = blob[40 : 40 + xor_len]
    if len(xor) != xor_len:
        raise SystemExit("ICO image is truncated")
    rows = []
    for y in range(size):
        start = (size - 1 - y) * size * 4
        row = []
        for x in range(size):
            blue, green, red, alpha = xor[start + x * 4 : start + x * 4 + 4]
            row.append((red, green, blue, alpha))
        rows.append(row)
    return rows


def read_png_rows(path):
    data = path.read_bytes()
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise SystemExit(f"{path} is not a PNG")
    pos = 8
    width = height = None
    idat = bytearray()
    while pos + 8 <= len(data):
        length = int.from_bytes(data[pos : pos + 4], "big")
        tag = data[pos + 4 : pos + 8]
        chunk = data[pos + 8 : pos + 8 + length]
        if len(chunk) != length:
            raise SystemExit(f"{path} is truncated")
        if tag == b"IHDR":
            width, height = struct.unpack(">II", chunk[:8])
        elif tag == b"IDAT":
            idat.extend(chunk)
        elif tag == b"IEND":
            break
        pos += 12 + length
    if width != 32 or height != 32:
        raise SystemExit(f"{path} is {width}x{height}, expected 32x32")
    raw = zlib.decompress(bytes(idat))
    rows = []
    cursor = 0
    stride = width * 4
    for _ in range(height):
        if raw[cursor] != 0:
            raise SystemExit(f"{path} uses a PNG filter this check does not read")
        cursor += 1
        row = []
        for x in range(width):
            row.append(tuple(raw[cursor + x * 4 : cursor + x * 4 + 4]))
        cursor += stride
        rows.append(row)
    return rows


def scale(rows, factor):
    out = []
    for row in rows:
        wide = [px for px in row for _ in range(factor)]
        out.extend(list(wide) for _ in range(factor))
    return out


def large_png():
    return ROOT / "docs" / "linux" / "oxypixel-256.png"


def check_large():
    data = large_png().read_bytes()
    width, height = struct.unpack(">II", data[16:24])
    if (width, height) != (256, 256):
        raise SystemExit(f"{large_png()} is {width}x{height}, expected 256x256")
    pos = 8
    idat = bytearray()
    while pos + 8 <= len(data):
        length = int.from_bytes(data[pos : pos + 4], "big")
        tag = data[pos + 4 : pos + 8]
        if tag == b"IDAT":
            idat.extend(data[pos + 8 : pos + 8 + length])
        pos += 12 + length
    raw = zlib.decompress(bytes(idat))
    expected = scale(pixels16(), 16)
    stride = 256 * 4 + 1
    for y, row in enumerate(expected):
        line = raw[y * stride : (y + 1) * stride]
        if line[0] != 0 or bytes(line[1:]) != rgba_bytes([row]):
            raise SystemExit(f"{large_png()} does not match the icon art")


def hicolor_png():
    return ROOT / "docs" / "linux" / "oxypixel.png"


def installer_ico():
    return ROOT / "installer" / "oxypixel.ico"


def export_hicolor():
    destination = hicolor_png()
    destination.parent.mkdir(parents=True, exist_ok=True)
    write_png(destination, icon_rows_32(installer_ico()))


def check_hicolor():
    icon_rows = icon_rows_32(installer_ico())
    png_rows = read_png_rows(hicolor_png())
    if icon_rows != png_rows:
        raise SystemExit(
            "docs/linux/oxypixel.png does not match the 32px image in installer/oxypixel.ico"
        )


def main():
    small = pixels16()
    large = scale2(small)
    icon = ico([(16, dib(small)), (32, dib(large))])
    targets = [
        installer_ico(),
        ROOT / "crates" / "oxypixel" / "assets" / "oxypixel.ico",
    ]
    for path in targets:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(icon)
    (ROOT / "crates" / "oxypixel" / "assets" / "icon32.rgba").write_bytes(rgba_bytes(large))
    export_hicolor()
    write_png(large_png(), scale(small, 16))
    check_hicolor()
    check_large()
    print(f"wrote {len(icon)} byte icon")


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "--check":
        check_hicolor()
        check_large()
        print("hicolor icons match the icon art")
    else:
        main()
