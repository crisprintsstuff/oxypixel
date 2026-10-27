# `.oxypix` versions 1 and 2

Little-endian. No compression. A file that does not match this layout is rejected.

```text
magic          8 bytes   OXYPIX1\0
version        u16       1
width          u32
height         u32
active_frame   u32
active_layer   u32
layer_count    u32
frame_count    u32
```

Then `layer_count` records:

```text
name_len   u16    at most 128
name       name_len bytes of UTF-8
visible    u8     0 or 1
opacity    u8     0-255
```

Then `frame_count` records:

```text
duration_ms  u32   20 through 10000
pixels       layer_count times width*height*4 bytes, RGBA, bottom layer first
```

Then the palette:

```text
palette_count  u16   1 through 256
colors         palette_count times 4 bytes RGBA
```

In version 1, trailing bytes are an error.

## Version 2

Version 2 is version 1 with `version` set to 2 and a list of sections after the palette:

```text
section_count  u16   at most 64
repeated section_count times:
  kind         u16
  len          u32   bytes in this section
  body         len bytes
```

Bytes after the last section are an error. A kind with the high bit set (`0x8000`) must be understood: a reader that does not know it rejects the file with "this file needs a newer OxyPixel". A kind without that bit may be skipped. Each kind appears at most once.

OxyPixel writes version 2 only when the document uses a section; a document without one is written as version 1, byte for byte as before. OxyPixel 0.8.5 and earlier read version 1 only.

### Section `0x0001`: animation tags (optional)

```text
count        u16   at most 256
repeated count times:
  from       u16   first frame, from 0
  to         u16   last frame, included; from <= to < frame_count
  direction  u8    0 forward, 1 reverse, 2 ping-pong, 3 ping-pong reverse
  repeat     u16   times the range plays; 0 is forever
  name_len   u16   1 through 128
  name       name_len bytes of UTF-8
```

A body longer than its tags is an error.

### Section `0x0002`: frame names (optional)

```text
count        u16   must equal frame_count
repeated count times:
  name_len   u16   0 through 128; 0 is a frame without a name
  name       name_len bytes of UTF-8
```

Written only when at least one frame has a name. A body longer than its names is an error.

### Section `0x8001`: layer blend modes (required)

```text
count        u16   must equal layer_count
repeated count times, bottom layer first:
  mode       u8    0 normal, 1 multiply, 2 screen, 3 overlay, 4 darken, 5 lighten,
                   6 color dodge, 7 color burn, 8 hard light, 9 soft light,
                   10 difference, 11 exclusion, 12 hue, 13 saturation, 14 color,
                   15 luminosity, 16 addition, 17 subtract, 18 divide
```

The codes are Aseprite's. Written only when some layer is not Normal. A count that differs from the layer count, an unknown code, or extra bytes are errors. The section is required: OxyPixel 0.9.2 and earlier reject a file that has it, rather than drawing every layer as Normal and losing the modes on save.

### Section `0x8002`: layer groups (required)

```text
count        u16   must equal layer_count
repeated count times, bottom layer first:
  flags      u8    0 a layer, 1 a group
  depth      u8    how many groups the layer is inside, 0 through 8
```

A group holds the layers right after it that are one level deeper, so the layer list is a tree written in order. The first layer has depth 0, a layer can be at most one level deeper than the one before it, and only right after a group. At least one layer must not be a group. A group's cels are stored like any layer's but are clear and never drawn; the group is drawn by compositing the layers inside it into their own picture and mixing that picture in with the group's visibility, opacity, and blend mode. Written only when the document has a group. A wrong count, a flag other than 0 or 1, a tree that breaks these rules, or extra bytes are errors. Required, so older OxyPixel refuses the file rather than flattening the groups.

Reference images are not a field in this file. Version 1 has no room for them: trailing bytes are rejected, and a new field would be a new version. A saved document may have a sidecar, `<name>.oxypix.refs`, in the same directory. Save writes it. That file lists PNG or JPEG paths and where each image sits. A path to an image inside the document's directory is relative to that directory; any other path is absolute. The pixels stay in those image files. A reader that only understands this document never sees them, and export does not read the sidecar.

## Limits

| Limit | Value |
| --- | --- |
| Maximum side | 1024 |
| Pixels in one cel | 1,048,576 |
| Total pixels (sides × frames × layers) | 16,777,216 |
| Frames | 256 |
| Layers | 32 |
| Palette colors | 256 |
| Tags | 256 |
| Sections (version 2) | 64 |
| File size | 80 MiB |

`layer_count` and `frame_count` are checked against these caps before any pixel buffer is allocated.

## Other formats

| Format | Read | Write |
| --- | --- | --- |
| PNG | New document, or one layer | Active frame, or a horizontal spritesheet |
| BMP | New document, or one layer | Active frame. Color is preserved; alpha handling follows the `image` crate |
| GIF | All frames as one layer named Imported | Composited frames, infinite loop |
| GIMP palette (`.gpl`) | Replaces the document palette. `Name:` and `Columns:` are ignored. A trailing number is alpha only when it is the last token | `R G B`, or `R G B A` when alpha is not 255 |
| Hex palette (`.hex`, `.txt`) | Replaces the document palette. One `RRGGBB` or `RRGGBBAA` per line | The same list, uppercase, with alpha digits only when alpha is not 255 |
| JASC palette (`.pal`) | Replaces the document palette. Alpha becomes 255 | `JASC-PAL`, version `0100`, then `R G B` lines |

Import as a layer crops or pads to the current canvas and reports which one happened. A palette file with no colors, a bad line, or more than 256 colors is rejected and leaves the document palette unchanged. Comments and blank lines are skipped.
