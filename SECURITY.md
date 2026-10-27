# Security

OxyPixel is a local editor. It does not phone home, and it does not execute scripts stored in documents. The only network request is the update check. It runs when the user chooses Help, then Check for updates, or once a day at launch if the user turned on Automatic updates (off by default; stored as `auto_update=1` in `view.txt`). It reads the public GitHub release list and downloads one release file plus `SHA256SUMS.txt`: after the user continues for a manual check, or by itself for the launch check when a newer release has a checksummed file for this system. Nothing is installed or replaced until the user clicks.

## Supported versions

| Version | Status |
| --- | --- |
| 0.2.x | Current release line |

## What we harden

- `.oxypix` parsing rejects bad magic, unknown versions, truncated sections, trailing bytes, and counts that would exceed the pixel budget. Those checks happen before the pixel buffers are allocated.
- PNG, BMP, and GIF decoding goes through the `image` crate with a maximum dimension of 1024 and an allocation cap.
- Reference images are PNG or JPEG only. Each side is at most 2048 pixels, the decoded buffer is capped, and at most eight are kept. Their sidecar is capped at 256 KiB and is not part of the `.oxypix` parser.
- Files larger than 80 MiB are rejected from their size before they are read into the parser.
- Palette files (GIMP `.gpl`, hex `.hex` or `.txt`, and JASC `.pal`) are read as text under that same size cap. A color count above 256 is rejected before the document palette is replaced. The JASC count is checked before those color lines are stored.
- OxyPack's `.aseprite` reader checks every read against the file's bounds, caps the canvas at 16384 a side and 4096 frames, refuses files whose frames together would not fit one atlas, and caps stored cel and tileset pixels, including the image a tilemap cel draws, before it allocates them. A compressed cel must inflate to exactly its stated size; the inflater stops one byte past it. Chunks and frames that run past their container are errors.
- `oxypixel mcp` reads JSON-RPC from stdin with a 16 MiB message cap and writes only protocol messages to stdout. It opens no network connection and runs no code from documents. Its tools read and write only the paths a call names; `create_document`, `export`, and `pack_sprites` refuse to replace an existing file unless the call sets `overwrite`, and `--root DIR` refuses any path outside DIR. Draw calls are capped at 10,000 operations and about a million points, and `render` at 4096 pixels a side.
- The project forbids `unsafe` in its own crates.

Image decoding still depends on `image` and the `png`, `jpeg`, `gif`, and `bmp` crates. Keep those updated when a decoder advisory is published.

## Threat model

A document is opened because a person asked for it. A hostile file should fail with an error or stay inside the memory caps above. It should not overwrite arbitrary paths: export writes only the path the user entered. An MCP client can ask `oxypixel mcp` to write any path the process may write; the client asks the user before each tool call, and `--root` narrows that to one folder. Autosave writes a recovery copy only inside the app data directory, as a temp file renamed into place, and removes that copy after a successful save.

The Windows build in this repository is **unsigned**. An unsigned installer can be modified in transit. Publish checksums with releases (`SHA256SUMS.txt` from the build scripts) and prefer an Authenticode signature once a certificate exists. Signing steps are in [docs/BUILD-WINDOWS.md](docs/BUILD-WINDOWS.md). Never commit a certificate or its password.

## Reporting

If this repository is private, open a private issue titled `Security:` and do not attach an exploit that drops files outside a temp directory. If the repository is later made public, use GitHub private security advisories instead of a public issue.

Include the OxyPixel version, the operating system, and a minimal document that triggers the bug if you can do that without sending secrets.
