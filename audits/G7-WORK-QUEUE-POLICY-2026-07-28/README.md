# G7 WorkQueuePolicy publication — 2026-07-28

## Authoritative path

`PublishOperationsWorkQueuePolicy` requires an authenticated actor, a
contained holding scope, and `operations.manage`. The server routes through
the active holding registry and the SQLx adapter binds the policy to the
holding. The write path uses the existing `operations_aggregate_snapshots`,
`operations_events`, holding outbox, and `operations_source_references`
contracts; the read path advances the existing `operations_read_model`
projection. No policy table or client-side state was introduced.

The adapter enforces bounded names/domains and SLA minutes, optimistic
expected versions, command idempotency, immutable event identity, and
projection replay. The corresponding list RPC returns bounded scalar policy
fields only. The Dioxus console renders the authoritative policy page and
submits publication commands to the server.

## Evidence

Fresh disposable PostgreSQL write/read databases passed:

- create and publish at aggregate version 1;
- read projection with active status and canonicalized source domains;
- idempotent replay without a duplicate event;
- optimistic update at version 2;
- bounded JSON policy listing; and
- authoritative event count of two.

Focused console wire-boundary tests passed, and `cargo check -p erpx-server
--offline` / `cargo check -p erpx-console --offline` passed. The exact
temporary databases were dropped after the run.

## Rollback and open evidence

Policy events are append-only. A stale update is rejected; rollback is an
independently authorized subsequent policy publication or a rebuild of the
disposable read projection from immutable events. This packet does not
activate a customer directory, switch Folio command ownership, retire Folio
writes, or perform production cutover.

Provider/device delivery, Folio parity, credentialed directory/vendor
certification, physical mobile execution, PITR/WAL proof, and production
approval remain queued.

## Manager authority continuation

The Operations exception transition boundary now obtains the assignee's
complete effective role-assignment chain from the control-plane write store.
Assign, reassign, and escalate require the supplied manager to be the
authoritative root and require that root to carry `operations.exception.manage`
or `operations.manage`. `OperationsManagerChainProof` validates the full
chain at the command timestamp, including active windows, scopes, delegation
links, authority levels, and permission preservation. This closes the prior
gap where assignee and manager were checked only as independent active
principals. A domain test covers both an accepted authorized root and a
rejected read-only root.

## Lifecycle MFA continuation

The `TransitionOperationsException` protobuf now carries an ephemeral
`mfa_code`. Assign, reassign, and escalate are rejected unless the server
performs fresh write-side TOTP verification before holding routing and
aggregate mutation. The console treats the value as a password/one-time-code
field; it is not included in command context, events, receipts, or read
models. Acknowledge and resolve remain lower-risk transitions.

## Source producer continuation

`CreateOperationsExceptionFromSource` is now the authenticated internal
producer boundary for source-context exception facts. It requires exact actor
binding, contained holding scope, `operations.exception.manage`, bounded source
and event identities/versions, RFC3339 occurrence time, and bounded valid JSON.
The server derives the holding reference from the active holding registry and
uses the existing SQLx source-provenance, aggregate, event, outbox, and read
projection path. Operations stores source identity and orchestration state;
the upstream context retains source transaction and payload ownership. This
internal producer RPC has no client UI surface.
