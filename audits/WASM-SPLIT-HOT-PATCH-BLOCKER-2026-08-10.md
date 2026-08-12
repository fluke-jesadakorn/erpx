# Dioxus WASM split / hot-patch blocker — 2026-08-10

## Outcome

The production split lane works and meets the proposed production WASM budgets, but the required
development combination of Dioxus hot patching and WASM splitting is blocked in the pinned Dioxus
0.7.9 toolchain. No monolithic development fallback was enabled.

This is the explicit stop condition from the WASM-first-load acceptance plan. The complete ERPX
verification gate and browser acceptance wave were not run because the required development runtime
cannot become healthy.

## Reproduction

Tool version:

```text
dioxus 0.7.9 (bfcc111)
```

The ERPX-managed development command was observed as:

```text
dx serve --hot-patch --wasm-split --web --fullstack true \
  --no-default-features --offline --debug-symbols false \
  --session-cache-dir target/dx-hotpatch -p erpx-console \
  --port 3004 --addr 127.0.0.1 --open false
```

After compiling the split graph, `wasm-split-cli` panicked:

```text
wasm-split-cli-0.7.9/src/lib.rs:1248:17
assertion failed: reloc_range.end <= range.end
Build failed: Build panicked! JoinError::Panic(...)
```

The process retained the listener on `127.0.0.1:3004`, but a three-second cold request to `/login`
returned no bytes and timed out. This is not a healthy development runtime.

## Control build

The corresponding release split build without hot patching succeeded:

```text
dx build --web --fullstack true --no-default-features --release \
  --locked --offline -p erpx-console --debug-symbols false --wasm-split
```

It emitted 23 lazy modules (21 registered feature surfaces, public marketing, and the authenticated
Console application) plus one shared split chunk and the base module. This isolates the failure to
the required hot-patch development path rather than ordinary Rust compilation or release splitting.

## Artifact evidence

The successful control build produced the following Brotli quality-11 measurements:

| Artifact set | Raw bytes | Brotli bytes | Budget | Result |
| --- | ---: | ---: | ---: | --- |
| Initial base WASM | 943,238 | 279,362 | 1,310,720 Brotli | PASS |
| All 25 WASM files | 10,020,073 | 2,207,279 | 3,407,872 Brotli | PASS |
| Authenticated `LazyConsoleApp` chunk | 8,832,321 | 1,848,231 | informational | — |
| Public marketing chunk | 210,141 | 59,540 | informational | — |

The development base budget cannot be certified because the combined hot-patch/split build panics
before it emits a valid runnable artifact.

## Evidence boundary

- Native `cargo xtask check-affected` passed before the final macro-hygiene adjustment.
- The controlled release split build passed after that adjustment.
- `cargo xtask test-affected` and `cargo xtask verify` were intentionally not claimed.
- Cold-login network isolation, deep links, history navigation, hydration parity, chunk-failure
  recovery, response compression headers, and three-run M3 Max timings remain unverified.
- Development was not silently downgraded to monolithic mode.

Resolution requires a Dioxus/`wasm-split-cli` fix compatible with the pinned 0.7.9 toolchain, or an
explicitly approved change to the framework/tooling constraint or hot-patch requirement.
