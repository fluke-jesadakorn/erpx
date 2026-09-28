# Expense workflow v2, AI connections and IT administration

> Historical plan/report. Superseded for delivery scope and priorities by [the current production plan](../../production-plan.md). Runtime and implementation statements below describe their original date; see [the retention index](README.md) for current requirements.


## Entry points

- Every active company member: **Work → New → Expense claim**. A draft needs neither an employee profile nor complete financial fields. Submission explains missing requirements and links to the existing People editor.
- Company owners and owner-appointed IT administrators: **Administration → AI providers**, at `settings?section=ai`.
- Owners grant and revoke **IT administrator** through People → the person's **Access** panel. The role is company scoped and adds AI configuration authority only. Beta invitations can include this role alongside ordinary employee access.
- Routing repairs remain in **Administration → Approvals & routines**. Replacements must currently qualify, with a reason and checked record/approval versions.

## Review batches

| Batch                | Main files and behavior                                                                                                                                                                   |
| -------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Drafts and access    | Migration 0031; `expense-drafts.ts`, Work creation, expense editor, shared People/member editor. Nullable draft fields, stable claimant identity and workflow version.                    |
| Approval and payment | `expense-routing.ts`, `expense-workflow.ts`, `expense-payments.ts`, shared `approvals.ts` and posting/settlement adapters; migration 0038 records actual authority and reviewed receipts. |
| IT and AI            | Migrations 0032–0037; business role/scope guards, `ai-providers.ts`, `ai-transport.ts`, provider administration, social/expense AI, document derivatives and jobs Worker.                 |
| Verification         | Expense/provider integration tests, invitation regression, migration/history checks, isolated expense E2E fixture, mobile/theme screenshots.                                              |

No existing submitted, posted or reimbursed claim is converted to the new approval sequence. New drafts and explicitly resubmitted claims use v2. Existing leave and task decisions retain their original rules.

## Business transitions

1. The member reviews and submits a complete claim with an active employee/contractor profile, department, active staff level and receipt.
2. The highest active rank in that department supplies the independent approval pool. Equal ranks compete for the same versioned step. If the claimant/preparer has no independent peer at that rank, routing checks the reporting superior and then a company owner. Missing configuration and reporting cycles block submission. A candidate must leave a feasible independent Accounts/manager/Treasury sequence.
3. An active, scoped `finance_preparer` reviews evidence and saves accounting classification, recoverable tax and posting details. Adjusting tax preserves the claimed total.
4. A different `finance_approver` approves. The approval decision, balanced expense journal and employee payable commit in one D1 batch. Closed periods fail the whole batch.
5. A different `treasury` operator records a payment made through the bank, uploads that payment's photo/PDF slip, reviews the extracted fields and explicitly confirms. Partial payments leave the balance open. Each confirmed payment requires a distinct slip; duplicate commands cannot pay twice.

The claimant/preparer cannot approve or pay their own preparation. Current assignments and company/branch scope determine authority, independently of the role selected in the workspace menu. Committing batches recheck roles, staff eligibility, routing inputs and optimistic versions. Material claim or receipt corrections return through department approval. Alternate posting/settlement endpoints cannot bypass v2.

Approval history includes the actual actor, reviewed version, role assignments, staff level, routing snapshot and receipt IDs/checksums. Existing historical decision rows remain intact.

## Provider configuration

Connections default to **Custom API — OpenAI-compatible** and Bearer authentication. Manual model IDs are always available; model discovery is optional. A missing key can be saved as an untested draft. Blank key input preserves a saved key; replacing and removing a key are explicit actions.

Saving creates an immutable draft revision. Authentication/model/JSON and optional image tests belong to that revision. Activation requires successful tests and an unchanged connection version. Editing leaves the active revision in use; the UI distinguishes the active values from draft edits. Disabling/deleting a selected connection requires a replacement selection or explicit disabling of its affected capabilities.

Company selections apply to social and expense AI through the same resolver. Text and document-image connections can differ. Reusing the default connection for documents requires its tested image model. Unconfigured companies retain the labelled Deployment default. A configured provider failure never switches to another provider automatically.

API keys are AES-GCM encrypted with a versioned 256-bit deployment key, authenticated to company/provider identity. API responses and UI expose credential presence only. Audit events contain operation metadata, not credentials. Endpoints must use public HTTPS, with no embedded credentials, private addresses or outbound redirects. Node development pins verified public DNS addresses; Workers uses public-network fetch restrictions.

## Local and deployed setup

Use Bun 1.2.17. Ports 3000, 3001 and 8080 belong to other projects; ERPX uses 3100, preview 8787 and jobs 8791.

```sh
bun run db:migrate:local
bun scripts/configure-local-ai.ts
bun run dev
# In a second terminal:
bun run dev:jobs
```

