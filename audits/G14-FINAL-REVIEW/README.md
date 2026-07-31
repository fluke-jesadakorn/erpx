# G14 final review — post-integration structural gate probe

Date: 2026-07-27  
Mode: read-only structural review after directory, RBAC, Operations, Finance,
product, UI, and G14 lane integration

## Verdict

**ERPX is not proven complete across G0–G14. No gate is closed by this
review.** The latest wiring is present in the tree, but structural markers,
focused packet evidence, compilation claims, or fail-closed seams do not
substitute for the authoritative/live acceptance required by the ERPX plan.

This review owns only this README. It does not authorize customer-directory
activation, production command-owner changes, Folio write retirement, or final
cutover.

## Read-only checks performed

| Check | Result | Interpretation |
|---|---|---|
| Branch check | PASS — `codex/erpx-parallel-completion` | Required branch is checked out. |
| `G14-PARALLEL-COMPLETION/check_read_only.sh` | Structural PASS | Required files, directory feed/TLS/RBAC markers, protobuf service marker, server/worker registration markers, tool availability, and audit-package scope were found. This is not a completion proof. |
| `git diff --check` | PASS | No whitespace errors were reported. |
| Folio `http://127.0.0.1:3004/operations` | BLOCKED — HTTP 000 / connection refused | The development server was not listening during this probe; Folio runtime continuity was not verified. |
| External directory acceptance | NOT RUN | No credentialed generic LDAP, AD DS, or UCS environment was available. |
| Cargo, Dioxus, protobuf-compatibility, and Bun verification | NOT RUN in this read-only probe | Historical packet claims were not promoted to current full-wave evidence. |

The existing worktree contains unrelated user changes and the broader ERPX
parallel wave. They were not modified by this review.

## Post-integration structural findings

The following seams are present in the current source tree. “Present” means
that the implementation or registration marker was found; it does not mean
that the corresponding G gate is accepted.

| Area | Current structural finding | Acceptance boundary still open |
|---|---|---|
| Directory authentication | `identity::directory::{auth,tls}` is registered; generic LDAP, on-prem Microsoft AD DS, and UCS provider policies, TLS-only modes, immutable provider keys, high-risk MFA, and local-only break-glass contracts are present. | Real certificate/hostname validation, bind, reconnect, vendor behavior, and live MFA/revocation evidence. |
| Directory synchronization | `feeds` and `sync` are registered; RFC 4533, AD DirSync, UCS Provisioning Service, bounded import planning to 1,000,000 users, checkpoints, immutable links, SLA records, outbox, and rollback seams are present. | Credentialed vendor runs, one-million-user bounded-memory evidence, native cursor continuity, source/read reconciliation, and measured five-minute revocation. |
| Directory RBAC | `directory::rbac` is registered with scoped mappings, append-only role facts, stale-feed fail-closed behavior, MFA, and break-glass contracts. | Current full-crate compilation/test evidence after integration, live SQLx routed write/read/rebuild tests, universal shared-authorization adoption, and vendor role-mapping acceptance. |
| Directory API/protobuf | A typed Directory service and server registration are present, including connector/import/role-mapping transport markers. | Generated descriptor compatibility against the approved prior descriptor, current server tests, authenticated end-to-end API evidence, and redacted audit/reconciliation runs. |
| Directory worker | `directory_sync` registration is present and intentionally refuses to claim work without a real external adapter. | Production adapter execution, projector/rebuild composition, live worker claims/checkpoints, and certification. |
| Operations | The Operations completion module and executable worker handler branch are present; manager-chain, reassignment, SLA, append-only outbox, projection/reconciliation, and read-only UI/API seams are documented. | Current authoritative source-producer integration, routed SQLx evidence, scheduled detection/reassignment execution, authenticated route/UI journey, manager-chain proofs, and Folio reconciliation. |
| Finance and products | Finance reconciliation runtime wiring and G9–G11 reconciliation/product packets are present. | Broader Finance acceptance, live routed two-database reconciliation, product transport/UI/workflow completion, provider evidence, and accounting/Operations acceptance. |
| G12 UI/QA | Typed directory/platform/product UI state and QA contract registries are present; target-check evidence is documented. | Complete role journeys, native secure storage, device execution, notifications, offline/conflict recovery, accessibility/localization, and current full target verification. |
| Folio bridge | Existing evidence preserves Folio as the write owner and prohibits cutover. | A live Folio probe was unavailable in this review; no command-owner switch, write freeze, read-only retention, or retirement evidence exists. |

## G0–G14 status after this review

