# Directory authentication preflight — 2026-07-28

## Scope

This bounded identity-lane update covers only
`crates/identity/src/directory/auth.rs` and its colocated tests. It does not
register a server route, activate a directory, change shared authorization,
or claim a vendor acceptance pass.

## Authoritative fail-closed checks

`DirectoryConnectorConfig::preflight` is now the construction gate used by
`DirectoryAuthenticator`. It records and enforces, for Generic LDAP,
on-premises AD DS, and UCS:

- exact LDAPS or StartTLS transport policy before a bind can receive
  credentials;
- the provider's native feed and immutable identity attribute (LDAP/UCS
  `entryUUID`, AD DS `objectGUID`);
- MFA required for every high-risk operation;
- explicit `Disabled` or `LocalOnly` break-glass policy, with automatic local
  fallback disabled; and
- external certification still required, so configuration cannot be reported
  as live LDAP, AD DS, or UCS acceptance.

`DirectoryTransport::preflight` is a second, adapter-owned gate. A transport
that lacks credentialed endpoint/CA/vendor readiness must return the opaque
`ExternalCertificationRequired` error; the authenticator then refuses to call
`bind`, so credentials cannot be sent to an uncertified or unavailable
environment.

The authentication path continues to validate adapter-produced certificate
evidence, bind the identity to the connector-scoped immutable key, reject
disabled/locked accounts, and accept local break-glass only from a validated
loopback peer through the explicit local method.

## Verification

The new identity unit tests cover all three providers across LDAPS/StartTLS,
provider/feed/immutable-key closure, high-risk MFA, local-only break-glass,
rejection of a mismatched plaintext endpoint before transport binding, and
the external-readiness gate blocking bind before credentials are sent.
Credentialed Generic LDAP/RFC 4533, AD DS/DirSync, and UCS Provisioning
Service environments were unavailable; those certification runs remain
queued and no vendor acceptance is claimed.

No production activation, command-owner switch, Folio write retirement, or
cutover was performed.
