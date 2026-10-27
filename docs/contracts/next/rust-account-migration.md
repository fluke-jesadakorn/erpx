# Native Account migration register

This register describes the current Rust implementation, not acceptance of the complete Identity owner. The existing [Identity contract](identity-access.md), [Holding address contract](holding-addresses.md), [active work context](active-work-context.md) and [production plan](../../production-plan.md) remain the parity requirements. Native routes are not mounted in the shared development or installed production service. No provider, live account, deployment or public release is changed by the source implementation.

## Implemented source and wire contracts

The Account core is private to `packages/server-identity_access/src/domain` and `application`. Its ports contain no SQLx, Axum or runtime imports. The PostgreSQL, bounded password hashing and HTTP adapters implement those ports; the composition crate must assemble the public facade using an explicitly selected Control database. A successful sign-in alone does not authorize a Holding business request.

| Surface | Native source | Current state |
| --- | --- | --- |
| Email credential sign-in | `account_auth`, `account_crypto`, `account_postgres`, `account_http` | Implemented for fresh Argon2id credentials; exact Account Host and Origin, strict JSON, nonce reservation, live credential recheck, principal and verification policy |
| Invitation credential creation | `account_credentials`, `account_credentials_postgres` | Fresh credential transaction and actual retained IAM/Organization/People/review authority adapter implemented. Separate ordered activation links the Person and creates the exact reviewed grant before finalizing Control; unconfigured authority still denies |
| Verification and password reset | `account_credentials`, `account_credentials_http` | One-hour, single-use hashed proofs, supersession, enumeration-safe request responses, private encrypted mail outbox, current credential/principal recheck and atomic audit |
| Native Account mail outbox | `account_mail` core, PostgreSQL and optional SMTP/Resend adapters | Encrypted payload opening, six-attempt claim/retry/lease recovery, retained current-proof dispatch and durable provider receipts implemented; transports exercised only against local fake sinks |
| Verification sign-in | Shared session issuance helper | Verification consumes its proof and issues a normal Account/device-bound session in one UoW; stale revision or session failure rolls back verification and proof consumption |
| Current password change | `account_credentials_postgres` | Requires current Account proof and password verification; rechecks the credential before mutation, revokes all affected sessions and records immutable audit |
| Account session read | `account_sessions` | Implemented; current browser selection, live parent binding, expiry and current principal |
| Saved device accounts | `account_postgres`, `account_http` | Implemented; maximum five, separate opaque saved-cookie proof per handle, captured revision, nonce exclusion, serialized compare-and-set |
| Device HTTP facade | `account_device_http`, shared credential handler | Existing `{data: DeviceAccountsDto}` envelope on discovery, initialization, drawer sign-in, selection and removal; exact registered Holding/Account origins on the Account host, credentialed CORS, host-only cookies and private responses |
| Native browser-group recovery | `account_group_recovery`, `0043_native_device_group_keys.sql` | Fresh opaque key rotation for a group independently proven by native parent cookies, original revision/nonce checks, active-parent priority, two-Holding child fences and no cross-group union; read-only discovery and strict typed initialize body |
| Sign-out and removal | `account_postgres` | Account or Holding sign-out revokes the current parent family; independent saved accounts/devices survive. Inactive-account removal revokes only its own family and preserves the current account's Holding children |
| Cross-Holding retirement | `account_revocation` port and PostgreSQL adapter | Mandatory trusted writer bindings, retained non-retrying Control command, ordered idempotent Holding tombstones and exact context deletion before acknowledgement; partial failure leaves earlier fences committed and requires normal SSO re-entry |
| Cross-Holding browser SSO | `account_sso` domain/application/PostgreSQL/HTTP | Exact registered Holding/Account redirects, five-minute browser proof, sixty-second grant, state plus S256 PKCE, atomic one-time consumption and hashed native child session |
| Holding child session proof | `account_sessions` | Exact audience/environment/current parent selection, original authentication time and minimum parent/child expiry; SSO never refreshes MFA age |
| Borrowed request session proof | `public::account_sessions` | Implemented for a caller-owned Control lease; retains read locks and never starts, retries, commits or closes the lease |
| Authenticator enrollment and verification | `account_mfa`, `account_mfa_crypto`, `account_mfa_postgres`, `account_security_http` | Existing six-digit SHA1 TOTP, durable failed-attempt throttles, atomic user-global counter CAS and exact-session freshness, explicit encrypted keyring and current-key rotation; no login challenge or recovery-code feature is invented |
| Personal security inventory and revocation | `account_security_snapshot`, `account_security_postgres` | Existing opaque account/family handles, unchanged Node revision projection, captured revision, current exact-session MFA before receipt replay, immutable audit and mandatory ordered two-Holding fences |
| Retained fresh-MFA evidence | `public::account_security` | Sealed non-serializable evidence requires the actual Control session/audience, usable envelope and exact current database/backend/transaction; `require_current` rechecks original MFA image and time at the consumer's final boundary. It grants no Company or Platform permission |
| TypeScript and OpenAPI | [generated HTTP description](account-auth.openapi.generated.json) | Generated from Rust DTOs and implemented HTTP handlers; not runtime acceptance |
| Next.js Account client | `feature-identity_access` application ports and HTTP adapters | Existing credential/device/activation and MFA/personal security callers use generated contracts and closed response validation. The Account shell consumes the named Identity client facade; saved-account selection carries a handle and revision, never a session token. Other server-rendered Account callers remain part of cutover |

