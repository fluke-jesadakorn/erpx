# Directory & access: local user management

The current shared People entry and IT-owned external subjects/direct grants are defined by [People external access](people-external-access.md). The local group, policy and custom-attribute rules below retain their existing reviewed semantics; historical `/people/access` destinations forward to Technology.

The canonical entry is the existing scoped /people/access page. People, Groups, Access policies and Custom attributes are page-local views, not new sidebar destinations. Advanced exposes the existing account inspector, directory connections, roles and matrix. Organization placement continues through HR. Platform and external-directory writes are outside this feature.

## Interfaces and ownership

- GET /api/v1/holdings/{holdingSlug}/people/access/management returns authorized people, units, local groups, values, definitions, policies, attachments and review candidates. The existing scope adapter supports Local Workspaces.
- POST /api/v1/holdings/{holdingSlug}/iam/user-management/preview accepts {change:{operation,input}}; operations are attribute, values, group, membership, policy and attachment. It runs validation and impact calculation in a rolled-back transaction.
- The matching /save operation also requires previewToken, Idempotency-Key, expected resource versions and rendered work-context headers. The token binds the change to the visible snapshot; drift requires a fresh preview. All authorization is rechecked by the server.
- Reviews remain POST .../iam/reviews/{id}. A policy.grant review represents one beneficiary and one scoped role, even when the interface selects several reviews.

The shared schemas live in [the IAM feature contracts](../../../packages/feature-identity_access/src/lib/user-management.ts). Holding migrations 0023/0024 add Company-owned metadata, immutable history and derived-grant provenance without modifying existing directory objects. No fixtures are added by normal seeding.

## Membership and policy semantics

Manual groups may contain manual or automatic children; automatic groups are leaves. Children remain within their parent's workspace boundary. Membership is a union of direct and nested members. Removing a direct membership does not remove membership inherited from a child.

Rules use bounded All/Any conditions, typed comparisons and explicit empty tests. Missing values never match inequality or ordered comparisons. Built-in fields come from the active engagement; custom conditions reference stable person-field identifiers. Group/unit attributes are descriptive metadata, not rule inputs. Required fields are enforced when their attribute form is edited, allowing existing records to be completed gradually.

Attachments select members, but each role retains its explicit permission scope and deadline. Exclusions suppress that attachment for members of the excluded subtree, including people also reached through another path in the same attachment. Other attachments and manual grants remain independent. Policy or attachment revisions require new proposals. Archived metadata and values remain in history.

Every candidate is checked against the attachment author's current authority before proposal and against the original author and reviewer again at approval. A blocked candidate explains why it cannot be proposed. Ownership and recovery roles require direct assignments. No reconciliation grants, renews or approves access.

Database eligibility functions participate in iam_assignment_active, covering commands and queued authority. Row-change triggers invalidate affected pending/active evidence and audit the loss. The existing directory job pulse also reconciles local policy proposals; a failed worker cannot leave stale eligibility usable. Manual revocations and rejected reviews do not generate repeated proposals while eligibility is unchanged.

## Interface and verification

The desktop hierarchy, results and details become successive mobile screens. Every drag action has a form alternative. Drops prepare an editor; previews precede saves. Connected-directory browsing retains the existing read-only explorer.

Run focused tests/integration/identity_access/user-management.test.ts, existing IAM tests, and tests/e2e/identity_access/iam.spec.ts, plus typecheck, lint and isolated release build. Public verification uses an authenticated session; no provider credentials or external write-back are needed. Development remains on 3100.

Verified on 2026-09-16: the full 45-file suite passed (383 tests); the expanded user-management suite then passed all 10 tests, including independent policy sources and expiry without reconciliation. All eight IAM browser journeys passed on desktop/mobile, and the final management journey passed again after contextual account actions, archive filtering and mobile Back controls. Layout screenshots cover 390, 768 and 1440 pixels in both themes. Typecheck, lint and the isolated deployment build passed. Authenticated public verification checked Directory & access and Groups; development and its reconciliation worker remain separate from the installed production service.
