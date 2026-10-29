# Native Files preview conformance

`b0-results.json` freezes 22 independently executed Node26.11.1 cases from the unchanged B0 preview adapter. The B0 source, source SHA256, actual Sharp/native versions and every original/output byte hash are retained. Inputs are generated synthetic images/PDF/CSV and the repository's own rotated HEIC. They contain no tenant uploads. The Node/Sharp generator is a development oracle only; the Rust worker has no Node or Python fallback.

The image contract is EXIF autorotation, an inside2400×2400 fit without enlargement, sRGB, WebP quality82/effort4 and stripped metadata. PDF uses the actual `pdftoppm` first-page `-scale-to2400 -jpeg` recipe; it can enlarge a small PDF page. CSV/DOCX have no preview. These are distinct behaviors. Byte-signature admission and direct renderer behavior are also distinct: B0's direct JPEG renderer accepts the appended-ZIP fixture, while the actual source admission rejects it. Native `PreviewInput` preserves the latter gate.

The corpus includes RGB/CMYK with and without ICC, real Display-P3-compatible ICC,16-bit PNG, non-square EXIF orientation, alpha resampling, decoder shrink-on-load, malformed files and a40-million-pixel boundary rejection. Generated decoder output is checked again by a Rust container/metadata/dimensions guard before an artifact can be retained. That guard is not a general-purpose image decoder or malware verdict.

## Color-order evidence

`p3-ordering-counterexample.json` retains the smaller P3 fixture's failure under the rejected thumbnail-only pipeline (max decoded channel error36, mean3.448421052631579). `provenance.json` separately preserves the earlier larger fixture's exact input/output hashes and error receipt; the original private capture was not overwritten. These errors are counterexamples, never accepted tolerances.

Sharp applies embedded ICC conversion before resize. The native Rust/libvips pipeline preserves that order, including B0's explicit invalid-embedded-ICC fallback. It also preserves16-bit processing, alpha premultiplication and JPEG/WebP loader scaling. The generated Rust binding's `webpsave_with_opts` always sets the nullable `profile` argument, which can force ICC retention despite `keep=None`; the worker uses the vetted high-level file writer with fixed `[Q=82,effort=4,keep=none]` options instead. The independent output-container guard rejects retained ICC/EXIF/XMP/IPTC metadata. No caller can supply these options.

## Isolation and resource contract

The dedicated Rust child and its native descendants use the optional Runtime `owned-process` infrastructure. The parent retains the unreaped leader with `waitid(WNOWAIT)` until group cleanup finishes, then reaps it. Cancellation/deadline/shutdown retain the capacity permit and owned temporary directory until cleanup; a kernel-level cleanup failure is not reported as success. Runtime tests exercise an exited leader with a live descendant, closed pipes with a live leader, caller cancellation and shutdown.

The separate `preview_process` provider target uses a test-only Rust native-tool fault fixture gated by `contracts`; it supplies no successful decoder. The actual Rust preview child is held inside the normal fixed Poppler invocation while its configured operator executable is atomically replaced, then returns a real typed failure. The retained regression originally returned `DecoderRejected`; the corrected parent keeps every completed response provisional until its executable-profile and aggregate-deadline checks finish. Tool replacement therefore returns `ToolConfiguration`, while a stable rejected decode retains its original failure. Additional cases exercise the reduced deadline and caller cancellation with a live native descendant. The original22-case codec corpus is unchanged.

The preview component admits at most10MiB original input,40million image pixels and32MiB final preview output. Native HEIF decoded PNG is capped at100MiB. The parent applies a caller-reducible40s aggregate compute deadline; each HEIF/Poppler invocation retains its20s cap. This adds a bounded image deadline to the old Sharp path and is a security/resource contract, not a claim of identical worst-case timing. The later scan orchestration must reduce the budget by actual remaining60s lease time and cleanup headroom.

Stdout+stderr are capped together at1MiB; no private native stderr is returned as a business message. The child clears its environment, uses fixed private filenames, creates0600 files under0700 directories and sets a private umask. Hard inherited CPU/file/descriptor/stack/core limits apply; Linux additionally applies address/data limits. Group RSS is sampled every25ms with a1GiB threshold. A separate sampled aggregate flat-directory guard permits at most128 regular files and384MiB for this profile; symlinks/subdirectories are rejected. Sampled guards are not cgroup/quota guarantees. A production worker/container still needs its qualified OS memory/disk envelope.

`PreviewProviderProfile` hashes configured executable bodies and the protocol/version marker before/after compute. It is data about those artifacts, not proof of user/record authorization, scanner health, a complete dynamic-library SBOM or release readiness. The retained Darwin qualification profile separately records26 inspected native artifacts and hashes. The worker/API default dependency graph does not enable the native `preview-provider` feature.

## Provenance

The native source release is [libvips8.18.7](https://github.com/libvips/libvips/releases/tag/v8.18.7); its pinned archive SHA is in `provenance.json`. The optional [OLX Rust binding2.3.0](https://github.com/olxgroup-oss/libvips-rust-bindings) is MIT; its license is retained in `licenses/libvips-rust-MIT.txt`. Native libvips is dynamically linked under LGPL-2.1-or-later; `licenses/libvips-LGPL-2.1.txt` retains its license. This is development qualification, not completed distribution compliance or an accepted production image.

The embedded P3 and sRGB profiles are bit-identical to [Compact-ICC-Profiles](https://github.com/saucecontrol/Compact-ICC-Profiles/tree/bdd84663061bc4ae95ca70decff54f581e27f702), respectively `DisplayP3Compat-v4.icc` and `sRGB-v4.icc`. Their source commit, SHA256 and CC0 license receipt are retained. These are not proprietary macOS/Adobe profiles. The upstream CMYK fallback's embedded public-domain notice is retained separately. The synthetic source fixture generator preserves these notices; it does not imply that arbitrary uploaded ICC data is ours to redistribute.

## Verification and remaining boundaries

Run the development oracle with pinned Node and the React-server import condition:

```sh
node --conditions=react-server scripts/testing/fixtures/generate-files-preview-goldens.ts --check
```

The default Cargo `preview` target tests portable source admission and independent output guards. The `preview_provider` provider target requires an explicitly qualified `PKG_CONFIG_PATH`, `ERPX_HEIF_CONVERT` and `ERPX_PDFTOPPM_BIN`, and compiles the actual `erpx-render-preview` child. Missing prerequisites fail; tests do not substitute decoders or skip cases.

Current corpus qualification is Darwin ARM64 with exact recorded versions. Linux ARM64/AMD64 native dependency/decoder qualification, source-build CI bootstrap and production image/SBOM are still pending. An arbitrary distribution's libvips/HEIF/Poppler version is not assumed equivalent.

This component does not mount a route, claim a job, publish a source or authorize a preview read. Opaque PostgreSQL scan-lease/source handling, immutable derivative storage, atomic scanner-result/preview/audit retention and fresh per-owner authorized cached reads are the next owner slice. The existing Legal-only available-original reader is not broadened to authorize quarantined scan input, HEIF or generic source labels. No permission is derived from `created_by`, a target label, caller path or object key.
