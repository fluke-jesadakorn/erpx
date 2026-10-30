# Field Files native container capability

This is scoped ARM64 development/runtime preparation for the merged native Field Files capability. It does not enable production database roles, replace the operator gateway/Tunnel, or qualify all owners, workflows or architectures. PostgreSQL and configuration in the qualification are disposable, explicitly leased resources.

## Images and processes

One shared Dockerfile builds the modular-monolith server and preview renderer. Its `api` and `worker` targets run the same server binary as UID 65532. API executes `private-api`; worker executes `field-files-worker`, both with an explicit private development configuration. The API also needs the real preview tools because its intake-readiness check probes them. Their library/tool closure is included and counted in the measured images.

| Qualified ARM64 image | Docker-reported bytes |
| --- | ---: |
| Distroless API | 46,438,599 |
| Minimal Debian worker | 64,260,485 |

The combined `docker image save` stream was 110,746,112 bytes, including tar metadata; this is not a compressed registry transfer size. The pinned scanner image is a separate required dependency. Actual image IDs, source hashes and checks are in the [local verification receipt](../../testing/evidence/2026-10-31-field-files-native-container.json).

The portable source build verifies libvips 8.18.7 and LCMS 2.19.1 archives, preserves the LCMS license, and checks library-local runtime resolution. The worker contains the preview executable, required HEIF/Poppler tools and runtime libraries. Build/test toolchains remain outside the production runtime targets. Debian package versions are observed; this is not a claim that every package artifact or the entire image build is bit reproducible.

## Build from a clean source context

Use the migration integration source containing this recipe. Keep private configuration and data outside the build context. The verified build used a clean Git archive and the checked-in Docker ignore file, rather than the operator checkout's environment or caches.

```sh
erpx_build_sha=$(git rev-parse HEAD)
erpx_build_ctx=$(mktemp -d "${TMPDIR:-/tmp}/erpx-field-files-build.XXXXXX")
git archive "$erpx_build_sha" | tar -xf - -C "$erpx_build_ctx"
cp ops/containers/field-files-native.Dockerfile.dockerignore "$erpx_build_ctx/.dockerignore"
docker build --platform=linux/arm64 --target=api \
  --build-arg TARGETARCH=arm64 --build-arg ERPX_SOURCE_KEY="$erpx_build_sha" \
  -f "$erpx_build_ctx/ops/containers/field-files-native.Dockerfile" \
  -t erpx-field-files-api:local "$erpx_build_ctx"
docker build --platform=linux/arm64 --target=worker \
  --build-arg TARGETARCH=arm64 --build-arg ERPX_SOURCE_KEY="$erpx_build_sha" \
  -f "$erpx_build_ctx/ops/containers/field-files-native.Dockerfile" \
  -t erpx-field-files-worker:local "$erpx_build_ctx"
```

Retain build receipts before removing that temporary context. ARM64 execution is verified; AMD64 remains a separate build/runtime gate. The current worker feature includes broader Rust build dependencies through `existing-business`; the measured stripped runtime, not the number of linked workspaces, establishes the delivered size.

## Explicit runtime inputs

`ops/containers/field-files-native.compose.yaml` requires every configuration/data location explicitly and disables implicit `.env` loading in the documented invocation. Set `COMPOSE_DISABLE_ENV_FILE=1` and choose an owned Compose project name. The required variables are `ERPX_FILES_API_IMAGE`, `ERPX_FILES_WORKER_IMAGE`, `ERPX_FILES_NETWORK_CONTAINER`, `ERPX_FILES_CONFIG_DIR`, `ERPX_FILES_STORE_DIR`, `ERPX_FILES_TEMP_DIR`, `ERPX_CLAMD_SOCKET_DIR`, and `ERPX_CLAMAV_DEFINITIONS_DIR`.

The namespace container belongs to the isolated leased PostgreSQL case so validated native connections remain exactly loopback:5432. This file is not an adapter for the installed host production database. No service publishes host gateway3100 or changes registered public hosts.

`private-api.json` supplies the existing typed Account/Holding configuration plus explicit Field Company bindings, `fieldAttachmentLinks`, `fieldAttachmentBinds`, `fieldFilesPolicy` and `fieldFilesProviders`. Runtime provider paths are `/var/lib/erpx-files`, `/var/tmp/erpx-files`, `/usr/local/bin/erpx-render-preview`, `/usr/bin/heif-convert`, `/usr/bin/pdftoppm`, `/run/erpx-scanner/clamd.sock`, and `/var/lib/clamav`. Keep configuration private and readable by UID 65532; never put credentials in images or this repository.

The scanner receives only `clamd.conf`, its socket directory and read-only definitions; it receives no API/database configuration. Its configured-socket health check gates API/worker startup. Roots are read-only, all capabilities are dropped, `no-new-privileges` is enabled, writable storage/temp/socket locations are explicit, and native process shutdown has a bounded grace period. The scanner has no network; current verified definitions must be provisioned separately. Signature refresh scheduling and production backup/restore/rollback remain operations gates, not outcomes of this fixture.

## Verification and retirement

The fresh default Dockerfile build used merged source `168420994b9b335bf0a470b67f96bb2e44a38ff4` with the recorded recipe/ignore hashes. Cold Compose startup, normal Account/SSO, real scanning/preview, eight exact HTTP previews across PNG/JPEG/WebP/HEIC/PDF, exact polyglot rejection, bind, checksum-matching download, expired-draft cleanup and graceful exits passed. The unchanged full 22-case retained renderer corpus passed on the exact default-built worker.

The qualification's failure cleanup separately proves that a nonzero owned worker exit remains a reported failure, all owned project containers are removed, and an independent canary stays running. It does not prove recovery of an in-flight business job after a crash.

The replaced Field Files TS implementation was removed in [PR #8](https://github.com/fluke-jesadakorn/erpx/pull/8); the Linux preview correction followed in [PR #9](https://github.com/fluke-jesadakorn/erpx/pull/9). Shared non-Field TS consumers remain outside these images' backend runtime and retain explicit native Field exclusion. Full Field/owner/catalogue counts do not increase from this container proof.


## Scoped delivery checklist

This is a container checklist, separate from the full Field/workflow/owner registers.

- [x] Fresh default API and worker targets build from the exact merged Rust source and pinned native provisioner.
- [x] Actual image sizes, architecture, identical server binary and provider-library identities are recorded.
- [x] Cold Compose startup waits for the actual scoped scanner; scanner receives no database configuration.
- [x] Normal SSO, native file lifecycle, exact HTTP previews and expired-draft cleanup pass with hardened runtime processes.
- [x] The unchanged 22-case preview corpus passes on the exact default worker image.
- [x] Normal teardown and nonzero-service failure teardown preserve ownership and failure reporting.
- [x] Independent review and targeted local source/documentation checks pass; hosted CI success is not asserted.
- [ ] Scoped PR merges into `codex/rust-backend-migration` and its remote SHA is verified.
