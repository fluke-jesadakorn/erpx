# ERPX Native renderer source closure

ERPX uses this vendored Dioxus Native/Blitz closure as its only native
renderer. A WebView renderer is neither compiled nor used as a fallback.

The sources are the crates.io packages locked on 2026-08-18. The recorded
archive checksums are copied from `Cargo.lock`; Cargo resolves every package
through the root `[patch.crates-io]` table.

| Package | Version | Upstream revision | crates.io SHA-256 |
| --- | --- | --- | --- |
| `blitz-traits` | `0.3.0-beta.1` | `11279518661e1e0af3ee141c232daffa8968d3fe` | `9bbf266d7d6d2cf00e79d2158b0fa399ff38f75c0e2d84caeb2e90ff1d7fb3f8` |
| `blitz-dom` | `0.3.0-beta.1` | `11279518661e1e0af3ee141c232daffa8968d3fe` | `a9f438e914c626f4f56cd58cd88045e164e1ac32e12d7c24d09c25121328161e` |
| `blitz-html` | `0.3.0-beta.1` | `11279518661e1e0af3ee141c232daffa8968d3fe` | `f4e13102df7e67fc61122b525b0f7dcc0ee8b88cdd4ea19e2d49ba2c46b51489` |
| `blitz-net` | `0.3.0-beta.1` | `11279518661e1e0af3ee141c232daffa8968d3fe` | `2d9304744317620b6a0f610ca58763cc236624f467ac82d6dcae7da606167358` |
| `blitz-paint` | `0.3.0-beta.1` | `11279518661e1e0af3ee141c232daffa8968d3fe` | `38a033d71915e8ded2b046b134c06c8042a0f1907f7624b6b6365bf99977c845` |
| `blitz-shell` | `0.3.0-beta.1` | `11279518661e1e0af3ee141c232daffa8968d3fe` | `393a06efd2f247e8a16cc4558601cb923fa9c2c64610fa8f75b7773c4c3286b7` |
| `dioxus-native` | `0.8.0-alpha.1` | `5fb7e6e1ca748a0901bd536388c7eb567e560b74` | `70ad209f3b9f5250005ca3a7aef44d23d6b932b55477b1317c0e0fc0675fe462` |
| `dioxus-native-dom` | `0.8.0-alpha.1` | `5fb7e6e1ca748a0901bd536388c7eb567e560b74` | `8403e63d8933bb7ee94c82732dc024727373bfe47e261ae7f2747dabd1a63f7c` |

The upstream Dioxus Native DOM package records a dirty source tree in its
crate metadata. The crates.io archive checksum above is therefore the
authoritative source identity in addition to the upstream revision.

License: MIT OR Apache-2.0. The complete license texts are in
`LICENSE-MIT` and `LICENSE-APACHE` in this directory.

## ERPX patch log

1. Route AccessKit `ActionRequested` activation, focus, blur, expand, and
   collapse through the DOM/framework event pipeline, then rebuild the tree.
2. Expose semantic roles, labels, values, disabled/selected/expanded/checked
   state, dialogs, and supported actions to AccessKit.
3. Preserve UTF-8 IME composition data and dispatch Dioxus composition
   start/update/end events while the existing input event path updates editable
   values.
4. Select exactly the Android `native-activity` implementation so the shared
   renderer graph never resolves an unconfigured activity entry point.

Upstream replacement is allowed only after the candidate revision passes the
same ERPX native regression and platform certification suites.