Passwords use Argon2id version 19 with 64 MiB memory, three iterations, one lane, a 32-byte output and a fresh salt. Password work runs outside database leases and async executor threads, with an explicit one-to-eight worker bound. Missing users run the same algorithm against a private dummy hash. No BetterAuth or archived password-hash fallback is provided; the approved clean seed supplies fresh credentials with the explicit `local:credential` issuer. A nullable legacy issuer or Directory credential is not accepted as a native local password credential.

Raw session, browser and reservation secrets use 32 random bytes and separate SHA-256 domains. Only digests enter database session/device fields. Host-only cookies are Secure, HttpOnly and SameSite=Lax; JSON contains display data and opaque handles. Each saved account requires its own cookie proof. The browser grouping cookie alone grants no account identity. Device revisions are closed integer values from zero through JavaScript's maximum safe integer; unsafe inbound and outbound values fail closed. IDs remain strings and dates use RFC3339 strings. No global conversion policy for future money or integer contracts is introduced.

`0043_native_device_group_keys.sql` separates the stable native browser ID from the rotating purpose-separated cookie digest. The migration backfills only existing 64-character native digest IDs; signed UUID/BetterAuth groups are not upgraded. Once a key rotates, the previous digest is revoked and there is no direct browser-ID fallback or stored digest used as a raw cookie. Recovery retains the original parent rows, absolute expiry, authentication time, membership and current selection. The new cookie is freshly generated and is returned only after the non-retrying Control command and all required Holding child fences succeed. Every retired child is identified from the locked native binding; unrelated browser groups and their work survive.

This is an explicit reviewed native security difference from the B0 signed-UUID implementation. The unchanged source witness is `a5ed8737a2f05eff1ee9841fda4a094b3926d732:packages/server-identity_access/src/adapters/device-account-store.ts` (Git blob `a91f8e6c59c0b85c5e5d25c10d136441dbc5b099`). B0 can re-sign a stored UUID and may merge independently proven memberships. Native recovery never reconstructs an opaque secret and never unions groups. A proven, currently selected active native parent chooses only its existing group; without that proof, saved cookies must identify exactly one group or discovery/recovery returns 409. Display rows still require their own independent live parent proof. A grouping key alone grants no identity.

Device GET remains free of writes and `Set-Cookie`. When the grouping key is absent or revoked but a group is independently recoverable, it returns `initialized: false`, that group's current revision and only its proven display rows. This supplies a real compare-and-set value to the existing picker. POST initialization takes the strict generated `DeviceInitializeInput`; recovery requires the captured revision, while anonymous initialization may still submit `{}`. Pending sign-in nonces and stale revisions reject rotation. Successful recovery advances the original revision once and retires every old group child before acknowledgement; it never refreshes parent authentication or selects a different saved account. Lost `Set-Cookie` acknowledgements require another GET and a fresh revision before another fresh secret can be issued. The old secret is never replayed.

For ambiguous saved-only proofs, ordinary email/password sign-in has a separate server-selected enrollment policy. The existing ordinary form sends no device revision; an absent revision or explicit zero denotes a fresh candidate's initial state. Any supplied old-group revision is rejected, and no client `ignoreProof`, group selection or enrollment flag is accepted. The server reserves a new empty native group with its own revision-zero nonce, completes normal bounded password/current-credential verification, and enrolls only that newly authenticated account with a fresh parent/session/key. A failed password deletes the empty candidate and leaves every old parent, key, membership, nonce and Holding child intact. Successful enrollment never adopts, selects, extends or retires an old group; old saved-cookie proofs cannot select accounts across the new group boundary. Valid single-use email verification applies the same fresh enrollment policy after validating its proof. The implicit GET/initialize paths still reject ambiguity without mutation. This additional reviewed security difference preserves an ordinary sign-in recovery path without cookie clearing or a permission-granting fallback.

Sign-in recovery pins the original group revision, nonce and proof lifetime during reservation, performs Argon2 outside database leases, and rotates only inside successful credential completion. A failed password releases only its exact nonce and leaves keys, revision and Holding work intact. Verification uses the same group resolution inside its existing single-use Control command. Both paths recheck proof/reservation expiry after Holding waits. A partial Holding failure or late Control failure sends no cookie or successful data; committed Holding tombstones remain monotonic while Control key/revision changes roll back. Retry never replays a previously committed Holding effect through an automatically retried Control callback. Account session GET continues to require the current group key, with no authentication fallback added by discovery.

