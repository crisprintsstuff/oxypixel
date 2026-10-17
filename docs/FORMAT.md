# `.oxypix` version 1

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

Trailing bytes are an error.

## Limits

| Limit | Value |
| --- | --- |
| Maximum side | 1024 |
| Pixels in one cel | 1,048,576 |
| Total pixels (sides × frames × layers) | 16,777,216 |
| Frames | 256 |
| Layers | 32 |
| File size | 80 MiB |

`layer_count` and `frame_count` are checked against these caps before any pixel buffer is allocated.

## Other formats

| Format | Read | Write |
| --- | --- | --- |
| PNG | New document, or one layer | Active frame, or a horizontal spritesheet |
| BMP | New document, or one layer | Active frame. Color is preserved; alpha handling follows the `image` crate |
| GIF | All frames as one layer named Imported | Composited frames, infinite loop |

Import as a layer crops or pads to the current canvas and reports which one happened.
