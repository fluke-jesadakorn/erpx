# ERPX vendored Dioxus Components primitives

- Upstream: https://github.com/DioxusLabs/dioxus-components
- Commit: `bf007c15d0cf4d04d3181cc46cf12325aa773955`
- Imported: 2026-08-08
- License: MIT OR Apache-2.0 (see `LICENSE-MIT` and `LICENSE-APACHE`)

ERPX vendors the `primitives` and `dioxus-attributes` crates because the
published `dioxus-primitives` crate is not the implementation used by the
official component registry and repository source policy rejects floating Git
dependencies. The Cargo manifests are normalized to standalone path crates and
pin Dioxus 0.7.9. Product styling lives in `erpx-ui`; vendored behavior remains
upstream-shaped so future updates can be diffed against the commit above.

Local patches beyond the standalone manifests:

- Controlled-open animated overlays seed their first render from `open`,
  avoiding SSR/hydration tree mismatches before effects execute.

To update, import a reviewed upstream commit, retain this provenance record,
reapply the standalone manifest changes, and run the complete ERPX verification
and platform certification gates.