`0041_native_account_auth.sql` adds native credential proofs, the encrypted mail outbox and immutable credential events. Reset, verification and invitation tokens have distinct digest domains. AES-256-GCM protects queued mail payloads using random 96-bit nonces and authenticated key identifiers and proof IDs, preventing ciphertext from being substituted into a different proof row. An explicit private keyring supplies up to four keys, allowing retained ciphertext to survive rotation. There is no default key, environment fallback, plaintext link in the outbox or live provider invocation. Proof consumption, credential updates, session revocation, outbox cancellation and audit share the same Control UoW. A failed audit or stale device selection rolls back all effects.

Signup remains invitation-only. A Control invitation row alone grants nothing. The `InvitationCredentialAuthority` port must return a retained current owner lease for the exact Holding, Company, Person and email, with the original independent review, published role, HR eligibility, authorizer `iam.manage` authority and parent scope. Repository code retains that lease until Control commits, including retries. Only the reviewed invitation name supplies the initial account display name. Existing or Directory identities use their verified linking flow and cannot create a replacement local credential through this path. `accepted_at` is left unset until the separate Holding access activation succeeds; creating a credential does not create an assignment. The real adapter resolves only a trusted Holding pool, validates the original authorizing assignment and canonical business-role module resource, binds the exact independently reviewed invitation/target/scope/expiry/published revision, and uses the current published permission ceiling. Current Organization lineage and scoped People engagement evidence are borrowed through owner public APIs. Missing, stale, suspended, same-Person, Directory or existing-account facts fail closed. A final time/policy recheck runs while Holding source locks remain retained through the credential commit.

The native invitation acceptance workflow preserves the existing `/api/v1/account-invitations` inspection and activation envelopes. Password verification occurs outside database leases. A resumable attempt must prove the password from its first credential creation; it cannot replace that password while retrying. A retained, non-retrying Control read lease protects the live invitation, credential, principal and Holding binding around a retrying Holding unit. The Holding unit writes the exact grant, immutable audit, command receipt and People-owned Person/legacy employee link together. The People adapter enforces Company/Person/version plus a current scoped engagement witness, refuses link stealing, preserves same-user idempotence, and uses the caller's transaction. Control acceptance finalization follows the durable Holding commit in its own Control-only unit. A later Control failure never replays that Holding commit. Resume checks the original receipt, current exact grant/module/scope/expiry and Person link; it never recreates a revoked or edited grant. This is an ordered two-database protocol, not a distributed atomic commit. Interrupted workflows that become expired before finalization still require a bounded reconciliation/operations policy; no worker completion is claimed.

`0042_native_account_mail_delivery.sql` adds immutable attempt events, durable dispatch intent, provider/profile identity, an exact message fingerprint and dispatch lease digest. Claim and ACK are Control-only retryable units; provider invocation occurs once in a retained, non-retrying current-proof lease. Missing, superseded, consumed, expired, suspended, changed-recipient or changed-credential evidence cannot send. The current six-attempt exponential backoff is bounded by proof expiry. A stale lease cannot acknowledge a newer attempt or revive a cancelled proof. If cancellation follows actual provider acceptance, the original matching dispatch can record that observed receipt while preserving cancellation.