| Gate | Status | Reason it is not closed |
|---|---|---|
| G0 | RE-AUDIT REQUIRED | Current-state and compatibility evidence is not a fresh complete audit of the post-integration tree. |
| G1 | OPEN | Restart, durable outbox/projector/rebuild, replacement-read activation, universal boundary adoption, and target evidence are not jointly proven. |
| G2 | OPEN | Directory/RBAC seams are present, but universal effective-access adoption, durable review/recovery delivery, live SQLx evidence, and end-to-end identity integration remain open. |
| G3 | OPEN | Global-read/client, pagination, multi-holding, audit/backfill, revocation reconciliation, and complete authenticated journeys remain open. |
| G4 | OPEN | Independent QA findings remain for approval containment, session/device state, support workflows, projection/notification behavior, universal enforcement, and reconciliation. |
| G5 | OPEN | Live provider/catalog/entitlement reconciliation, official client/API synchronization, replay controls, and complete billing command adoption remain open. |
| G6 | OPEN / BLOCKED | Credentialed MinIO/PITR/restore proof, complete platform operations, scheduler/projector race evidence, and full routed lifecycle acceptance remain open. |
| G7 | OPEN | Operations local behavior and worker wiring are present, but source producers, scheduled execution, authenticated UI/API, authoritative manager proofs, and Folio reconciliation are not proven together. |
| G8 | OPEN / BLOCKED | Finance local seams exist, but consolidation/tax/AP/AR/close/SoD/UI and live two-database reconciliation evidence are incomplete. |
| G9 | OPEN / BLOCKED | Product reconciliation seams exist, but complete Lead-to-Cash/Procure-to-Pay, logistics, transport, scheduling, accounting, UI, and live routed reconciliation are not proven. |
| G10 | OPEN / BLOCKED | Vendor-native/live provider, callback/read-projection, journeys/CRM UI, attribution-to-Finance, and scheduler composition findings remain. |
| G11 | OPEN | Remaining ERP contexts lack complete transport, durable scheduling, UI, MinIO/signed-access, payroll/legal, and end-to-end workflow evidence. |
| G12 | OPEN / BLOCKED | Contract and target-check evidence exists, but native device execution and complete cross-target role journeys are absent. |
| G13 | OPEN | Only read-only/dry-run extraction and ordered cutover contracts are evidenced; no live backfill, shadow replay, reconciliation, rollback rehearsal, owner switch, Folio read-only transition, or retirement is proven. |
| G14 | OPEN | Existing independent QA packages retain scheduler, callback, read-projection, route/race, vendor, device, migration, and Folio-retirement findings. |

## Explicit certification blockers

The directory packages queue, but do not pass, these acceptance runs:

- `CERT-DIR-LDAP-001`: real generic LDAP over validated LDAPS/StartTLS,
  paged import, RFC 4533 continuity, disable/delete, reconnect, rollback, and
  rebuild.
- `CERT-DIR-AD-001`: on-prem AD DS DirSync rights and cookies across restart,
  USN/invocation behavior, objectGUID linking, ACL denial, disable/delete, and
  measured revocation.
- `CERT-DIR-RFC4533-001`: RefreshOnly/RefreshAndPersist, refresh-required,
  deletion, cursor replay, and TLS outage/reconnect.
- `CERT-DIR-UCS-001`: UCS Provisioning Service HTTPS credentials,
  token/sequence restart, replay/order, disable/delete, and outage/reconnect.
- `CERT-DIR-SCALE-001`: one-million-user bounded-memory import with restart,
  duplicate suppression, source/canonical/outbox/read-model reconciliation,
  and staged rollback.
- `CERT-DIR-REVOKE-001`: real-feed disable/delete access cutoff within five
  minutes, stale-feed fail-closed behavior, and append-only revocation proof.

Until these are signed by an environment owner, directory activation remains
disabled and the fail-closed worker behavior is the only safe claim.

## Required next evidence, in order

1. Restore and verify the Folio development server on port 3004, recording the
   authenticated `/operations` redirect without changing Folio ownership.
2. Run serialized current-tree verification: `cargo fmt --all --check`,
   warnings-denied workspace Clippy, workspace tests, all ignored disposable
   database tests, protobuf compatibility comparison, Dioxus web/desktop/
   Android/iOS checks, `bun run lint`, and `bunx tsc --noEmit`.
3. Add current SQLx write/read, outbox, worker/projector, authorization/API/UI,
   reconciliation, and rollback evidence for each affected gate.
4. Execute the queued LDAP, AD DS, UCS, scale, and revocation certifications.
5. Complete G13 migration rehearsal and obtain explicit approval before any
   command-owner switch, Folio write freeze/retirement, directory activation,
   or final production cutover.

## Final conclusion

The latest directory/RBAC/Operations wiring materially improves the tree and
is structurally visible, but this probe found no basis to claim G0–G14 at 100%.
The correct status is **implementation present; gates open pending authoritative
and live evidence**.

Files changed by this review: this README only.

## Coordinator revalidation — current tree — 2026-07-27

The HTTP 000 observation above is historical. A current probe reaches Folio’s
protected `/operations` route on port 3004 and receives the expected HTTP 307
authentication redirect. Current coordinator evidence also passes workspace
formatting, warnings-denied Clippy, workspace tests, ignored local SQLx
targets, protobuf compatibility execution, all Dioxus targets, Folio
lint/typecheck, and authorization-route verification.

This improves runtime and local-evidence status but does not close the review:
credentialed directory/vendor acceptance, device journeys, approved descriptor
input, live Folio migration/rollback/retention/cutover evidence, and explicit
production approval remain unavailable. Historical findings remain open unless
covered by the current authoritative acceptance matrix.
