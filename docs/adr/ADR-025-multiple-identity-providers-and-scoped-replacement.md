# Multiple identity providers and scoped identity replacement

**Accepted — 2026-09-15.** ERPX retains local accounts and permits multiple independently activated AD, Entra ID and Univention UCS connections in a Company. Connecting a directory means authentication and synchronization, not joining the application host to a domain. This supersedes ADR-013's single-connector and identity-replacement/fallback clauses; Holding database isolation and Company lineage remain authoritative.

A Person is independent of their login Principals. Provider identities use directory authority plus immutable object identity; reviewed Person links preserve staff history and independence checks. Email and username are never linking keys. A migration resolves selected users or groups into a fixed batch, independently reviews replacement grants, and supersedes local access only in the selected Company. It does not transfer existing grants, alter historical actor IDs, affect unrelated Companies, or restore local access after group removal or an outage.

Permissions belong to a code-owned catalog. Company-owned custom roles publish immutable, independently reviewed revisions. One selected Assignment supplies permissions for a work context; identity participation and mandatory business conditions remain separate checks. Publication invalidates affected contexts. Matrix explanations and inspection use the same catalog and policy services; a Conditional inspection is never execution authority. Platform permissions occupy a separate namespace and never inherit customer authority.

## Consequences

- Explicit grants expire, inherit a bounded grant ceiling and validity from their parent, and default to at most 90 days. Existing expiry dates remain intact. Revocation and stale policy/identity evidence stop commands and queued work.
- Secrets remain in externally provisioned profiles restricted to approved connection IDs and hosts. Inactive configuration has no sign-in, import, or grant side effects. AD and UCS use certificate-validated LDAPS; Entra and UCS Keycloak use verified OIDC with PKCE.
- A complete directory reconciliation supplies at most five minutes of authorization evidence. Partial imports never mean an empty directory. Failed refresh denies dependent access. Emergency local administrators use audited, time-limited MFA elevation.
- Independent review is checked against the Person, including reviewed linked Principals. Initial customer commissioning must provide independent administrators before grants or migration can be reviewed; the application does not borrow a Platform administrator's authority to bootstrap customer access.
- Additive schema tracks and idempotent command receipts make retries safe. Better Auth remains the session store. Synthetic internal directory login identifiers avoid its unique-email constraint while directory contact email remains separate.

See the [implementation and operations contract](../contracts/next/identity-access.md) for provider provisioning, recovery, migration and verification.
