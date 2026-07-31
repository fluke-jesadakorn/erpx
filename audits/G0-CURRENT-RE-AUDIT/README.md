# G0 current-state re-audit — 2026-07-27

## Scope and verdict

This packet re-audits the current ERPX/Folio bridge baseline on branch
`codex/erpx-parallel-completion`. It covers the workspace shape, shared
contract/protobuf compilation, deterministic migration fixture package, final
warnings-denied verification, Dioxus targets, Folio checks, and the explicit
remaining blockers. It is evidence for the current tree, not a production
readiness or cutover claim.

G0 is **locally evidenced but not gate-closed**: an approved protobuf
descriptor baseline is not configured, and the broader G0 contract requires
independent compatibility comparison against that baseline. No production
database, customer directory, command owner, Folio write path, or cutover
state was changed by this audit.

## Current baseline

- Folio remains the operational legacy write owner and its protected
  `/operations` route responds with HTTP 307 on port 3004.
- ERPX includes the Rust/Tonic/SQLx/PostgreSQL/MinIO bridge, typed protobuf
  contracts, server and worker composition, Dioxus console, deterministic
  migration fixture package, directory identity/RBAC/feed contracts, and
  platform/product read adapters.
- The current read-only structural probe confirms the required G14 evidence
  files, directory vendor/feed/TLS seams, server/worker/protobuf registration,
  and branch ownership without mutating state.

## Verification evidence

| Evidence | Result |
|---|---|
| `cargo fmt --all --check` | PASS |
| `cargo clippy --workspace --all-targets --offline -- -D warnings` | PASS |
| `cargo test --workspace --offline --no-fail-fast` | PASS, including workspace and doc tests; console 69/69 |
| Full ignored workspace suite with ten fresh disposable PostgreSQL databases | Every local target PASS; generated databases cleaned; sole nonzero target is the intentional credentialless G14 external preflight |
| `cargo check -p erpx-proto --offline` | PASS |
| G13 compatibility / deployment recovery | 9/9 and 4/4 PASS; approved descriptor comparison remains queued without a baseline |
| G14 directory acceptance | 8/8 PASS; vendor certification remains queued |
| Dioxus web, desktop, host mobile, iOS, Android, and macOS desktop target checks | PASS |
| Folio `bun run lint` | PASS, zero errors and four pre-existing n8n warnings |
| Folio `bunx tsc --noEmit` | PASS |
| `check_read_only.sh`, `git diff --check`, port-3004 probe | PASS |

## Remaining G0 blockers

The independent compatibility comparison needs an approved descriptor
reference; no such baseline is configured. The G14 external preflight also
continues to fail closed because credentialed Generic LDAP/RFC4533, AD
DS/DirSync, and UCS Provisioning Service environments and their measured
import/revocation evidence are unavailable. Physical mobile execution, live
Folio migration/rollback/retention evidence, MinIO round-trip, and explicit
production approval remain outside this local re-audit.

These blockers are queued rather than converted into mocks, schema-only
claims, or production actions.
