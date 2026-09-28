# Social + AI public beta: conversations that become completed work

> Historical plan/report. Superseded for delivery scope and priorities by [the current production plan](../../production-plan.md). Runtime and implementation statements below describe their original date; see [the retention index](README.md) for current requirements.


**Date:** 2026-09-07  
**Status:** Implemented and verified locally; external-service and production launch gates remain closed.  
**Audience:** ERPX implementers and the operator running the beta.  
**Evidence:** Primary vendor documentation checked on 2026-09-07; vendor claims are distinguished from ERPX hypotheses below.

This note records the research and accepted beta scope. It does not certify that any feature, test, provider integration, or production deployment is complete. The implementation owner must add actual verification evidence after delivery.

## Business hypothesis and judgment

Start with the user's own business, then offer the same workflow to multi-branch service businesses. The first problem is **conversations that lead to dropped work**: somebody reports an issue, a discussion happens, and responsibility or completion is lost.

The core journey is **social post → private AI draft → human-reviewed staff task → accountable completion**, using existing approval workflows where required. For example, a branch posts an equipment problem; AI proposes a task from that conversation; a person checks the details and selects the assignee; the team follows the linked task through completion. A friendly Home gives staff reasons to ask, share, and help each other while keeping the work traceable.

The idea is worth testing, but adding a social feed and AI across screens is already a competitive category. ERPX's proposed advantage is a simple Thai workflow that reliably turns frontline conversations into completed service work. That advantage remains a hypothesis until real organizations use it repeatedly and a buyer pays for the result. A broad ERP replacement and feature parity with incumbent suites are outside this one-month beta.

The largest uncertainty is whether managers will pay enough for fewer missed handovers and less follow-up to cover hosting, AI, email, onboarding, and support, while staff participate without constant prompting. More posts, likes, or AI requests alone would not answer that question.

## Primary research and implementation implications

