# IAM implementation acceptance

Verified on **2026-09-15** in the existing ERPX development workspace. The additive IAM migrations were applied to the development databases without reseeding employees, changing credentials or connecting a live directory. See the [implementation and operations contract](../contracts/next/identity-access.md) and [ADR-025](../adr/ADR-025-multiple-identity-providers-and-scoped-replacement.md).

## Automated checks

| Check | Result |
| --- | --- |
| `bun run typecheck` | Passed |
| `bun run lint` | Passed, no warnings |
| `bun run test:integration` | 44 files, **366 tests passed** |
| IAM, authentication and work-journey Playwright suites | **32 desktop/mobile tests passed** |
| Final IAM Playwright rerun after adding audit search and CSV download checks | **2 desktop/mobile tests passed** |
| `bun run deploy:build` | Passed; isolated release `2026-09-15T13-30-02-491Z` |

The browser regression run used the existing development roster through `ERPX_E2E_EMAIL` and `ERPX_E2E_PASSWORD` overrides. Credentials are not recorded here. All build and test commands used the pinned Node 24.20.0/Bun 1.2.17 toolchain. Local logs are retained under ignored `.scratch/iam/` (`integration-end.log`, `typecheck-end.log`, `lint-end.log`, `browser-complete.log`, `browser-end.log`, `build-end.log`).

IAM tests cover default denial and unavailable preview capabilities, immutable role revisions, independent Person review and fresh MFA, bounded delegation, parent revocation/expiry, stale contexts and queued authority, duplicate contact emails, immutable directory identities, partial synchronization, selective migration, historical attribution, group/manual grant separation, recovery protection, record inspection and scoped audit pagination. Provider tests exercise AD/UCS LDAP behavior and Entra/UCS OIDC with controlled directory/Graph responses and signed JWT verification. These are adapter tests, not live-provider acceptance.

## Public browser acceptance

Verified through the existing authenticated Chrome session at [ERPX IAM](https://erpx.jesadakorn.com/holdings/acme/iam):

- Overview, Users, Roles & permissions, Groups & mappings, Directory connections, Reviews & audit, Access matrix and My access load.
- AD, Entra and UCS provider choices display the relevant configuration fields. UCS switches between LDAPS and Keycloak OIDC. No connection was saved or activated during verification.
- The auditor matrix permits Finance read and denies preparation/approval/payment actions. Its explanation names the selected assignment, scope, role revision, grant source and expiry. Preview modules show Unavailable.
- Keyboard navigation and explanation focus work. At a 390-pixel mobile viewport, matrix scrolling stays inside its container and the detail panel is readable. The viewport override was reset.
- Audit search returns an empty result for an unmatched query. Playwright additionally verifies the controlled CSV download and its header.
- Platform IAM has its own permission namespace and assignments. Global `/my-access` displays the person's workspace assignments without requiring a selected customer role.
- The owner Home page still displays existing work after the authorization migration. Request-scoped eligibility reuse reduced the observed local owner summary response to approximately 4.8 seconds; this is a development observation, not a performance guarantee.

After verification, restoring the browser's original auditor selection reached Chrome's unsaved-change confirmation (the audit search was the unsaved input). The Mac locked before that confirmation could be cleared. This browser cleanup needs the user to unlock the Mac; it does not affect the completed page checks or server availability.

## Runtime availability

- `http://127.0.0.1:3100/api/health`: **HTTP 200**, `{"status":"ok"}`.
- `https://erpx.jesadakorn.com/api/health`: **HTTP 200**, `{"status":"ok"}`.
- The workspace's development server remains detached on **3100**, using pinned Node 24.20.0. PID and logs remain in ignored `.scratch/dev/`.
- The development PostgreSQL job runner was rebuilt and restarted as a detached process with the directory reconciliation pulse.
- Production **3101**, reserved ports and public Tunnel routing were preserved.

## Remaining environment acceptance

No live AD, Entra or UCS profiles, tenant consent or reachable test directory were supplied. Verify all four real sign-in paths, certificate trust, nested directory membership, account disablement, refresh/outage handling and a reviewed pilot migration against the organization's actual configuration before enabling employee migration.

Customer commissioning must supply independent authorized administrators with MFA, then designate protected local recovery access through review. The application intentionally provides no self-approval or Platform-to-customer authority shortcut. Existing local-only operation remains available while connections are inactive.
