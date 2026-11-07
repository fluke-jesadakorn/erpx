# Owned HEIC portability fixture

The raster is an ERPX-authored 80×40 solid color with the synthetic EXIF description
`private metadata`. The recorded libheif 1.23.6 encoder applies a clockwise 90°
transform. The resulting 40×80 HEIC retains that description; the real preview
test requires decoding through the configured `heif-convert`, verifies the size,
and checks that the WebP preview removes metadata. Separate real parser cases
reject a truncated file and an appended ZIP prefix.

`manifest.json` binds the generator, source PNG and encoded HEIC with SHA-256.
There is no external photograph, personal data or downloaded fixture. The fixture
is distributed under the repository license.

Run `bun scripts/testing/fixtures/generate-attachment-heic.ts --check` for offline
integrity verification. Regeneration is explicit: use the recorded encoder via
`ERPX_HEIF_ENC` and `--write`, then review the bytes and manifest. CI consumes the
committed bytes and needs the real decoder, rather than an encoder CLI whose
options and plugins differ between supported platforms.
