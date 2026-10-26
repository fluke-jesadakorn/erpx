# Native Account migration register

This register describes the current Rust implementation, not acceptance of the complete Identity owner. The existing [Identity contract](identity-access.md), [Holding address contract](holding-addresses.md), [active work context](active-work-context.md) and [production plan](../../production-plan.md) remain the parity requirements. Native routes are not mounted in the shared development or installed production service. No provider, live account, deployment or public release is changed by the source implementation.

## Implemented source and wire contracts

The Account core is private to `packages/server-identity_access/src/domain` and `application`. Its ports contain no SQLx, Axum or runtime imports. The PostgreSQL, bounded password hashing and HTTP adapters implement those ports; the composition crate must assemble the public facade using an explicitly selected Control database. A successful sign-in alone does not authorize a Holding business request.

| Surface | Native source | Current state |
| --- | --- | --- |
| Email credential sign-in | `account_auth`, `account_crypto`, `account_postgres`, `account_http` | Implemented for fresh Argon2id credentials; exact Account Host and Origin, strict JSON, nonce reservation, live credential recheck, principal and verification policy |
| Invitation credential creation | `account_credentials`, `account_credentials_postgres` | Fresh credential transaction implemented; requires an injected current owner authority lease. The default denies; actual IAM/People authority assembly and Holding grant activation remain pending |
| Verification and password reset | `account_credentials`, `account_credentials_http` | One-hour, single-use hashed proofs, supersession, enumeration-safe request responses, private encrypted mail outbox, current credential/principal recheck and atomic audit |
| Verification sign-in | Shared session issuance helper | Verification consumes its proof and issues a normal Account/device-bound session in one UoW; stale revision or session failure rolls back verification and proof consumption |
| Current password change | `account_credentials_postgres` | Requires current Account proof and password verification; rechecks the credential before mutation, revokes all affected sessions and records immutable audit |
| Account session read | `account_sessions` | Implemented; current browser selection, live parent binding, expiry and current principal |
| Saved device accounts | `account_postgres`, `account_http` | Implemented; maximum five, separate opaque saved-cookie proof per handle, captured revision, nonce exclusion, serialized compare-and-set |
| Sign-out and removal | `account_postgres` | Implemented; Holding sign-out revokes only its child, Account sign-out removes its active family, explicit removal and switching invalidate child sessions |
| Holding child session proof | `account_sessions` | Read verification implemented; exact audience, environment, current parent selection and twelve-hour bounds; child issuance and SSO exchange remain pending |
| Borrowed request session proof | `public::account_sessions` | Implemented for a caller-owned Control lease; retains read locks and never starts, retries, commits or closes the lease |
| TypeScript and OpenAPI | [generated HTTP description](account-auth.openapi.generated.json) | Generated from Rust DTOs and implemented HTTP handlers; not runtime acceptance |
| Next.js Account client | `feature-identity_access` application port and HTTP adapter | Five existing caller groups use typed HTTP; saved-account selection carries a handle and revision, never a session token |

Passwords use Argon2id version 19 with 64 MiB memory, three iterations, one lane, a 32-byte output and a fresh salt. Password work runs outside database leases and async executor threads, with an explicit one-to-eight worker bound. Missing users run the same algorithm against a private dummy hash. No BetterAuth or archived password-hash fallback is provided; the approved clean seed supplies fresh credentials with the explicit `local:credential` issuer. A nullable legacy issuer or Directory credential is not accepted as a native local password credential.

Raw session, browser and reservation secrets use 32 random bytes and separate SHA-256 domains. Only digests enter database session/device fields. Host-only cookies are Secure, HttpOnly and SameSite=Lax; JSON contains display data and opaque handles. Each saved account requires its own cookie proof. The browser grouping cookie alone grants no account identity. Device revisions are closed integer values from zero through JavaScript's maximum safe integer; unsafe inbound and outbound values fail closed. IDs remain strings and dates use RFC3339 strings. No global conversion policy for future money or integer contracts is introduced.

