# Security

OxyPixel is a local editor. It does not phone home, and it does not execute scripts stored in documents. The only network request is Help, then Check for updates, which the user starts. That request reads the public GitHub release list and, if the user continues, downloads one release file plus `SHA256SUMS.txt`.

## Supported versions

| Version | Status |
| --- | --- |
| 0.2.x | Current release line |

## What we harden

- `.oxypix` parsing rejects bad magic, unknown versions, truncated sections, trailing bytes, and counts that would exceed the pixel budget. Those checks happen before the pixel buffers are allocated.
- PNG, BMP, and GIF decoding goes through the `image` crate with a maximum dimension of 1024 and an allocation cap.
- Files larger than 80 MiB are rejected from their size before they are read into the parser.
- The project forbids `unsafe` in its own crates.

Image decoding still depends on `image` and the `png`, `gif`, and `bmp` crates. Keep those updated when a decoder advisory is published.

## Threat model

A document is opened because a person asked for it. A hostile file should fail with an error or stay inside the memory caps above. It should not overwrite arbitrary paths: export writes only the path the user entered.

The Windows build in this repository is **unsigned**. An unsigned installer can be modified in transit. Publish checksums with releases (`SHA256SUMS.txt` from the build scripts) and prefer an Authenticode signature once a certificate exists. Signing steps are in [docs/BUILD-WINDOWS.md](docs/BUILD-WINDOWS.md). Never commit a certificate or its password.

## Reporting

If this repository is private, open a private issue titled `Security:` and do not attach an exploit that drops files outside a temp directory. If the repository is later made public, use GitHub private security advisories instead of a public issue.

Include the OxyPixel version, the operating system, and a minimal document that triggers the bug if you can do that without sending secrets.