| Finding | Evidence and limitation | Adopt in ERPX / avoid |
| --- | --- | --- |
| Contextual AI inside social and task surfaces is established. | Bitrix24 documents composing and summarizing Feed posts/comments, generating task descriptions/checklists, and mentioning CoPilot in chat. These pages describe product behavior, not independent adoption or ROI evidence. They do not establish arbitrary live cross-record retrieval by the Feed writing helper. [Feed](https://helpdesk.bitrix24.com/open/19313272/), [Tasks](https://helpdesk.bitrix24.com/open/19275690/), [Chat](https://helpdesk.bitrix24.com/open/18991098/) | Put a small “prepare task” action next to the conversation. Keep the original post as context. Avoid making users relocate their discussion into a separate generic AI chat to create work. |
| AI assistance need not publish on its own. | Bitrix24 says Feed AI responses are not automatically published; users can copy them into comments. Slack supports private and public agent interactions. [Bitrix24 Feed](https://helpdesk.bitrix24.com/open/19313272/), [Slack agents](https://slack.com/help/articles/33076000248851-Work-with-AI-agents-and-assistants-in-Slack) | Show an editable private draft to the requester, then require a deliberate human action to create a task or share text. Avoid generated praise and automatic replies that fill the community with AI activity. The latter is an ERPX product judgment. |
| Cross-app intelligence needs an explicit supported surface. | Zoho advertises permission-aware search and actions across business apps; its compatible-app list names Cliq, People, Mail, WorkDrive, Notebook, CRM, Calendar, and Projects. The refreshed Zoho One launch page still labels Ask Zia Beta and asks for early access; no account-level general-availability check was performed. [Ask Zia](https://www.zoho.com/one/ask-zia.html), [Zoho One refresh](https://www.zoho.com/one/os-for-business-2025/) | Implement the narrow post-to-staff-task capability first, with clear sources and fields for review. Avoid implying that all ERPX modules are already actionable by AI. |
| The caller's access and a shared audience's access are different. | Slack says Agentforce answers use the caller's Slack/Salesforce permissions, warns about sharing generated information, and allows prompt links that produce a recipient-specific answer. Agentforce access also needs the relevant Salesforce license and assignment. [Agentforce in Slack](https://slack.com/help/articles/36218786859667-Use-Agentforce-in-Slack) | Enforce permissions when reading context and again when creating or sharing a result. Keep private information in private drafts; a task or post must not expose data merely because its creator could read it. |
| Business rules belong in deterministic operations. | Salesforce recommends Apex Actions/Flows for record changes, validations, and approvals, and describes typed action inputs/outputs and a user-confirmation setting. [Tool selection](https://developer.salesforce.com/docs/platform/hosted-mcp-servers/guide/agentforce.html), [Agent Script actions](https://developer.salesforce.com/docs/ai/agentforce/guide/ascript-ref-actions.html) | Route the reviewed draft through the existing staff-task service and approval policy. AI proposes fields; server code validates authorization and workflow rules. Avoid arbitrary model-generated SQL and bypasses around existing approvals. |
| Confirmation addresses a demonstrated integration risk. | Salesforce's September 2025 notice changed several actions to require confirmation, including case creation/commenting and order cancellation, citing prompt-injection risks. This is a vendor security measure, not proof that confirmation alone solves every risk. [Confirmation notice](https://help.salesforce.com/s/articleView?id=005133036&language=en_US&type=1) | Keep source text as data, bind the reviewed task fields to the create operation, and recheck state/permissions on execution. Never treat an instruction inside a post as authority to perform an unrelated operation. |
| AI pricing requires explicit limits and observability. | Bitrix24 advertises Standard Vibe+ at US$199/month billed monthly, or US$120/month billed annually, for 50 users with “Unlimited AI”; its help page still limits an account to 600 requests/hour. Salesforce lists US$500/100,000 Flex Credits and 20 credits for a standard action, plus a US$5/user/month access option that requires Flex Credits. Displayed prices are a dated snapshot, not Thai quotes or total ownership cost. [Bitrix24 pricing](https://www.bitrix24.com/prices/), [Request limit](https://helpdesk.bitrix24.com/open/24846136/), [Agentforce pricing](https://www.salesforce.com/agentforce/pricing/) | Record per-organization AI use and cost, bound requests, and enforce the operator budget. Avoid promising subsidized unlimited inference or adopting competitors' price points as validated ERPX prices. |
| Incumbent sales show commercial activity, not ERPX product-market fit. | Salesforce reported Agentforce ARR above US$1.5 billion on 2026-08-26, but that quarter's metric includes other AI offerings, Slackbot, and Headless 360. This vendor financial disclosure does not establish Thai service-business demand or independent customer ROI. [Q2 FY2027 results](https://investor.salesforce.com/news/news-details/2026/Salesforce-Delivers-Record-Second-Quarter-Fiscal-2027-Results/default.aspx) | Test repeat use, saved follow-up work, willingness to pay, and eventual actual payment. Do not infer market size or a success probability from incumbent revenue. |

## Accepted beta scope

### Delivery and admission

- One developer, one month: prioritize one reliable end-to-end conversation-to-completion journey.
- Public self-service registration for the first **20 organizations**, followed by a waitlist when capacity is exhausted.
- Free beta with **no payment card**. Admission must not create a paid subscription or promise automatic conversion after the beta.
- Default limits: **20 members, 5 branches, and 1 GB storage per organization**; **100 hosted AI calls per organization per month**.
- An organization maps to a Holding for isolation and entitlement purposes. Use **20 preallocated physical database-per-Holding slots**; claim a slot during onboarding and avoid placing multiple Holdings' business data in the same tenant database.
- Keep slot allocation and retry handling safe under concurrent onboarding. A failed or incomplete setup must not leave an organization mapped to another organization's resources.

### Social Home and staff work

- Make Home the social entry point for office and branch staff, with posting and discussion connected to work. Keep urgent work and pending approvals accessible.
- Reuse the existing post, staff-task, and approval capabilities where possible. Preserve the source-post link when a human creates a task from an AI draft.
- AI prepares an **editable private draft**. A person reviews the task content, responsibility, and timing before creating it; existing policies determine any approval required afterward.
- Let staff follow responsibility and completion from the original conversation. Task completion is the business outcome; drafting, posting, and liking are intermediate activity.
- Enforce Holding/branch access for context, drafts, tasks, attachments, and links. AI must not expand the requester’s authority or the audience of the source material.
- Normal posting and manual task creation must remain useful when AI is unavailable or its allowance is exhausted.

### Identity, infrastructure, and funded AI

- Prepare Better Auth registration/sign-in with **email verification, password reset, and invitations**, using **Resend** for delivery. Validate the configured flows with real email before public launch.
- Use the current Next.js/Bun/Cloudflare runtime and the preallocated Holding resources. A local demo or passing unit test is not evidence that all 20 production slots, bindings, migrations, or isolation paths work.
- The operator pays for hosted-beta inference and sets the **provider budget**. Enforce the default organization allowance alongside the global operator control; expose clear quota/provider failure behavior.
- Record hosting, AI, email, and support time per company. Avoid hidden inference loops or repeated spending caused by unsafe retries.

### Commercial preparation

- Prepare **Stripe test-mode** billing and its lifecycle handling during the beta. Enable live paid service only after the beta and an explicit launch decision.
- No actual selling price is validated. Existing numbers in [ADR-018](../../adr/ADR-018-open-source-modular-ai-and-hosted-stepper-pricing.md) remain pricing hypotheses, not proof of demand.
- Preserve ADR-018's broader open-source core, customer-funded BYOK per-module AI, and paid cross-module Flow/hosted-service direction. The user-approved **operator-funded hosted beta** is a narrower exception with a 100-call default; it does not silently impose that quota on customer-funded BYOK or rewrite the long-term model.
- No automatic paid conversion after the free beta. Obtain agreement to a specific offer and measure actual live payment when paid service launches.

## Pilot goals and decision gates

These are the user's accepted **experiment thresholds**, not industry benchmarks or achieved results.

| Measure | Accepted goal / evidence to collect |
| --- | --- |
| Activation | An organization has **at least 3 staff and at least 3 tasks created from posts completed within 7 days**. Count completed work, not AI drafts. |
| Retention | Among the **first 10 activated organizations**, **at least 5 return for three consecutive weeks**. Preserve the underlying usage evidence so “return” is not inferred from email opens or background jobs. |
| Operational value | Interview **at least 5 managers**, recording follow-up time and concrete dropped-work examples before and after use. Separate their reported experience from automatically measured task activity. |
| Willingness to pay | Before enabling live paid service, **at least 3 decision-makers agree to start a paid trial at an explicit price**. Stated agreement is not revenue; subsequently measure actual Stripe payment. |
| Unit economics | Track hosting, AI, email, and support time per company, including the operator's own business. Use observed costs and support effort when evaluating a paid offer. |

Use the pilot to decide whether the social entry point improves task follow-through, whether AI shortens task preparation enough to justify its cost, and which customer problem is repeatable. Do not add broad ERP or autonomous-agent scope merely because the feed attracts activity. If activation occurs but managers cannot show operational value or willingness to pay, revise the workflow or target segment before expanding paid infrastructure.

## Required verification and current status

The entries below remain the acceptance checklist. Local evidence and the external checks that still remain are recorded immediately after it.

- **Core journey:** create a post, request a private AI draft, edit it, create exactly the reviewed staff task, use the applicable existing approval, and complete the task with the source connection intact. Verify manual operation when AI is unavailable.
- **Authorization:** different Holdings and branches, unauthorized links/attachments, changed permissions between drafting and creation, and AI-derived content that would broaden the audience.
- **Provider behavior:** a real configured provider, invalid output, timeout/rate limit, exhausted organization allowance, operator budget cutoff, retries, and usage accounting. Local mocks are insufficient for provider readiness.
- **Admission/isolation:** first-20 capacity, the next registration reaching the waitlist, concurrent claims, interrupted setup, retry safety, and correct database/binding selection for all preallocated slots.
- **Identity/email:** verified registration, verification retry, password reset, invitations, expired/reused links, and actual Resend delivery with production-domain configuration.
- **Limits:** member, branch, storage, and hosted-AI defaults enforced on the server with understandable user feedback.
- **Payments:** Stripe test-mode checkout/lifecycle/webhook behavior without a live charge or automatic conversion. Live payment evidence remains a later gate.
- **Production:** deployed runtime/bindings/migrations, backups and recovery checks, logs without secrets, realistic mobile use, and end-to-end production smoke tests with real providers before opening registration.

Public-beta readiness and commercial viability require the remaining real-service checks and customer evidence; passing local tests does not establish either.

## Implementation and local evidence — 2026-09-07

Implemented against the existing, uncommitted Next.js workspace. No production deployment, real email delivery, real inference call or live payment was performed.

- Home now presents chronological, paginated team conversations with private saved drafts, authenticated images, comments, named mentions, appreciation, following, branch filters and a work strip. Updates redirects to Home; legacy `post`/`postId` links resolve to the new post permalink. Reports and owner moderation use audited commands. Summaries stay private; task preparation opens a reviewed form.
- Manual and AI-assisted task creation share the existing staff-task service and independent approval path. A unique post/task origin, receipt, task, notification, audit and outbox commit together. Source revisions, selected comments and participant access are rechecked. Linked-task reads also recheck source access. The original reporter can track the linked status while task details and evidence remain restricted to task participants. No automatic approval is performed.
- Better Auth now supports verified signup, recovery and existing-account invitation acceptance through Resend. Its Drizzle adapter explicitly maps existing epoch account dates and ISO session dates; no account tables were destructively rebuilt. Signup/verification/reset tests exercise actual Better Auth handlers with email delivery stubbed. See [Better Auth's adapter guidance](https://better-auth.com/docs/adapters/drizzle).
- A shared resource registry supports the app and jobs Worker. `wrangler.beta.jsonc` and `wrangler.beta.jobs.jsonc` describe 20 isolated D1 write/read pairs and 20 separate R2 buckets. Local initialization completed; control registry query returned **20 available slots**. Remote IDs are deliberately placeholders until provisioning runs. A durable control claim and tenant bootstrap owner prevent retries from assigning a tenant twice or recycling another owner's data.
- Beta limits and a server-side destination allowlist protect the limited product surface. AI reserves its monthly allowance before calling, caps two simultaneous requests per company and six requests per caller per minute, bounds input/output size, applies a timeout and validates structured output and source IDs. Failed calls retain usage because provider billing may have occurred. Monthly periods use UTC. Expired upload reservations and abandoned AI leases have bounded cleanup in the jobs Worker.
- Stripe test-only Checkout, Portal, HMAC verification, durable event inbox, current-state reconciliation and stale/duplicate handling are present. A Stripe test status never changes free beta access. Customer identity is bound to Holding. Live keys and live webhook events are rejected.
- Feedback is saved per company. `scripts/beta-metrics.sql` extracts activity and activation inputs. AI token usage is persisted; provider invoices, hosting/email costs, support time, manager interviews and willingness-to-pay evidence require operator collection.

Local verification:

| Check | Evidence |
| --- | --- |
| Integration | `bun run test:integration`: **75 tests across 12 files passed**, including new Social/AI, concurrent reservations, interrupted provisioning, verified invitations, signup/recovery, and Stripe signature/retry/order cases. |
| Types and lint | `bunx tsc --noEmit --pretty false` and `bun run lint` passed. Generated bundles and local rehearsal artifacts are excluded from lint. |
| Build | `bun run build:cloudflare` completed. Jobs Worker also bundled with `wrangler deploy --dry-run --config wrangler.beta.jobs.jsonc`. |
| Browser journey | Desktop and Pixel 7 runs completed post + photo → comment → manual task → evidence upload → independent acceptance, including AI-unavailable fallback and legacy post redirects. The fallback test verifies that a typed task title survives the AI failure. These ran against the built Cloudflare Worker at `http://localhost:3101`; four existing staff-task/routine desktop/mobile cases also passed. |
| Resource preparation | `bun scripts/beta-resources.ts --initialize`: all 20 local write/read/file slots initialized without customer business seeds. |
| Local recovery | `bun scripts/rehearse-beta-restore.ts`: isolated restore matched contents of 12 Holding tables and **32 attachment checksums**, with no foreign-key violations. Original resources remained in place. Local report: `.scratch/restore-1788796461299/report.json`. The script creates fresh isolated resources for each run and preserves source revisions while restoring triggers. |
| AI evaluation corpus | `fixtures/ai/social-beta-cases.json`: **32 Thai/English cases**, including ambiguous names/dates, missing information, explicit assignments and adversarial instructions. Live model evaluation has **not** run. |

## Operator setup and launch sequence

1. Keep `ERPX_PUBLIC_BETA=false`. Use the generated beta configs for the hosted pool; keep the existing root configs for local legacy fixtures. Review `BETTER_AUTH_URL` in the beta config and set the actual canonical HTTPS domain. Do not run business seed scripts against a customer pool.
2. On the intended Cloudflare account with Workers Paid, run `bun run beta:provision` to provision and initialize remote resources. This explicit command creates infrastructure, updates real D1 IDs in both beta configs, applies migrations and marks slots available only after schema checks. Re-running it resumes existing resources. The command has **not** been run remotely in this implementation.
3. Configure `BETTER_AUTH_SECRET`, `RESEND_API_KEY`, `RESEND_FROM`, `AI_BASE_URL`, `AI_MODEL`, `AI_API_KEY`, `STRIPE_SECRET_KEY`, `STRIPE_WEBHOOK_SECRET` and `STRIPE_PRICE_ID` as appropriate server secrets/vars. `.env.example` documents names without credentials. Use a verified Resend sender. AI base URL must be an operator-chosen HTTPS `/v1` endpoint; the model must support strict JSON schema in Chat Completions. Provider budget is set by the owner at the provider.
4. Limits default to 20 members, 5 branches, 100 monthly AI requests and 1 GiB. Env defaults are copied into the new tenant's `beta_policy`; an operator can update an existing policy server-side. A beta onboarding engagement supplies the minimal staff graph with Thai/THB defaults and no compensation, payroll or transactional samples. Revisit country/employment metadata before expanding to HR/payroll.
5. Build the app and deploy app/jobs with the same beta registry. The jobs Worker needs its configured D1/R2/Queue bindings, cron trigger and dead-letter queue. Check queue processing and cleanup. Use regenerated binding types via `bun run beta:typegen` when configuration changes.
6. Complete actual email verification, reset and invitation delivery on the deployed domain; verify the same email's existing account joins without account replacement. Test source and attachment links using two companies and revoked memberships. Repeat recovery on the hosted control/tenant/read databases and R2 backup procedure before accepting customer data.
7. Run `bun run beta:eval:ai` using the chosen provider. It issues 32 real inference requests and writes `.scratch/social-ai-evaluation.json`. Inspect the Thai/English summaries manually as well as checking references and unknown fields. Confirm quota, timeout, refusal and provider-budget behavior. Offline contract tests are not a substitute for model quality evidence.
8. In Stripe sandbox, configure an explicit test Price and Portal, register `/api/v1/stripe/webhook`, and exercise checkout, payment failure and cancellation with the actual signing secret. The implementation pins API version `2025-02-24.acacia`; match the webhook endpoint version when configuring it. Reconcile retained pending inbox events by retrying delivery. Beta stays free throughout.
9. Only after these deployed checks pass, enable registration in the beta config. The 21st admission is waitlisted. Do not reset a claimed slot to available; resolve its provisioning state instead. No automatic paid conversion, live billing activation or customer outcome has been claimed.

## Remaining external evidence

Real Resend delivery, the selected AI model's 32-case evaluation, actual Stripe sandbox sessions/webhooks, remote resource provisioning/deployment, and a hosted recovery rehearsal remain unverified. These need the operator's real service configuration. Public registration remains closed by default. The paid-launch price and the four-week business metrics remain future experiment outcomes.
