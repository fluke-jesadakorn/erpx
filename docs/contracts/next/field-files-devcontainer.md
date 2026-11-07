# Field visit Files contributor container

This extends the [Count contributor profile](service-count-devcontainer.md) with the merged [Rust Field Files capability](field-visit-files-native.md) and its [qualified native images](field-files-native-container.md). The supported boundary is Acme Account/Apps/Service on macOS ARM64 with the dedicated Colima engine. It does not qualify every Field workflow or owner, AMD64, another host engine or a production release.

Use the same keyed Linux-source-volume lifecycle and exact HTTPS origins described in the Count guide:

```sh
export ERPX_DEV_WORKSPACE_KEY=my-field-files-arm64-rust1991
export ERPX_DEV_SOURCE_REF=$(git rev-parse HEAD)
scripts/local/dev-container up --app=service
```

Source is cloned from the specified credential-free Git ref. Host working changes, ignored environment files and operator data are not imported. The toolchain image contains the pinned source-built preview dependencies, including Little CMS2.19.1; qualification resolves the actual loaded library under the native worker UID without loader environment overrides. The development image includes compilers and is separate from the minimal production images.

## Ownership and lifecycle

The workspace runs the Rust API, Count projection worker and Field Files worker as UID65532. The Files worker receives only the supplementary group needed for the isolated scanner socket and definitions. Startup waits for the actual scanner socket, version and fresh definitions before assembling native Files configuration. The renderer and server binaries are copied into a private runtime location only after the source/restart admission gate and graceful shutdown of the matched prior processes. Editing renderer source cannot silently replace a live worker's executable: ordinary start rejects a changed binary; explicit `restart-native` rebuilds and replaces the owned pair.

A dedicated keyed volume retains native Files objects separately from temporary files and rebuildable Cargo targets. Namespace `down/up` preserves source, databases, private keys, objects and caches. Existing data without the matching secret lease blocks initialization; reset remains an explicit exact-key operation. No operator volume, database or secret is automatically imported.

The synthetic contributor seed adds only the scoped Field role and visit needed for the journey. Its Support projection derives the UTC due date from the scheduled visit; existing deferred constraints remain enforced. The attachment feature is enabled only for the explicitly configured contributor Holding identity and native Field family. Global attachment rollout and unrelated owner panels are unchanged. Full-authority validation admits the exact development HTTPS origin on3443 and rejects different ports, suffix hosts, HTTP and production use.

Next remains the renderer and gateway, with transitional shared consumers listed in the Count guide. Rust owns the qualified Field Files commands, reads, scanner and cleanup, alongside native Count. [PR #8](https://github.com/fluke-jesadakorn/erpx/pull/8) removed the replaced Field-only TypeScript implementations; this profile does not restore them. Shared non-Field Files, mixed-owner lists/preferences and unrelated legacy jobs retain their named consumers and withdrawal conditions. Global readiness remains a whole-migration gate; health200 alone does not establish business readiness.

## Qualification

The source-bound [receipt](../../testing/evidence/2026-10-31-field-files-devcontainer.json) records fresh isolated bootstrap, migrations/seed and selected Service startup, followed by normal Account authentication, explicit Company/role selection, one upload, real scanner/preview, bind and checksum-matching download. A response-body capture failure after upload202 was reconciled against the exact committed synthetic upload before continuing; the upload was not replayed. Initial seed/scanner failures were corrected and affected steps retried, rather than counted as passes.

Actual Next source-edit HMR produced20 update frames with zero WebSocket, page or console errors. The reversible Rust edit built in3562ms; restoring the source reused the cache in86ms. These are observations on this case, not portable performance promises. A separate renderer edit proved ordinary start preserves the live binary/leases and rejects changed source; explicit restart publishes the new pair, followed by restoration and another successful restart. Namespace down/up preserved the bound original and its checksum, stable app keys and cached modules with zero rebuilt modules on observed warm compiles. Final normal authenticated readback had no page or console errors.

Official Dev Containers CLI0.90 `up` reused the exact existing Compose project/service container and successfully ran host initialization, bootstrap (zero foreign-key errors) and startup hooks. Official `exec` verified source identities and native/gateway health200. This qualifies CLI lifecycle hooks; a separate editor GUI attachment is not claimed.

An actual CLI attempt with an orphaned native Files volume failed before creating secrets, PostgreSQL or source; its sentinel was unchanged. Only that test-owned volume was removed. This is a bounded ownership proof, not a general crash-recovery matrix. Closing an editor leaves the keyed namespace running. The profile publishes only local3443 and does not replace gateway3100 or native Cloudflared routes.

## Scoped delivery checklist

- [x] Fresh keyed source, pinned toolchain and actual loaded preview library qualify.
- [x] Isolated migration/seed and selected Account/Apps/Service startup pass without operator data.
- [x] Normal authenticated native upload/scan/bind/content journey and final readback pass.
- [x] Persistent objects, app keys, caches and source survive namespace restart.
- [x] Actual HMR and incremental Rust edit/restore pass; renderer publication rejects unauthorized reuse.
- [x] Orphaned data fails closed; focused boundary tests and independent findings review pass.
- [x] Official Dev Containers CLI lifecycle passes against the existing exact project; final local source checks pass.
- [x] [PR #11](https://github.com/fluke-jesadakorn/erpx/pull/11) merged into `codex/rust-backend-migration` at `72e5b5f3b1de44e829b59f5169a85dd5ee97f62f`; remote SHA verified.
