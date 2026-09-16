# Access and error feedback — 2026-09-14

Workspace layout failures no longer become blanket 404s. `/access` checks the signed-in account against the requested scope and shows a reason, next step, current account and recovery actions. Existing API authorization remains unchanged.

## Cases

| Condition | Feedback / recovery |
|---|---|
| Signed out or expired session | Sign in; preserve a validated internal destination |
| Active access in a different branch or department | Explain scope mismatch; open my workspace |
| Expired, revoked or future assignment | Explain the specific assignment state; ask an administrator and check access again |
| No assignment or invalid role at its scope | Explain missing access or role setup; contact administrator |
| Feature permission or platform restriction | Explain separate required access |
| Invalid selected company | Explain company selection/setup; return to available workspace |
| Inactive organization | Explain inactive workspace only when own membership permits diagnosis |
| Unknown, inaccessible or malformed workspace | Generic unavailable link; do not expose other organizations |
| Invalid organization lineage | Explain setup repair |
| Capability unavailable | Explain feature availability |
| Missing page or record (404/410) | Custom not-found page |
| Conflict, invalid request, rate limit | Specific recovery guidance |
| Unexpected server failure | Error boundary with retry and safe reference, never raw exception details |
| Browser reports offline after app loads | Offline guidance; restore original reason after connectivity returns |
| Access restored | Requested page is available; open it |

The shared catalog is `src/lib/access-problem.ts`. Assignment diagnosis is read-only and uses the current user's assignments, validity dates, revocations and authorized organization scope. URL reason hints cannot manufacture an expired or revoked assignment. Return links reject external URLs, encoded path separators and traversal outside workspace routes.

## Verification

- `bun run test -- tests/access-problem.test.ts tests/access-diagnosis.test.ts tests/page-error.test.ts`: classification, isolated PostgreSQL histories and inactive nodes, redirect handling, safe destinations, unknown errors and no database mutation.
- `bunx playwright test tests/e2e/access-feedback.spec.ts tests/e2e/auth-smoke.spec.ts`: desktop/mobile session handling, scoped account recovery, platform/section restrictions, invalid company, true 404, keyboard focus, 44px targets, offline recovery and login regression.
- Inspect public host at desktop and 360px width; ensure complete text and usable recovery buttons.

## Boundaries

These are application page/access states, not replacements for browser DNS/TLS errors or a Cloudflare outage before ERPX loads. Existing form-specific validation remains in its forms. Unauthorized API calls continue returning their original HTTP status; displaying feedback never grants access. A not-found response deliberately does not reveal whether a hidden record exists.

## Company preference recovery

Holding-level company choices are now filtered by the signed-in user's current, valid holding-scope assignments. The company-switch endpoint uses the same list. A browser preference referencing an active but unauthorized company falls back to an authorized holding company, when one exists. This prevents a previous account or previously exposed test-company option from stranding an otherwise authorized user. Missing/inactive company IDs still show company-unavailable feedback; branch-only users do not gain holding access.

`tests/e2e/company-recovery.spec.ts` covers a real active unauthorized company cookie, successful holding recovery, rejection of unauthorized company switching, and filtered company choices on desktop and mobile.

## Searchable preferred role

The account menu's role combobox filters the user's existing assignments by role/scope text. Enter chooses the highlighted result, arrows change the highlight, Escape cancels the search, and pointer selection is supported. No matches produces inline feedback. The selected preference and Automatic behavior are unchanged. Browser coverage in `preferred-role.spec.ts` exercises filtering, no results, keyboard selection, reload persistence, pointer selection and unauthorized assignment rejection on desktop and mobile.
