# Dioxus v2 cutover baseline — 2026-08-18

This record distinguishes observed build facts from release certification. It
does not turn missing hardware measurements into a pass.

## Locked inputs

- Checkpoint before overwrite: `c0a37ee`.
- Rust: `1.93.1` from the repository-pinned toolchain.
- Dioxus CLI: `0.8.0-alpha.1`, revision `5fb7e6e`.
- Blitz crates: `0.3.0-beta.1`, revision `1127951`; Dioxus Native crates:
  `0.8.0-alpha.1`, revision `5fb7e6e`.
- Active delivery generation: v2; active service IDs: `IFC-SVC-201..265`.

The retained WebView measurement in
`audits/DIOXUS-NATIVE-MACOS/webview-baseline.json` reports 121,264 KiB RSS and
124,272,640 bytes maximum resident set at revision `47b0f55`. Its startup,
frame-time, and input-to-paint fields were not captured, so those comparisons
remain open rather than inferred.

## Release WASM observation

The release build used Dioxus Router splitting with `web,web-split` and emitted
the base, 36 lazy route modules, and one shared chunk. The generated schema-v3
manifest is runtime evidence under `.runtime/erpx/wasm-size-release.json` and
is intentionally outside Git.

| Gate | Observed | Limit | Result |
| --- | ---: | ---: | --- |
| Base Brotli | 369,297 bytes | 1,048,576 bytes | PASS |
| Largest workspace entry Brotli | 72,004 bytes | 768,000 bytes | PASS |
| Largest cold-route closure Brotli | 441,524 bytes | 1,835,008 bytes | PASS |
| Typed workspace entries | 21 | exactly 21 | PASS |

The verifier also rejects stale/unreferenced chunks and references to absent
chunks. Development intentionally remains monolithic for hot reload.

## Certification boundary

Web vitals over 20 locked mobile runs, Server Function façade latency, and the
30-run macOS M1 performance/accessibility suite have not been captured by this
source change. The existing native evidence template therefore remains
uncertified until populated with raw artifacts and hashes. No macOS release may
be represented as certified from compile success alone.

The 2026-08-19 platform gate compiled the same Native/Blitz graph for macOS,
`aarch64-apple-ios`, and `aarch64-linux-android`, including the Android bridge.
The Android lane used NDK 29, API 24 Clang, and the renderer's explicit
`native-activity` feature. `cargo xtask verify-platforms` reached its final
fail-closed check and remains red solely because the required M1 8 GiB
performance/accessibility `evidence.json` has not been captured.

The delivery cutover is structurally active but is not a production release:
Web bootstrap/query/public-preview use typed Server Functions, while typed
command, authentication, and public-form operation adapters still fail closed.
Native owns and invokes the TLS-only Tonic gateway from the active route graph,
but its operation-specific generated client adapters also still return typed
`Unavailable`. These are release blockers, not compatibility fallbacks.
