# G2-001-WEBAUTHN-PROVIDER-006

## Delivered

The identity crate now contains a real browser-ceremony provider backed by
`webauthn-rs = 0.5.5` with the `danger-allow-state-serialisation` feature.
The provider uses the maintained high-level `webauthn-rs` API for:

- registration response parsing and attestation verification;
- assertion response parsing and signature verification;
- configured RP ID and origin validation;
- persisted provider-state binding to the ERPX challenge digest;
- credential-ID binding;
- authentication counter extraction for the existing atomic monotonic update.

Malformed JSON, oversized response/state payloads, wrong ceremony state,
wrong ERPX material, wrong origin, mismatched credential IDs, malformed stored
credentials, and legacy requests without browser ceremony data fail closed.

## Dependency

Pinned direct dependency:

```toml
webauthn-rs = { version = "=0.5.5", features = ["danger-allow-state-serialisation"] }
```

The feature is required only to serialize the provider's registration and
authentication state for server-side persistence. It must never be used for
client-controlled cookie state.

## Verification

```text
cargo fmt -p erpx-identity -- --check
cargo clippy -p erpx-identity --all-targets -- -D warnings
cargo test -p erpx-identity
```

Focused tests cover deterministic malformed browser responses, state and
origin tampering, legacy fail-closed behavior, RP/origin policy, challenge
replay/expiry, credential binding, revocation, and sign-counter rollback.
The disposable PostgreSQL tests remain ignored when no test database is
configured.

The positive deterministic vector exercises packed attestation parsing and
cryptographic verification. The assertion path is wired to
`finish_passkey_authentication` and rejects malformed assertions, binding and
counter failures; a positive browser assertion fixture is still required in a
later integration test because this packet does not add a live ceremony-state
repository or browser transport fixture.

The default `webauthn-rs` passkey registration flow uses attestation conveyance
`none`. It verifies the attestation object and credential proof, but this
packet does not establish a trusted manufacturer/AAGUID CA policy. A deployment
that requires attested-device allowlisting must add the attested registration
flow and its CA policy before relying on that control.

## Open integration boundary

The existing `lib.rs` exports and server HTTP DTOs do not yet expose or carry
the new browser request/state types. Main must add a safe server-side
ceremony-state repository keyed by ERPX challenge ID, re-export the provider
types, and ensure browser clients never submit provider state directly.
Only after that integration may the provider be used by production routes.

No live production route or overall ERPX completion claim is made by this
packet.
