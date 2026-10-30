# Shared native Parts and Count container

This follows [native Parts writer closure](service-parts-native-closure.md), merged in [PR #12](https://github.com/fluke-jesadakorn/erpx/pull/12) at `443623c863c09d5a8a89cd239a97b3a85784b8b9`. It reuses the [Count Dockerfile/Compose](rust-service-count-containers.md); no owner-specific image or TypeScript backend container is added.

## Measured artifact and scope

The fresh ARM64 build from that exact Git archive produces one distroless, non-root image shared by the API and Count worker:

| Observation | Value |
| --- | --- |
| Image identity | `sha256:024812ee1bcb85971a419f870a5984666a1682b12e8bb2bd084f4e966fbe3372` |
| Docker inspect uncompressed bytes | 16,958,478 |
| Native binary SHA-256 in runtime and test fixture | `3970dd0d9591280d66d46f24133dab25a209ddfc9f46f11430424da510848989` |
| Runtime UID/GID | 65532/65532 |
| API/worker memory at the captured instant | 4.801/3.43 MiB |

Size is the measured runtime image, not a compressed registry transfer or builder/fixture size. The toolchain and test fixture are separate. These point observations do not establish sustained performance or deployment sizing. AMD64 remains unqualified. The runtime has the native binary and migration inventory; it contains no Node/Bun/Python business backend. The Dockerfile and Compose are unchanged; only the existing qualification is strengthened.

Actual isolated qualification passes normal Account/SSO, empty initial Finance references and mapping, native chart/account/period/rate preparation, mapping readback, warehouse/product creation and selected draft Count creation/readback. PostgreSQL proves those exact chart/master receipt IDs link to the persisted Count source. The real worker acknowledges its outbox event and writes the matching HoldingRead receipt/checkpoint. This container run does not claim Count approve/post; that complete functional journey has separate source-bound browser/PostgreSQL evidence in the Rust delivery.

Both processes use read-only roots, dropped capabilities, no-new-privileges and bounded writable temporary storage. The test PostgreSQL namespace uses exact isolated development leases, not installed operator databases or production roles. All three owned long-running processes exited0 and owned test resources were removed. The Account helper observes health200 and full-migration readiness503; readiness is not forged. Only normal teardown is qualified here, with no new crash/fault matrix claim.

## Shared runtime and retained dependencies

Use `ops/containers/service-count-native.Dockerfile` with `private-field-api` and the existing explicit Compose configuration inputs. The enhanced qualification records recipe hashes, image revision/architecture/user/size and identical native binaries. It invokes the real setup and Count routes; it does not route these commands through legacy TypeScript.

The [Field Files images](field-files-native-container.md) remain separate feature builds from their recorded source. This minimal Parts/Count image does not supply the Files native-tool closure and must not replace a broader configured API without its own combined feature qualification. Next rendering and retained legacy consumers remain outside this image; no frontend production container or public Tunnel cutover is claimed. Full41-owner Rust-only topology, actual production privileges, AMD64, sustained performance and encrypted restore/rollback remain task gates.

The [receipt](../../testing/evidence/2026-10-31-service-parts-containers.json) binds the exact runtime, fixture, scripts and local evidence. Source changes are scoped to qualification and documentation. No image was published and `development`, staging, private production and native Cloudflared routes are unchanged.

## Delivery checklist

- [x] Fresh exact merged-source ARM64 shared runtime builds; recipe/feature identity recorded.
- [x] Image size, binary identity and non-root metadata verified.
- [x] Normal SSO and initially empty native Parts/master/Count lifecycle and exact source links pass.
- [x] Actual worker acknowledgement/projection and health/readiness semantics pass.
- [x] Hardened processes, normal graceful exits and owned resource cleanup pass.
- [ ] Independent review/final local checks and scoped PR merge into the migration branch; verify remote SHA.
