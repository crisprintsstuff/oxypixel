#!/usr/bin/env python3
"""Generate the original OxyPixel icon (16 and 32 px) as ICO and raw RGBA."""

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


def main():
    small = pixels16()
    large = scale2(small)
    icon = ico([(16, dib(small)), (32, dib(large))])
    targets = [
        ROOT / "installer" / "oxypixel.ico",
        ROOT / "crates" / "oxypixel" / "assets" / "oxypixel.ico",
    ]
    for path in targets:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(icon)
    (ROOT / "crates" / "oxypixel" / "assets" / "icon32.rgba").write_bytes(rgba_bytes(large))
    print(f"wrote {len(icon)} byte icon")


if __name__ == "__main__":
    main()