`0041_native_account_auth.sql` adds native credential proofs, the encrypted mail outbox and immutable credential events. Reset, verification and invitation tokens have distinct digest domains. AES-256-GCM protects queued mail payloads using random 96-bit nonces and authenticated key identifiers and proof IDs, preventing ciphertext from being substituted into a different proof row. An explicit private keyring supplies up to four keys, allowing retained ciphertext to survive rotation. There is no default key, environment fallback, plaintext link in the outbox or live provider invocation. Proof consumption, credential updates, session revocation, outbox cancellation and audit share the same Control UoW. A failed audit or stale device selection rolls back all effects.

Signup remains invitation-only. A Control invitation row alone grants nothing. The `InvitationCredentialAuthority` port must return a retained current owner lease for the exact Holding, Company, Person and email, with the original independent review, published role, HR eligibility, authorizer `iam.manage` authority and parent scope. Repository code retains that lease until Control commits, including retries. Only the reviewed invitation name supplies the initial account display name. Existing or Directory identities use their verified linking flow and cannot create a replacement local credential through this path. `accepted_at` is left unset until the separate Holding access activation succeeds; creating a credential does not create an assignment.

## Explicit remaining parity

| Remaining surface | Required parity before native owner acceptance |
| --- | --- |
| Invitation owner assembly and access activation | Supply the real retained IAM/People authority adapter, complete original grant/Person-link transaction witnesses and only then finalize acceptance. The unconfigured authority port fails closed |
| Private mail delivery worker | Implement the native delivery claim/current-source recheck, original retry/backoff/lease recovery and provider outcome; commands are durably queued and no delivery success is fabricated |
| SSO issuance/exchange | State, PKCE/challenge, exact registered host and callback, browser transaction and single-use grant consumption, parent/device revision fences and current session family |
| Directory login/provisioning | Exact issuer and immutable provider identity, current source freshness and approvals, private secret profiles, reviewed mappings, provider failure and revocation rules |
| Effective Holding authorization | Current assignment/role lineage is necessary but insufficient: complete IAM, HR eligibility, GPO, emergency/fresh-MFA, same-Person independence and original owner record policy must execute in the retained request transaction |
| Account security and administration | MFA enrollment/challenge/recovery, session inventory/revocation, profiles, preferences, invitations, access reviews, role/matrix/inspector, directory administration and account lifecycle |
| Queued authority and notifications | Source witnesses, current-policy recheck, idempotency, durable outbox and provider publication boundaries |
| Environment activation | Current immutable SSO schema allows the `development` binding only. Native policy fails closed for other environments until an explicit schema/configuration transition is implemented and tested |
| Abuse and abandoned work | Per-source abuse controls, cancelled password-work/abandoned nonce cleanup, delivery throttles and durable housekeeping still need production parity review |
| Source removal and release | Retire remaining TypeScript owner implementation and exports only after equivalent native flows and typed clients pass; then complete browser, isolated build, exact-source CI and release gates |

The generated HTTP description includes implemented signup, reset, password-change and verification handlers. It does not establish availability of a delivery worker or current invitation owner adapter. No compatibility fallback supplies those gaps. The shared development service remains on its existing backend until a separate coherent cutover is ready.

## Verification

The canonical suites are `identity_access.rust-account-auth.unit`, `identity_access.rust-account-auth-postgres.integration`, `identity_access.rust-account-recovery.unit`, `identity_access.rust-account-recovery-postgres.integration` and `identity_access.account-auth-http.unit`. PostgreSQL tests use the guarded disposable `erpx_dev_rust_*` fixture and immutable Control migrations, never production or staging databases. Cases cover credential proof, closed JSON/cookies, exact authority and CORS, saved-proof isolation, stale parallel selection, principal/parent/audience bounds, nonce cleanup, sign-out scope, single-use/superseded mail proofs and failed-audit rollback. Positive signup repository tests explicitly inject a reviewed test authority; they do not certify the pending production IAM/People adapter. These results are development verification, not full owner, browser, provider or production acceptance.

Regenerate both contracts with `bun scripts/docs/generate-account-contracts.ts`; verify with the same command plus `--check`. The exporter uses the pinned Rust workspace and locked dependencies. Do not edit generated TypeScript or OpenAPI output by hand.
