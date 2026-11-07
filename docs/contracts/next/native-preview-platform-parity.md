# Native preview platform parity

The corrective source preserves the existing retained B0 image outputs and rejection behavior. It fixes Linux scaled JPEG/WebP loading and pins the color-management dependency used by the reference outputs. It does not declare a whole Files owner, catalogue workflow, Legal PDF verifier or production image complete.

The generated libvips scaled-loader binding passed an output-only flags property through the optional argument list. Real Linux backtraces and a matching C control reproduced the failure. The adapter now uses computed loader options on its fixed private filename, retaining sequential access and warning-level decoder rejection; no original filename or caller-provided option text enters that path.

The portable provisioner verifies the official LCMS 2.19.1 archive before building it beside libvips. The source release reports pkg-config version 2.19 and encoded CMM version 2190, so those values alone cannot establish the patch release. The pinned archive hash and build receipt establish its source. The provisioner retains the LCMS license.

libvips receives a deterministic packaging patch for library-level `$ORIGIN` RUNPATH. After installation, the provisioner checks both the exact ELF RUNPATH and that dependency resolution without loader environment overrides chooses LCMS from the same prefix. The Rust executable's direct-library RUNPATH alone cannot establish that transitive dependency.

The original 22-case renderer corpus passes on the qualified host and actual ARM64 worker after these changes; the four preview units, 18 toolchain cases and focused release Clippy gate also pass. The source-bound [local receipt](../../testing/evidence/2026-10-31-native-preview-linux-parity.json) records the evidence and source hashes. Fixtures and assertions remain unchanged.

The final worker proof uses an explicit verification-only library overlay on already compiled Rust. A complete fresh Dockerfile build, its final runtime/Compose checks, and the contributor environment remain separate delivery gates. This evidence does not qualify AMD64, whole-system integration or Legal PDF signatures.