Both providers require an actual acceptance receipt: a valid Resend response ID or a positive SMTP server response. The outbox `sent` state records transport acceptance, not a recipient read or a delivery/bounce guarantee. An ambiguous SMTP outcome is terminal and cannot automatically resend. Resend recovery reuses the same key and exact provider/sender/recipient/content fingerprint within its [24-hour idempotency window](https://resend.com/docs/dashboard/emails/idempotency-keys), with the one-hour proof lifetime imposing the shorter bound. Provider credentials are bound into an opaque profile identity; a changed provider identity or message fails closed. Encryption-key rotation can retain old queued envelopes through the bounded keyring; losing the required key never invokes a provider.

`mail-resend` and `mail-smtp` are explicit optional features; `mail-providers` selects both for provider verification. The default/API dependency tree adds neither transport. Resend fixes the external endpoint, uses an explicit Ring/Rustls trust configuration, disables ambient proxies, redirects and implicit HTTP retries, and bounds time and response bodies. SMTP production construction permits implicit TLS or required STARTTLS; [Lettre's required STARTTLS contract](https://docs.rs/lettre/0.11.23/lettre/transport/smtp/struct.AsyncSmtpTransport.html#method.starttls_relay) refuses downgrade before authentication or message submission. Plain local test constructors accept only literal loopback, non-protected ports. There is no successful default transport, plaintext production fallback, environment credential discovery or live send in verification.

The native SSO wire preserves the existing 43-character URL-safe state, challenge, code and browser/verifier proof used by the Account login continuation. The verifier remains in the Holding's Secure, HttpOnly, SameSite=Lax host-only transaction cookie. Central credential cookies never enter a Holding callback, URL or client storage. A callback consumes its browser transaction and grant in the same Control UoW as child creation; invalid state, PKCE, host, callback, expiry, current parent selection or principal leaves valid proofs unconsumed. Return paths retain repeated Company/filter parameters, normalize path segments and remove callback credentials/fragments. The [RFC7636 Appendix B](https://www.rfc-editor.org/rfc/rfc7636#appendix-B) public S256 vector supplies an independent cryptographic oracle.

The sign-out oracle is the reviewed TypeScript route chain at R2 `b4db260cd029707886e82ac958daab3b4fab1509`: workplace auth POST calls `productSignOut`, which invokes `revokeFamily`; its original deletion primitive resolves a child to its parent and deletes that entire current family. The earlier native test asserting parent preservation was an incorrect oracle and has been corrected. No user decision changed the family policy. The native implementation preserves other saved parents and independent devices.

Native Account and credential adapters now require an explicit `AccountWorkContextInvalidation` dependency and clock. There is no default or legacy constructor that skips Holding retirement. Under the original Control session-first fence, the adapter captures exact child session/user/environment identities and current active Holding writer bindings. The production invalidator resolves only its trusted configured writer map, rejects absent or changed bindings before any Holding write, and serializes `iam_revoked_sessions` plus `active_work_contexts` in the same order as the current queued authority policy. Every affected Holding commits its monotonic tombstones and deletes only matching contexts before Control can acknowledge selection, removal, sign-out, reset, password change or verification selection. Selecting an already-current saved account preserves the existing revision and contexts, matching the TypeScript no-op policy.

These commands use `PgUnitOfWork::run_once`: the outer Control callback and commit are never automatically replayed after a Holding commit. Only each self-contained Holding unit may retry. A later Holding failure, Control rollback or expired reservation/proof leaves any earlier Holding fences in place; it cannot acknowledge a new selection, resurrect a grant or undo that fence. Current time is sampled after Holding waits before the final credential/selection result. Recovery uses a normal fresh SSO transaction. This is an ordered fail-closed protocol, not a distributed atomic commit. Control-only focused fixtures explicitly inject a recording test port; the separate real revocation suite uses Control plus two distinct disposable Holding databases and the actual native Auth/SSO/invalidation adapters. This source implementation does not activate the shared gateway, installed service or jobs runtime.

The authenticator source oracle is the unchanged B0 `a5ed8737a2f05eff1ee9841fda4a094b3926d732` MFA domain/application/adapter and personal session-security adapter. [The retained Node fixture](../../../fixtures/identity_access/account-security/conformance.v1.json) records each source SHA256 and Node 26.11.1 output; its generator executes actual source function bodies, with explicit fake ports for application rule observations. Independent [RFC4226](https://www.rfc-editor.org/rfc/rfc4226) and [RFC6238](https://www.rfc-editor.org/rfc/rfc6238) vectors cover the existing SHA1 protocol. The already locked Ring implementation supplies HMAC-SHA1 only for this existing TOTP compatibility rule; it is not selected for signatures. Secrets contain 20 random bytes, use uppercase Base32 and six ASCII digits, and test the previous/current/next 30-second step in that order. Counters advance across the whole user, while verified freshness belongs only to the exact live session. Verification on a Holding child never verifies its Account parent or creates Platform authority. The current native host policy supports Account and configured Holdings; the separate legacy Admin-host runtime remains a cutover dependency.

Native enrollment makes the explicitly reviewed correction of checking original binding `authenticated_at` within five minutes instead of a newly created child's `createdAt`. SSO cannot refresh authentication age. Counter CAS, current-key resealing and exact-session freshness insertion commit together; final source/proof checks reject deferred effects and expiry. Freshness rejects timestamps in the future, is strict at the five-minute boundary, and authority evidence expires no later than the underlying session. The existing wire `verifiedUntil` remains the verification timestamp plus five minutes. Enrollment attempts allow 3 per five-minute window and verification 8; even malformed code strings and failed codes increment a separate committed, authenticated Control attempt unit before the protected business unit. Rolling back a failed verification cannot erase its attempt. The window resets only after the original strict cutoff.

`AccountMfaKeyring` requires explicit purpose-specific keys, bounded key IDs and a selected current key; no key is inferred from mail, BetterAuth or the environment. The `mfa1` AES-256-GCM envelope authenticates its version, key ID and user/purpose context with a random 96-bit nonce. Successful verification re-encrypts the unchanged secret with the current key. Retained keys allow rotation; missing keys, ciphertext substitution, tampering and unsupported legacy envelopes fail closed. Existing `iam_mfa` and `iam_mfa_sessions` tables are reused without changing historical migrations. Enrollment is the only response containing the setup key; it is transient component state, cleared after verification/account switching, and never stored as a session bearer or client authorization proof.

Personal security reads return display-only families and a count of live unbound legacy sessions; they do not adopt those sessions. Family revocation removes that parent's exact live family; all-device revocation includes every own session while preserving foreign users. Both commands require current Account proof and fresh MFA before considering an idempotency receipt. Replay retains that requirement; self-revocation cannot replay through its now-invalid cookie. Source-bound opaque hashes, trimmed reason and the original command key/body remain unchanged across uncertain retries. Holding tombstones commit before the destructive Control effect; current freshness is sampled again after every Holding wait and before receipt/audit acceptance. Late serialization failure never replays a committed Holding callback. A rejected or expired Control command leaves prior Holding fences intact and returns no successful acknowledgement.

Personal source SQL selects only the actor's sessions, directly relevant parent/child binding metadata and own MFA/user records. A foreign child pointing at an own parent rejects the source through a bounded existence check; another user's raw session image never enters the personal snapshot. Source rows are streamed with SQL byte guards before transfer, a maximum 2048 retained rows and 2MiB aggregate budget. Ordinary source rows are limited to 64KiB; MFA source/verification images and receipt results have 4KiB limits, and encrypted MFA material has a 256-byte limit. Oversized sources fail closed instead of truncating an image used by the revision hash. The final comparison retains the original scoped binding IDs after session deletion. Borrowed fresh-MFA reads positively lock the actual `iam_mfa` and `iam_mfa_sessions` tuples; concurrent disable/rotation after an older SERIALIZABLE snapshot propagates `40001`, rather than accepting stale enabled/fresh metadata.

The display-only user-agent limit makes one approved Unicode normalization: native truncates on complete Unicode scalar boundaries within 500 UTF16 units. B0 `slice(0,500)` can split a surrogate pair; native omits that whole boundary character. The fixture records the actual old `JSON.stringify` result as an escaped string, the approved native display and the unchanged raw-source revision for adjacent boundary cases. This does not change the full source image, current proof or authorization. Fresh native issuance currently records a null user agent; bounded actual HTTP header capture remains an explicit metadata-parity task before complete cutover, and null fixture data does not waive it.

## Explicit remaining parity

| Remaining surface | Required parity before native owner acceptance |
| --- | --- |
| Invitation administration and existing-account linking | Creation/proposal/review/revocation/delivery administration, verified direct/existing-account linking and interrupted expired-workflow reconciliation still require native parity. The implemented acceptance path covers reviewed fresh local staff only; unsupported payloads fail closed |
| Mail composition and live provider acceptance | Assemble the explicit Control manifest, registered Account policy, retained encryption keyring and selected private provider profile into the jobs runtime. No runtime or live provider delivery is active; SMTP/Resend transport verification uses literal loopback fixtures only. Existing TypeScript delivery queues are not silently adopted |
| SSO runtime and browser cutover | Assemble the native router with the actual gateway/Account configuration and complete real browser/normal-login/directory continuation and cross-Holding revocation acceptance; in-process HTTP and disposable PostgreSQL proof do not activate it |
| Native device grouping browser acceptance | The bounded native recovery protocol is implemented and independently tested; real browser/cutover and private runtime composition remain pending. Ambiguous saved-only discovery still returns 409; the existing ordinary credential form can create a fresh isolated group only after successful authentication. Signed UUID/BetterAuth browser groups are not adopted, and unrelated groups are never silently merged |
| Directory login/provisioning | Exact issuer and immutable provider identity, current source freshness and approvals, private secret profiles, reviewed mappings, provider failure and revocation rules |
| Effective Holding authorization | Current assignment/role lineage is necessary but insufficient: complete IAM, HR eligibility, GPO, emergency/fresh-MFA, same-Person independence and original owner record policy must execute in the retained request transaction |
| Account security and administration | Native private runtime/SSR assembly, legacy raw-session-ID APIs, profiles, preferences, emergency activation/revocation, invitations, access reviews, role/matrix/inspector, Directory administration and account suspension/restore-review lifecycle. B0 has no passkeys, recovery-code login, self-service deletion or login-time TOTP challenge |
| Queued authority and notifications | Source witnesses, current-policy recheck, idempotency, durable outbox and provider publication boundaries |
| Environment activation | Current immutable SSO schema allows the `development` binding only. Native policy fails closed for other environments until an explicit schema/configuration transition is implemented and tested |
| Abuse and abandoned work | Per-source abuse controls, cancelled password-work/abandoned nonce cleanup, delivery throttles and durable housekeeping still need production parity review |
| Source removal and release | Retire remaining TypeScript owner implementation and exports only after equivalent native flows and typed clients pass; then complete browser, isolated build, exact-source CI and release gates |

The generated HTTP description includes implemented signup, reset, password-change, verification, invitation and SSO handlers. The reset mail keeps the existing `/api/auth/reset-password/{token}?callbackURL=...` shape. Its native GET validates a live proof without consuming it and privately redirects to the Account reset form; the POST consumes the proof. Reset and verification re-sample time after source locks and reject expiry before the final credential commit, including expiry during a staged audit. It does not establish provider delivery, administrative workflow parity, runtime configuration or browser acceptance. The real current invitation authority adapter and fresh-staff activation are source implementations behind an unmounted facade. No compatibility fallback supplies those gaps. The shared development service remains on its existing backend until a separate coherent cutover is ready.

## Verification

The canonical suites are `identity_access.rust-account-auth.unit`, `identity_access.rust-account-auth-postgres.integration`, `identity_access.rust-account-recovery.unit`, `identity_access.rust-account-recovery-postgres.integration`, `identity_access.rust-account-sso.unit`, `identity_access.rust-account-mail.unit`, `identity_access.rust-account-mail-postgres.integration`, `identity_access.rust-account-mail-transport.provider`, `identity_access.rust-account-sso-postgres.integration`, `identity_access.rust-account-revocation-postgres.integration`, `identity_access.rust-account-device-http-postgres.integration`, `identity_access.rust-account-group-recovery-postgres.integration`, `identity_access.rust-account-mfa.unit`, `identity_access.rust-account-mfa-postgres.integration`, `identity_access.rust-account-security-postgres.integration`, `identity_access.rust-invitation-authority-postgres.integration`, `identity_access.rust-account-invitation-activation-postgres.integration`, `people_payroll.rust-invitation-person-link.unit`, `people_payroll.rust-invitation-person-link-postgres.integration` and `identity_access.account-auth-http.unit`. PostgreSQL tests use the guarded disposable `erpx_dev_rust_*` fixture and immutable Control migrations, never production or staging databases. Cases cover credential proof, closed JSON/cookies, exact authority and CORS, saved-proof isolation, stale parallel selection, principal/parent/audience bounds, nonce cleanup, sign-out scope, single-use/superseded mail proofs and failed-audit rollback. SSO cases add two registered Holdings, a one-connection callback race, real serialization retry, failed-binding rollback, inherited expiry/authentication time, scope-preserving family revocation, no legacy-session upgrade and full in-process HTTP redirects/cookies. Repository-only signup tests explicitly inject a reviewed test authority. Separate real two-database suites exercise the actual IAM/Organization/People adapter and ordered activation, including immutable role revisions, scoped denial cases, source-lock expiry, exact Origin/Host envelopes, true Control and Holding serialization retries, retained Control proof, failed audit rollback, password-bound resume, failed finalization recovery, and no revoked-grant resurrection. The People suite independently proves enclosing rollback, competing-account exclusion and an earlier engagement snapshot raising `40001` before a fresh denial. Mail cases also prove single-connection worker exclusion, retained recipient/credential locks, lease recovery before/after durable intent, post-send ACK rollback, SMTP uncertainty without replay, Resend idempotent recovery, retained-key rotation, stale ACK rejection and end-to-end reset-link redirects. Separate provider-lane cases use the actual Rust HTTP/SMTP adapters against owned loopback servers, including rejected TLS downgrade and unfollowed redirects. These results are development verification, not full owner, browser, live provider or production acceptance.

Regenerate both contracts with `bun scripts/docs/generate-account-contracts.ts`; verify with the same command plus `--check`. The exporter uses the pinned Rust workspace and locked dependencies. Do not edit generated TypeScript or OpenAPI output by hand.

## Native Directory observation foundation

The optional `directory-ldaps` feature now contains a native LDAPS provider and
validated private-profile generations. It uses the unmodified, strict
`ldap3_proto 0.8.1` codec over explicit Ring/Rustls, with certificate and server
name verification, optional private CA roots, and no plaintext, proxy or referral
following. Package-local license/source notices preserve MPL-2.0 for the codec
and MIT for `ldap3_lber`; this does not complete the release dependency inventory.

The transport checks definite BER lengths before allocating a body and rejects
nested indefinite encodings, depth over 32, more than 8,192 elements per frame,
frames over 1 MiB, over 64 MiB total wire traffic, or over 32 MiB retained decoded rows.
Requests are sequential, with positive matching message IDs, a 15 second operation
deadline and 60 second total stream deadline. Search observation requires every
page/range to terminate successfully; repeated/missing page cookies, unsolicited
messages, truncated results, exceeded budgets and unsupported critical/operation controls never
produce a complete snapshot. User/group searches cap each at 10,000 rows; each
container search does too, with at most 20,000 combined containers when separate
group roots are required. AD partition referrals are ignored only on the same
main observations as B0 and are never followed; UCS and range-search referrals
fail the observation. Raw LDAP request/response values and upstream decoder
tracing are never emitted by this adapter.

The retained unchanged-Node oracle covers hierarchy traversal order, AD/UCS
account control coercion, binary GUID order, contextual Unicode lowercase and
immutable-ID aliases. These are finite rule/codec checks, not enterprise-provider
acceptance. The canonical provider suite exercises twelve actual owned loopback TLS
scenarios, including private CA/SNI, service and user binds, paging/ranges, referrals,
malformed/unsolicited streams and retained profile generations. Private profile bytes are loaded explicitly outside database leases;
an explicit validated reload waits for retained profile-generation guards. The
provider retains its generation during observation, and the final Account command
retains that exact generation through Control commit.

The native PostgreSQL adapter resolves only explicit configured Control/Holding
bindings. It stores complete identity/group/hierarchy observations, removes missing
memberships, evaluates current standing reviewed mappings, and appends immutable
audit in one Holding unit. Intended revocations are checked against bounded locked
pre-images, then upstream mapping/parent/role evidence is captured before new
grant insertion. Exact intended grant/revocation after-images and that original
source are checked after final effects, so immediate and deferred triggers cannot
replace the source baseline, suppress a revocation or rewrite the recipient.
Existing reviewed user/Person links remain unchanged;
imports do not create accounts. An older observation or failure cannot replace a
newer complete observation. Policy reads are bounded before transfer, and current
Organization lineage and People source locks use their owners' borrowed public
ports. Revoked mapping grants are never recreated automatically. The unchanged
B0 mapping oracle preserves ordinary outcomes and records the two reviewed native
scope rejections in [ADR-055](../../adr/ADR-055-native-directory-scope-and-credential-witness.md).

A successful real provider bind produces expiring, non-serializable evidence. The
session-first, non-retrying Control command retains exact Directory configuration,
approved membership, current mapping/parent/role read sets and profile generation
through normal opaque Account issuance. It uses the existing browser revision and
nonce guards and mandatory cross-Holding retirement; no local password or dummy
credential is inserted. Time and source evidence are rechecked after waits and
before acknowledgement. A second Holding failure leaves earlier tombstones in
place, rolls back Control acknowledgement, and permits a normal credential retry.
The real disposable Control/two-Holding suite proves these boundaries, final audit
and parent-version rollback, credential expiry during a real lock wait, and durable
failed-attempt throttling.

The unmounted native POST `/api/auth/directory/sign-in` keeps the existing
`{connectionId, username, password}` intent and `{success:true}` response; an
optional captured native device revision adds no bearer authority. It requires
exact Account Host/Origin, bounded JSON and normal secure host-only opaque
cookies. Like B0 `Request.json()` and its Zod object schema, duplicate properties
keep their final value and harmless unknown properties are discarded. Extra
actor, grant and session fields never become authority; only the typed credential
intent is consumed. Holding Origins cannot submit Directory credentials to it. A typed
frontend port consumes the generated Rust DTO and rejects credential-shaped
response extensions. The existing Directory login form uses that port for LDAPS;
OIDC retains its existing implementation while its native migration is pending.

Stale business requests now have a native current-source preparation path. A short
session-first Control/Holding preflight checks the actual device-bound child,
configured Holding, scoped tombstone, selected grant and original CAS, plus current
Directory membership/configuration. Explicit suspension, revoked sessions, stale
selection or an invalid link deny before provider work. An old disabled observation
may be refreshed to discover re-enablement; a current disabled identity is denied.
Every database lease ends before service-only LDAPS observation. No password bind,
login credential, session or Actor DTO is minted by refresh.

The actual provider transfers its retained private generation with the completed
observation. Reconciliation reuses that retention and rechecks the original request
on a fresh same-Control lease; it never nests a second fair generation read behind
a queued reload writer. Final business execution uses the ordinary
`HoldingWorkAuthorizer`, configured by `DirectoryRefreshRuntime::holding`, with the
same concrete owner-facing API and stable constructor. The
configured path consumes deferred selectors once after session verification, then
enters only the existing internal coordinator with actual current IAM/owner facts.
Current route/configuration and profile retention share that coordinator's
session-first Control transaction through Holding commit. Relevant Directory
mapping ancestry, immutable identity link, source generation and original deadline
are checked before effects and after deferred effects. Extra sources can only
narrow the normal context's deadline. They cannot construct permission authority.
The runtime bounds concurrent full provider observations to two without an
unbounded waiting queue.

Holding migration `1074_native_directory_observation_provenance.sql` records an
explicit approved source revision and complete-observation provenance beside the
actual Directory Connection. A selected canonical profile uses a purpose-separated
HMAC-SHA256 tag and a named shared private key. The unkeyed whole-file generation
fingerprint remains local. File formatting and unrelated profiles do not alter the
selected source; changed selected credentials, TLS/CA policy, aliases or Connection
configuration require explicit approval and a new revision. Reusing earlier
material also advances the revision. Restarts preserve the source identity when
material and keys match. An incompatible API/worker fails configuration without
rewriting the approved source, provider health or another instance's observation.

`DirectoryProfileStore` requires an explicit `provenance::ProvenanceKeyRing`.
The bounded private keyring file declares format version 1, environment and up to
eight uniquely named 32-byte keys. It has no ambient credential or generated-key
fallback. `prepare_source_change` captures an actual sealed candidate;
`proposed_source()` exposes only its exact key/tag/configuration binding for a
retained native review. `change_source` requires current Control/Holding policy
ports and their final checks. Initial seed proves expected absence, while rotation
proves the original source revision and Connection CAS. There is no production
default policy or startup approval. Component seed policies are explicitly guarded
to disposable databases and do not establish real admin/review/MFA acceptance.

Complete synchronization stamps the exact source with identity/group/hierarchy,
reviewed mapping effects and audit in one Holding unit. Five-minute freshness now
also requires matching durable source and identity timestamps; future observations
are rejected. Missing observation provenance under an approved matching source
requires a complete refresh. Missing or mismatched source approval is a configuration
failure before provider work. `1075_native_directory_principal_provenance.sql`
preserves the complete existing People, external relationship, invitation and
legacy principal predicates and adds the same durable source check to SQL grant
ancestry. The ordinary authorizer, configured refresh and queued/offline current-IAM
paths retain source checks through owner commit. A newer timestamp from an older
source cannot skip required current-source reconciliation.

Add a new key to intended instances before selecting it, advance affected source
revisions and obtain complete observations before retiring the previous key.
Missing selected keys and provider outages after rotation admit no previous-source
fallback. Source tags are factual data; current session, IAM/HR/scope, MFA and owner
policy remain required. Native Control-platform Directory fails closed until its
provenance schema is mirrored. See [ADR-056](../../adr/ADR-056-durable-directory-observation-provenance.md).

The registered provenance suites have 17 pure cases and 14 actual PostgreSQL/owned-loopback cases. The concurrent revision-CAS regression forces two first policy reads and observes three Holding policy attempts in total: two original attempts and one native bounded retry, with exactly one committed source/audit change. Retry limits, deadlines and production lock order are unchanged. Future Directory observations deny before owner effects: the ordinary authorizer's SQL principal guard returns `WORK_ROLE_UNAVAILABLE`, while the configured refresh preflight returns `DIRECTORY_SOURCE_CONFIGURATION` before provider I/O. All 39 affected current-authority, Directory, configured-refresh and command-metadata regression cases pass. These are development component results, not full owner or whole-candidate acceptance.

The service-owned `DirectorySourceAdministration` implements source preview and
atomic independent `approve_and_apply`. Normal sign-in and SSO produce the actual
Holding child; its own MFA freshness is required, with no inherited parent proof.
The existing authorizer verifies the exact session, configured Holding binding,
selected grant/CAS, full IAM/HR and Company scope. Its Identity-private admin hook
retains MFA and the target route on that same Control lease and binds its witness
to the actual Holding database/backend/transaction. A generic authorized context
cannot substitute for this witness. Profile retention happens only after every
database lease has ended, followed by fresh verification before durable preview
or application. No provider request is made by source administration.

Proposals require `iam.directory.manage`; a different current person with
`iam.review` approves. A proposal expires within five minutes and retains its
original selected assignment, role publication, identity version and session/CAS
pins. New application rechecks the proposer's actual current authority and the
exact proposed key/tag/configuration, Connection version and original source
revision. Existing `iam_reviews`, `iam_receipts` and `iam_audit` storage records
the review and command. Source mutation, approval and sanitized audit/receipt
effects share the same Holding unit; predetermined images are checked after
deferred effects. Receipt replay performs current session/IAM/MFA checks first
and cannot replace the review's Connection with another same-Company target.
It returns historical result data without selecting or rotating a source again.
Explicit initial insertion still requires expected absence and independent
approval; startup never approves local configuration.

The registered `identity_access.rust-directory-source-admin-postgres.integration`
target passes 12 actual two-account sign-in/SSO/MFA/PostgreSQL cases. They cover
self-review refusal, parent/child proof separation, revoked proposer authority,
stale role/profile rejection, retained Control revocation and Holding expiry
waits, missing admin witness, wrong replay target, explicit key rotation/restart,
and deferred review-payload tampering. The existing provenance14 and ordinary
current-authority10 targets also pass after the shared borrowed-writer change.
These are component verification; all owner completion flags remain false.

Native SSR connection lookup, administrative listing/rejection, trust editing and
activation, full reconciliation jobs, operator seed/HTTP/UI composition, OIDC/Graph,
actual route/composition rollout, real browser and live provider acceptance, and
runtime activation remain pending.
The new provider/router are unmounted and grant no Work permission from a copied
snapshot. The legacy Directory runtime remains in place during migration. No
enterprise credentials or external enterprise Directory calls were used.
