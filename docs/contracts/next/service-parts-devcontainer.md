# Service Parts contributor container qualification

This extends the [Field Files contributor environment](field-files-devcontainer.md) from merged source `1f71a96fbccddbd8b2a8c5e29b7734dd6661c880`, after the [Parts native retirement](service-parts-native-closure.md) and [shared Parts/Count image](service-parts-containers.md). It reuses the workspace image and lifecycle, without another owner container or TypeScript backend replacement image.

## Supported and observed scope

Fresh repository CLI population, bootstrap, migration/seed and selected Account/Apps/Service startup passed in the separate `service-parts-q-1f71-arm64` Linux-volume namespace. Normal Account authentication selected Acme and confirmed an existing Parts mapping through Rust, created warehouse/product masters and created/read back a draft Count. The real worker acknowledged that Count event with one HoldingRead receipt and checkpoint. These operations use the already qualified native writer paths; no new backend implementation is introduced here.

The same-case down/up and rebootstrap preserved source/data/keys, volume identities and caches. An actual source edit produced visible Next HMR and was restored, with zero captured WebSocket errors. Native cold bootstrap compiled the service in32,802ms; later first/warm checks took320/235ms and edit/restore checks4,289/3,125ms. These single-run observations are not performance acceptance or cold/warm averages.

Use the [shared CLI and editor setup instructions](field-files-devcontainer.md). The workspace declares `/workspace/erpx`, `postCreateCommand` bootstrap and `postStartCommand` start. Official Dev Containers CLI0.90.0 successfully reused this populated namespace and ran initialize/postCreate/postStart. Exact returned-container execution passed after a folder-discovery attempt failed; both outcomes are preserved. This does not establish fresh namespace creation by that external CLI or an editor GUI session.

## Source identity and known limitations

The [source-bound receipt](../../testing/evidence/2026-10-31-service-parts-devcontainer.json) records base and overlays, the unchanged workspace image, checksums and private evidence. The fresh guest used the earlier MJS diagnostic overlay recorded there. The final host-only diagnostic extension adds sanitized stage/path/status reporting and was exercised in warm readback and again after official CLI hooks; it was not retroactively included in the guest run.

Cold create completed with two React premount warnings and two unclassified404 console errors; restart readback had one premount warning. Later diagnostic and post-CLI readbacks captured zero page/console/HTTP/WebSocket errors. These later clean reads do not erase the cold warnings or close Field16. A disk-floor pause caused the original runner timeout while paused; the preserved case was resumed through the supported path, without replaying business commands.

This is existing-mapping confirmation and draft Count qualification. It does not claim zero-reference Finance preparation, Count approve/post, clean cold hydration, compiled release SSR, all applications, AMD64, public Tunnel activation or full41-owner migration. Retained generic legacy consumers and their withdrawal conditions remain in the native closure contract. No additional legacy implementation is removed by these environment-only changes. The engineering database bridge is separate, unqualified work and excluded from this delivery. Operator gateway3100, Tunnel registrations, staging and private production configuration remain unchanged.

## Delivery checklist

- [x] Fresh source population, actual migration/seed and selected application startup.
- [x] Normal SSO, native existing mapping/master/draft Count journey and worker readback.
- [x] Actual Next source-edit HMR and source restoration.
- [x] Native incremental edit/rebuild and restoration using actual runtime features.
- [x] Restart persistence of data, keys, source and caches.
- [x] Populated official CLI lifecycle and exact-container execution.
- [x] Source-bound evidence, known cold diagnostics and independent bounded review.
- [ ] Final documentation checks and scoped PR merge into the migration branch, with remote SHA verification.
