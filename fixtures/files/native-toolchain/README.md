# Native compiler source pins

This manifest supplies development/CI build tooling for the separate Files native-worker lane. It does not define a production image or establish runtime preview parity.

The immutable libvips 8.18.7 release archive is pinned to SHA256 `5baaead3b0bb20ffdb9e9ff09aa9fda08620923df77b63b436654cb5e0b3bf94`. The digest and byte size were checked against the [upstream release asset metadata](https://api.github.com/repos/libvips/libvips/releases/tags/v8.18.7). The source build uses the [official installation procedure](https://www.libvips.org/install.html) and explicit options from the [tagged Meson definitions](https://github.com/libvips/libvips/blob/v8.18.7/meson_options.txt).

Meson 1.12.1 and the native Linux AMD64/ARM64 Ninja 1.13.2 wheels have separate exact URL, byte-size and SHA256 pins from their [Meson package metadata](https://pypi.org/pypi/meson/1.12.1/json) and [Ninja package metadata](https://pypi.org/pypi/ninja/1.13.2/json). Wheel installation requires these hashes, uses only the already verified local downloads, and resolves no extra package dependencies. Python is a development build-tool prerequisite for [Meson](https://mesonbuild.com/Quick-guide.html).

The provision command requires an explicit new Linux prefix. Official distribution development packages must already exist. The manifest lists those packages, and each build retains their actual versions plus the compiler and options. Distribution packages, system libraries and compiler binaries are version-observed, not claimed to be artifact-pinned. Meson dependency downloads are disabled. Compilation is limited to two jobs.

The generated receipts distinguish verified downloaded source artifacts from an observed installed prefix. An ordinary preflight does not attest source provenance. The current retained Darwin [provider profile](../preview/darwin-qualification-profile.json) is separate evidence. Linux hosted compilation and the 22-case runtime codec corpus require their own actual successful receipts.

The Linux compiler configuration embeds the explicit native library directory as ELF RUNPATH through fixed Cargo-encoded linker arguments. The same verified flags survive later quality/provider rebuilds. Both CI paths inspect the actual ELF and require its clean, minimal-environment argument-admission startup; no loader environment override is introduced. These hosted checks are pending execution and remain separate from all decoder corpus claims.
