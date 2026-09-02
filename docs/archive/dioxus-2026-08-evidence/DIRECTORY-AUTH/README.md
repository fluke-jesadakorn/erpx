# Directory authentication handoff — ERPX G2/G14

## Scope completed in this packet

The registration-ready policy boundary is in:

- `crates/identity/src/directory/auth.rs`
- `crates/identity/src/directory/tls.rs`

It covers generic LDAP, on-premises Microsoft Active Directory Domain
Services, and Univention Corporate Server connector metadata and bind policy.
The same contract supports platform and holding connector scopes. Provider
identity keys are fixed to generic LDAP `entryUUID`, AD DS `objectGUID`, or
UCS `entryUUID`; the resulting `erpx:directory-link:<connector>:<provider>:<uuid>`
reference is immutable and contains no mutable DN, username, display name, or
email.

The boundary enforces:

- LDAPS or StartTLS only, with an exact endpoint scheme and mandatory
  certificate-validation evidence. Plain LDAP, `ldapi`, validation-disabled
  handshakes, wrong names, old TLS, and stale trust anchors fail closed.
- Monotonic trust-anchor rotation and bounded connect, StartTLS, and bind
  timeout policy. The authentication service also bounds the complete bind
  future.
- Zeroized, non-`Debug`/non-`Display` bind credentials and an opaque error
  taxonomy that never carries adapter or credential text.
- Native-feed declarations tied to the provider: RFC 4533 for generic LDAP,
  AD DirSync for AD DS, and UCS Provisioning Service for UCS. Feed execution
  and full import are owned by the synchronization lane, not faked here.
- High-risk access only after an identity- and operation-bound, short-lived
  MFA step-up receipt. Missing, unbound, expired, or rejected receipts fail
  closed.
- A separate, explicitly invoked local break-glass method. Directory failure
  never calls it; it is available only when the connector is configured
  `LocalOnly`, and normal local credentials are not represented by this API.

## Required integration hooks

The coordinator/registration owner must wire the following without changing
the policy files above:

1. The current identity module is rooted at `src/directory.rs`. Register the
   new files as its child modules (or re-export an equivalent bridge) and
   expose the contract types needed by the server and worker. This packet does
   not edit that shared registration file.
2. Add a SQLx write-side migration owned by the migration lane. Persist one
   immutable connector record keyed by `connector_ref`, including platform or
   holding scope, provider, TLS mode/server name, trust-anchor reference,
   fingerprint and generation, secret-manager reference/version, bounded
   timeouts, native-feed kind, and break-glass policy. Store secret references,
   never bind passwords.
3. Persist immutable external identity links keyed by the complete canonical
   link reference. Enforce unique `(connector_ref, provider, stable_object_key)`
   and reject updates/deletes through database policy. A username/DN change
   must not create a new link; a stable-key collision with another link must
   be an audited conflict and fail closed.
4. In the same authoritative SQLx transaction as the accepted auth decision,
   append a redacted directory-auth event/outbox row. Include connector,
   holding/platform scope, immutable link, result category, correlation and
   actor references, trust-anchor generation, and MFA/break-glass decision;
   never include passwords, bind packets, raw LDAP errors, or MFA assertions.
   The worker/projector must consume this event idempotently into a rebuildable
   read model and retain reconciliation evidence.
5. Register server routes and shared authorization at the coordinator-owned
   boundary. High-risk authorization must require the `StepUpReceipt` to match
   the immutable link and operation. Directory revocation must be appended as
   a durable role-revocation event and enforced within the five-minute
   revocation SLO; this packet does not bypass the shared authorization owner.
6. Register connector workers for bounded full imports (up to 1,000,000 users
   per connector) and native change feeds: RFC 4533, AD DirSync, and UCS
   Provisioning Service. Workers must use cursor/checkpoint persistence,
   idempotent outbox publication, projection/reconciliation, and a forward-only
   rollback/disable procedure. Do not activate a customer directory or retire
   Folio writes without explicit product-owner approval.
7. Wire the concrete adapter so it applies `TimeoutPolicy::connect` before
   opening a socket, `TimeoutPolicy::starttls` for the StartTLS upgrade, and
   `TimeoutPolicy::bind` for the bind. It must use a certificate-validating
   TLS implementation, return `TlsHandshakeEvidence`, and never retry with a
   plaintext or validation-disabled mode.

## Certification status and rollback

No external LDAP, AD DS, or UCS environment/credential is available in this
workspace. Vendor acceptance is therefore **not claimed**. Queue the following
certification blocker for an environment-owner run:

- generic LDAP: LDAPS and StartTLS certificate-chain/name/rotation tests,
  RFC 4533 cookie continuity and revocation;
- AD DS: LDAPS/StartTLS bind, objectGUID linking, account lock/disable,
  DirSync cookie/invocation-ID behavior, and native revocation;
- UCS: LDAPS/StartTLS bind, entryUUID linking, UCS Provisioning Service
  reconnect/replay behavior, and native revocation.

Rollback is forward-only after persistence is wired: disable the connector and
its worker, stop issuing new directory sessions, preserve auth/outbox/link
history, and replay the prior read model. Do not delete immutable links or
erase audit evidence. Before production data exists, revert only the
registration/migration packet that introduced the connector tables through the
normal migration owner; this auth packet itself has no database side effect.

## Local verification

Focused policy tests are colocated in the two Rust files. Once the coordinator
registers `directory`, run:

```text
cargo test -p erpx-identity directory
cargo fmt --all --check
```

The required full workspace verification remains coordinator-owned, including
warnings-denied Clippy, workspace and ignored database tests, protobuf
compatibility, Dioxus targets, `bun run lint`, and `bunx tsc --noEmit`. No
external vendor acceptance test was run here.