The local setup script generates the encryption key once, preserves it on retries, and mirrors it into ignored `.env.local` and `.dev.vars`, with mode 0600. OpenNext intentionally excludes `.env` values from Worker bindings, so `.dev.vars` supplies the application's local encryption binding. The jobs Worker shares the same key and R2 buckets. No provider API key is generated or activated by this script.

For a deployed environment, configure **the same** `AI_CREDENTIAL_KEYS` JSON keyring and `AI_CREDENTIAL_KEY_ID` in both application and jobs Workers, including their beta counterparts when used. For example, the keyring has the shape `{"v1":"<base64-encoded 32 random bytes>"}`. Supply it through Wrangler secrets or the Cloudflare dashboard, never a checked-in config. The key ID chooses the key for newly saved credentials. During rotation, keep previous entries until all credentials using them are explicitly replaced and activated. Deployment-default AI additionally uses `AI_BASE_URL`, `AI_MODEL` and `AI_API_KEY`.

Apply the holding-write migrations before deploying the new application and jobs code. Back up each D1 database first. Migration 0031 preserves the historical expense-decision foreign keys during the draft-table rebuild. Keep v2 records and their audit history when rolling application code back; do not drop the new tables to undo a UI deployment.

## AI preparation and document boundaries

- The original JPEG, PNG, WebP or PDF remains the evidence. PDF.js **6.3.289** and its browser worker/assets are pinned and copied locally during dev/build; no document is sent to a third-party PDF renderer.
- Scanned PDFs render one page at a time. Maximum 20 pages per original, 1600 pixels along the longest edge, and 2 MB per derivative. AI processes at most 40 evidence pages per claim, in batches of two images. Original uploads retain the existing 10 MB/file and 10-file limits.
- Encrypted/unreadable documents, unsupported CSV extraction, excessive page counts and unavailable AI preserve the original and allow manual completion. Payment evidence must be an image or PDF.
- Derivatives inherit original-document access and count towards storage. Proposals are bound to company, stage/version, evidence digest and provider revisions.
- The outbox and jobs Worker handle preparation. Conditional leases prevent duplicate workers from applying the same result; the scheduled dispatcher recovers queued/expired work. Quotas combine social and expense AI, and usage is attributed to the actual text/image models.
- AI summarizes department and manager reviews and suggests receipt fields, Accounts coding/tax/posting details, and bank-slip fields. Payment extraction uses that payment's slips. Supporting attachment/page citations are validated; conflicting values remain blank. Identity and approval routing come from company records.
- AI never submits, approves, posts or confirms a payment. Suggestions fill blank fields; replacing an existing value requires an explicit choice. Human changes survive refresh, uploads and AI failure. Source/configuration changes invalidate unsaved AI values without erasing human edits. Concurrent server edits require review before overwriting.

## Verification commands and artifacts

```sh
bun run lint
bun run build
bun run build:cloudflare
# Integration runs after the Cloudflare build has finished:
bun run test:integration
bun scripts/seed-expense-e2e.ts
ERPX_E2E_BASE_URL=http://localhost:3100 node node_modules/@playwright/test/cli.js test tests/e2e/expense-v2.spec.ts
```

The seed script creates a separate fictional company and users; it does not modify normal local accounts. Its fixture lives in `.scratch/expense-v2/fixture.json`. The E2E suite exercises an incomplete member draft, scanned PDF rendering, the full four-person sequence, two partial payments with distinct test slips, IT provider drafts, employee denial, and AI-to-form handoff. Synthetic payment slips are test evidence; no bank transfer is executed by the test or application.

Screenshots at 390/768/1440 px in light/dark themes, overflow and 44 px form/control checks, and build/test logs are under `.scratch/expense-v2/`. Provider service/queue tests use synthetic adapters. A real company provider must still pass **Test connection** before activation; no live provider credential or bank account is assumed by local verification.

Verified locally on 2026-09-08:

- Lint, Next.js production build and Cloudflare build passed. ERP and beta jobs Worker dry-run builds passed.
- All 101 integration tests across 15 files passed after the final Cloudflare build, including branch isolation, revoked assignments, transactional posting, migration compatibility and provider/queue behavior.
- All eight expense E2E cases passed across desktop/mobile; the two complete approval/payment journeys passed again after the final branch-scope and decision-draft changes. Five related approval, People, social-task and routine regression cases also passed.
- Light/dark screenshots and layout checks cover 390, 768 and 1440 px. The final credential scan checked 46 generated production client JavaScript files with zero matches for local encryption credentials and synthetic test keys.
- Holding-write migrations 0031–0038 were applied to both local holding databases. No remote migration or deployment was performed.

Primary implementation references: [Cloudflare public fetch flag](https://developers.cloudflare.com/workers/configuration/compatibility-flags/#global-fetch-strictly-public), [PDF.js examples](https://mozilla.github.io/pdf.js/examples/), [pinned PDF.js release](https://github.com/mozilla/pdf.js/releases/tag/v6.3.289), and [IANA IPv6 special-purpose registry](https://www.iana.org/assignments/iana-ipv6-special-registry/).
