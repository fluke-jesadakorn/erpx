# ERPX workplace mockup

An English-first, interactive prototype covering the 31 ERPX modules. It uses vanilla HTML and JavaScript, Tailwind browser CDN 4.3.3, Lucide 1.41.0, shadcn-style HTML components, system fonts, original local SVG illustrations, and three local captioned video lessons. A network connection is required for CDN styling and icons; no build, account, or API is required. This remains a static UI prototype, not an installation of the React shadcn/ui library.

## Open the mockup

From the repository root:

```sh
python3 -m http.server 8017 --bind 127.0.0.1 --directory Designed/mockup
```

Open [the workplace](http://127.0.0.1:8017). A static server is recommended so video captions load correctly.

## Fewer steps, calmer screens

Home starts with three compact tasks, four everyday shortcuts, Stories, one composer entry and a chronological feed. At 390 × 844 the beginning of Stories is visible below the task panel. The single header context control shows the customer, branch and active role; its panel separates workspace selection from role selection. Demo personas remain under **Explore the demo**.

**Work** has To do, Waiting and Completed queues, live search, Filters, New work, Modules and one Connected journeys entry. Managers can select My work or Team work. **Modules** is a direct destination (`#/holdings/acme/modules`), also used by favorite management. Each module opens its first business section. Three primary sections remain visible; More contains other sections, Overview, All records and Activity. All original section and record links remain valid.

Tasks use one column. Fields needed now are visible; optional notes expand under Additional details. Details contains progress, connected work, backlinks and history; Discussion is the owning conversation. Assignment, assistant and help tools live in Task tools. Reference insertion appears when a field receives focus or the user types `@`; inserted chips remain visible.

On phones, a task replaces the main bottom navigation with one action bar. Back restores the originating route, filters, scroll position and focus. The navigation menu remains available. Session retention keeps an unfinished draft locally; **Save draft** explicitly updates the sample record without executing a business action.

### Walkthrough

1. **Leave:** as Mali, Request leave → complete the details → Submit leave request. The page shows who acts next. Preview Nisa, open the same task and inspect the read-only decision summary → Approve leave. The approver cannot edit the request in this screen. Ask for correction prepares an unsent discussion message.
2. **Support:** Triage ticket records the preparation once. Its resolution form appears beneath a compact success message. Enter the resolution, review it and explicitly resolve.
3. **Quote:** edit the quote and recipient → Review quote → Send quote. Wait for the external response. Record customer response is a separate action; acceptance prepares an order draft that still needs confirmation. No customer response, delivery or financial action is inferred.
4. **Supply:** Submit requisition → Nisa’s decision summary → Approve requisition. Open the prepared PO, review and release. Enter actual received quantity and attach evidence before accepting the receipt. Open Inventory’s prepared draft, review and receive stock. Each linked draft uses the source record’s values.
5. **Finance:** Nisa reviews and posts a balanced journal, then records close preparation once. Open the prepared period; Pim independently reviews and closes it. Nisa cannot bypass this rule by switching to Finance. New posting into the closed period is blocked.
6. **Quality:** the seeded CR-02 record demonstrates failure → passing recheck → release review. A newly created inspection records its observed result; a passing result proceeds directly to release review. A failed result requires rechecking.
7. **Customer Authority:** compare the displayed profiles without changing them → review the merge → confirm. The comparison itself creates no business history entry.
8. **Recovery:** Explore the demo → Save failure preserves the draft for retry. Handoff preparation fails records the successful decision and retries only draft creation. Record changes in another session prevents confirmation of an outdated review. Assignee becomes unavailable requires an eligible reassignment.
9. **Discussion:** Comment on a business-record post opens and focuses that record’s discussion in one navigation. Ordinary posts retain inline comments. Ask for help prepares text and a structured record reference but never sends it automatically.

### Measured action clicks

Counted after opening the correct task; typing, selection, evidence attachment, navigation and persona switching are excluded. These are deterministic mockup checks, not staff usability measurements.

| Path                                                                         | Before | Now |
| ---------------------------------------------------------------------------- | -----: | --: |
| Submit leave + manager approval                                              |      4 |   2 |
| Triage + resolve ticket                                                      |      4 |   3 |
| Requisition → approval → PO release → receipt acceptance → inventory posting |     10 |   8 |
| Journal posting → close preparation → independent close                      |      6 |   5 |
| Failed inspection → passing recheck → release                                |      6 |   4 |
| Compare duplicate profiles → merge                                           |      4 |   2 |
| POS sale completion                                                          |      2 |   2 |

A new passing inspection uses three action clicks: record the result, open release review, confirm release. The explicit action-policy registry preserves the other workflow reviews. Immediately eligible next actions on the same record appear without Continue; external waiting, another person’s responsibility and linked-record handoffs remain explicit.

## Explore

- **Mali Srisai:** Bangkok branch manager, with a separate Sales associate assignment. Switching assigned roles preserves Mali's identity.
- **Dao Arun:** warehouse specialist and receiving lead at the central warehouse.
- **Nisa Chen:** customer owner with all Acme companies and branches, a Finance assignment, and access to the second customer, Northstar Supply. Use the workspace switcher to move between them.
- **Arun Patel:** Northstar branch manager.
- **Fluke:** platform owner and support operator. The Platform Console contains service metadata and platform modules, without granting customer business access.

Use **Explore the demo** in the sidebar to change personas, try interface states, or reset the sample data. On a phone, open the navigation menu to find these controls.

Home balances personal work and social updates, with department and feed filters beside Team updates. Home and the former Community page shared a composer and feed; Community’s department filters now live on Home. Old Community links redirect to Home using history replacement while retaining the workspace boundary. Filters reset when the work context changes. Work links directly to the module directory and favorites. Learn has captioned Shorts; Inbox links notifications back to work. Records retain the original module fixtures, field types, lifecycles, and permitted cross-module references. Support, Talent, and Projects also have a board view.

### Full feature coverage and saved local demo

`capabilities.js` is the executable coverage manifest for all 31 modules. It groups the active capability specifications into user-facing feature pages instead of creating one page for every archived service operation. Each entry is reachable from its module’s **More** menu and opens an appropriate full workspace: operational tables, boards, timelines, ledgers, directories, document libraries, health checks, reports, or shared conversations. These pages use the same linked records and browser-local command layer, so actions update the owning record, history, summaries, and connected feed together.

The demo now saves records, posts, stories, threads, drafts, feature outcomes, notifications, handoffs, and attachment blobs in a versioned IndexedDB database named `erpx-connected-mockup`. Reloading restores this browser’s saved demo. The existing **Reset demo** confirmation clears saved business data and uploaded files, restores the seeds, and retains theme and favorites. If another tab saves a newer version, the interface asks whether to load it or keep the current tab’s state.

Record and task discussions are one shared conversation per owning record. Any related page opens the same thread. Messages support permitted typed references and real JPEG, PNG, WebP, or PDF attachments. CSV is additionally accepted by documented import surfaces. A submission accepts at most 10 files and 10 MB per file. Completion evidence only satisfies a workflow gate after the selected files are explicitly saved to the record.

External payments, customer delivery, provider connections, infrastructure changes, live bank feeds, and deferred tax filing remain clearly labeled local simulations. No mockup action calls a backend.

Company selection filters branch choices; single-company customers have a simpler selector. Branch and role determine permitted record views. Department selection targets team-feed audiences and the department of new records. Department names and business-function module groups are separate concepts.

## Demo behavior

- Posts, comments, Stories, follows, reactions, saved Shorts, read notifications, forms, and lifecycle changes work with temporary local data.
- Composer and record drafts are retained by person and work context during the session. Reactions, follows, saved Shorts, and read markers belong to a person in a customer workspace.
- Linked posts and Stories inherit the linked record's access restrictions. Search, feed previews, reference lists, and direct record routes apply the same simulated visibility rules.
- Stories appear for 24 hours. Expired items leave the Story viewer; their data remains in session memory. Linked records have an independent lifecycle.
- Consequential tasks use a focused review and append an attributed sample history entry. Appreciation never changes a business record.
- Failure mode makes the next save fail without losing the draft; retry succeeds. Stale mode requires refreshing the record before saving or confirming an action. Other controls demonstrate draft, loading, success, denial, and empty states.
- Reload or Reset Demo restores the fictional records. Only theme and module favorites persist in localStorage.

**This is a UI prototype, not an authorization or transaction system.** All sample data is delivered to the browser. There is no real authentication, payment, upload, message delivery, AI inference, notification service, or ERP transaction.

## Files

- `index.html`: static entry point.
- `styles.css`: responsive layout and light/dark tokens.
- `data.js`: preserved module fixtures, field definitions, lifecycles, and links.
- `app.js`: shared workplace shell, original record fixtures, social interactions, and navigation.
- `state.js`: in-memory seeds, customer/persona context, and shared visibility.
- `modules.js`: the 31 module workspace definitions and representative workflows.
- `workflows.js`: workflow records, prerequisites, reviewed actions, evidence, and draft suggestions.
- `workspaces.js` / `workspaces.css`: business workspace layouts and responsive record details.
- `connections.js`: reference editors, backlinks, topics, following, journeys, and moderation.
- `tasks.js`: scoped task ownership, stage bundles, action policies, and temporary draft handoffs.
- `task-views.js` / `tasks.css`: focused tasks, shared queues, reviews, inline outcomes, and responsive layouts.
- `task-events.js`: explicit saves/decisions, assignment, recovery, and disclosure restoration.
- `navigation.js` / `calm.css`: canonical shared presentation, context, live search, record views, scoped return navigation and phone action bars.
- `bootstrap.js`: initialization and read-only browser diagnostics.
- `assets/`: original illustrations and 12-second sample videos with English WebVTT captions.
- `verify.mjs`: browser interaction checks; screenshots go to `/tmp/erpx-mockup-review` by default.

## Browser verification

Keep the static server running. Install verification tooling outside the repository:

```sh
npm install --prefix /tmp/erpx-browser-tools playwright@1.55.0
```

Run using Google Chrome on macOS:

```sh
ERPX_PLAYWRIGHT=/tmp/erpx-browser-tools/node_modules/playwright \
ERPX_CHROME_PATH='/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' \
node Designed/mockup/verify.mjs
```

Alternatively install Playwright's Chromium and omit `ERPX_CHROME_PATH`:

```sh
/tmp/erpx-browser-tools/node_modules/.bin/playwright install chromium
ERPX_PLAYWRIGHT=/tmp/erpx-browser-tools/node_modules/playwright node Designed/mockup/verify.mjs
```

Optional environment variables: `ERPX_MOCKUP_URL` changes the server URL; `ERPX_SCREENSHOTS` changes the screenshot output directory.

The original regression checks cover all 31 module/record routes, persona and assigned-role switching, customer and branch isolation, search and previews, social interactions, scoped drafts, retry and stale states, reviewed actions and attribution, local video playback and captions, inbox navigation, keyboard focus, reset, mobile overflow, and dark mode.

## Docs-guided module expansion

Each module opens its first business section and retains an overview, all records, activity and a representative interactive workflow under its section navigation. Start with the named primary action, or open an assigned task from Home or Work. **Create quote**, **Request leave**, and equivalent actions create another temporary sample draft. Existing record IDs and links remain available under Records.

The scope remains UX/UI only: the archive’s Rust service names and the planned Next.js API routes are design references, not runtime dependencies. Current capability specifications and the shared interaction contracts guide the mockup; Stories and Shorts remain because they are explicitly requested.

### A few useful walkthroughs

- **Sales:** preview Nisa, open Sales, send the sample quote, record acceptance, and create the separately typed sales order. Fulfillment links onward to Finance.
- **Procurement:** submit and approve the requisition, create its purchase order and separate receipt, attach local sample evidence, and accept the receipt. The outcome links to Inventory.
- **Finance:** Nisa prepares and posts the balanced journal, then prepares the company period close. Nisa cannot close it by switching to Finance. Preview **Pim Wong**, open Finance → Records, and review the period-close record. The period closes for that company; a new journal cannot be posted into it.
- **Quality / maintenance / service:** passing temperature checks and local sample evidence are visible prerequisites. Use the record fields to correct a failed result before reviewing the next action.
- **Connected work:** use the + in Connected work, or type @ in a text field. Search records, colleagues, module groups, or the approved assistant pool. References deduplicate; arrows reorder them; target records show Mentioned in. Field references are retained with the saved sample record.
- **Draft assistant:** request a deterministic proposal, inspect source versions and the before/after preview, then Apply to draft. Close the assistant and use Save draft explicitly; applying a proposal does not change the record. Source conflict demonstrates a blocked application.
- **Topics and following:** use the topic links on feed cards or Collaboration → Topics. Follow people, records and topics independently. Feed sections are chronological within Following, Topics & team updates, and System.
- **Private collaboration:** Support distinguishes internal notes from sample customer replies. Internal notes never enter the general feed. Managers and owners can replace a record comment with a moderation tombstone while retaining its references and history.

### Workflow checklist and source references

Use the customer owner for customer modules and Fluke for Platform Administration and catalog publication. Customer 360 and customer billing have exploration paths with no write controls. Secondary sections are representative previews, not exhaustive replicas of every documented RPC.

| Module                  | Sections                                                   | Representative path                                                                     | Capability source                                                                                  |
| ----------------------- | ---------------------------------------------------------- | --------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| brand                   | Drafts, Identity preview, Version history                  | Preview identity → Publish version                                                      | [Specification](../../docs/contracts/next/catalogue.md)                   |
| commercial              | Catalog, Subscription, Invoices, Entitlements              | Review catalog → Publish catalog                                                        | [Specification](../../docs/contracts/next/catalogue.md)              |
| identity_access         | People, Assignments, Access reviews                        | Review renewal → Renew assignment                                                       | [Specification](../../docs/contracts/next/catalogue.md)         |
| organization            | Organization tree, Branches, Departments, Sharing          | Review placement → Activate branch                                                      | [Specification](../../docs/contracts/next/catalogue.md)            |
| platform_administration | Service health, Provisioning, Backups                      | Review retry → Retry provisioning                                                       | [Specification](../../docs/contracts/next/catalogue.md) |
| customer_360            | Profiles, Sales summary, Service timeline                  | Profile → accessible source record → return                                             | [Specification](../../docs/contracts/next/catalogue.md)               |
| customer_authority      | Bindings, Duplicate review, Access history                 | Read comparison → Review merge → Confirm merge                                          | [Specification](../../docs/contracts/next/catalogue.md)         |
| support                 | Tickets, SLA watch, Incidents                              | Triage ticket → Review resolution → Resolve ticket                                      | [Specification](../../docs/contracts/next/catalogue.md)                    |
| finance                 | Journals, Receivables, Payables, Banking, Close checklist  | Review journal → Post journal → Prepare close → Close period                            | [Specification](../../docs/contracts/next/catalogue.md)                   |
| pos                     | Register, Held sales, Receipts, Refunds                    | Review tender → Complete sale → Review refund → Confirm refund                          | [Specification](../../docs/contracts/next/catalogue.md)                       |
| sales                   | Quotes, Orders, Fulfillment                                | Send quote → Record acceptance → Create sales order → Confirm fulfillment               | [Specification](../../docs/contracts/next/catalogue.md)                     |
| assets                  | Asset register, Condition, Maintenance history             | Begin maintenance → Return to service                                                   | [Specification](../../docs/contracts/next/catalogue.md)                     |
| inventory               | Availability, Movements, Reservations                      | Review receipt → Receive stock                                                          | [Specification](../../docs/contracts/next/catalogue.md)                  |
| maintenance             | Work orders, Schedule, Downtime                            | Schedule order → Start work → Complete work                                             | [Specification](../../docs/contracts/next/catalogue.md)                |
| operations              | Exceptions, Area queues, Handovers                         | Acknowledge exception → Escalate to lead → Resolve exception → Close exception          | [Specification](../../docs/contracts/next/catalogue.md)                 |
| procurement             | Requisitions, Purchase orders, Goods receipts              | Submit requisition → Approve requisition → Create purchase order → Accept goods receipt | [Specification](../../docs/contracts/next/catalogue.md)                |
| quality                 | Inspections, Results, Release gates                        | Record result → Recheck only after failure → Review release → Release gate              | [Specification](../../docs/contracts/next/catalogue.md)                    |
| legal                   | Contracts, Document versions, Holds                        | Review contract → Activate contract → Archive contract                                  | [Specification](../../docs/contracts/next/catalogue.md)                          |
| people_payroll          | My requests, Team requests, Employees, Payroll summaries   | Submit leave request → Approve leave                                                    | [Specification](../../docs/contracts/next/catalogue.md)                 |
| crm                     | Pipeline, Accounts, Activities                             | Qualify opportunity → Record next step → Hand off to Sales                              | [Specification](../../docs/contracts/next/catalogue.md)                           |
| marketing               | Campaigns, Content preview, Channel readiness, Schedule    | Preview campaign → Verify readiness → Schedule campaign                                 | [Specification](../../docs/contracts/next/catalogue.md)                     |
| manufacturing           | Orders, Material readiness, Execution, Quality disposition | Release order → Reserve materials → Report output → Complete order                      | [Specification](../../docs/contracts/next/catalogue.md)                 |
| planning                | Scenarios, Shortages, Recommendations, Comparison          | Run scenario → Review recommendation → Release recommendation                           | [Specification](../../docs/contracts/next/catalogue.md)                           |
| projects                | Work breakdown, Board, Dependencies, Progress              | Link dependency → Approve dependency → Complete milestone                               | [Specification](../../docs/contracts/next/catalogue.md)                           |
| ecommerce               | Channels, Listings, Orders, Returns                        | Preview listing → Publish listing → Inspect sample checkout → Create sample order       | [Specification](../../docs/contracts/next/catalogue.md)                         |
| benefits                | Available plans, Enrollment, Dependents, Coverage          | Validate coverage → Confirm enrollment                                                  | [Specification](../../docs/contracts/next/catalogue.md)           |
| learning                | Assigned courses, Course catalog, Progress, Results        | Record attendance → Complete course                                                     | [Specification](../../docs/contracts/next/catalogue.md)           |
| talent                  | Requisitions, Applications, Interviews, Offers             | Review application → Record interview → Approve offer                                   | [Specification](../../docs/contracts/next/catalogue.md)             |
| field_service           | Dispatch schedule, Service orders, Parts, Visit evidence   | Schedule visit → Dispatch order → Complete visit → Release billing                      | [Specification](../../docs/contracts/next/catalogue.md)                 |
| agent                   | Agent catalog, Versions, Assignments, Proposals            | Approve version → Start assignment → Accept as draft                                    | [Specification](../../docs/contracts/next/catalogue.md)                      |
| collaboration           | Threads, Mentions, Following, Topics                       | Publish discussion → Mark answered                                                      | [Specification](../../docs/contracts/next/catalogue.md)              |

Shared guidance: [Social paradigm](../../docs/contracts/next/workplace-ux.md), [Reference and AI contract](../../docs/contracts/next/catalogue.md), and [Cross-department journeys](../../docs/contracts/next/catalogue.md).

### Expanded browser checks

Run each command with the same `ERPX_PLAYWRIGHT` and optional `ERPX_CHROME_PATH` environment variables shown above:

```sh
node Designed/mockup/verify.mjs
node Designed/mockup/verify-workspaces.mjs
node Designed/mockup/verify-connections.mjs
```

The workspace suite visits all module sections, exercises the 31 representative paths, checks save failure recovery and business prerequisites, and inspects 1440, 768, 390 and 320 px layouts. Its screenshots default to `/tmp/erpx-workspaces-review`. The connections suite checks reference ordering and backlinks, source conflicts, mentions, internal notes, tombstones, follows, private linked content, expired assignments, keyboard selection, and business limits.

## Task-first verification

With the local server running, use the same `ERPX_PLAYWRIGHT` and `ERPX_CHROME_PATH` environment variables shown above:

```sh
node Designed/mockup/verify-tasks.mjs
node Designed/mockup/verify-task-handoffs.mjs
node Designed/mockup/verify-calm.mjs
node Designed/mockup/verify.mjs
node Designed/mockup/verify-connections.mjs
npm run lint
npm run build
git diff --check
```

`verify-workspaces.mjs` is retained as a compatibility entry for `verify-tasks.mjs`. The expanded checks visit every module and primary section, exercise the focused workflows and failed-save retries, check requester/reviewer transitions and draft handoffs, preserve original record/social/reference/assistant tests, and inspect desktop, tablet, and 320–390 px phone layouts. Screenshots are written outside the repository under `/tmp/erpx-tasks-review`.

These are automated behavior and layout checks. They do not establish that real staff can complete tasks without instruction; comparative usability sessions with staff remain a product-review activity.

`verify-calm.mjs` checks the measured click targets, read-only approvals, changed approval versions, the pass-first inspection, direct module/task/discussion links, saved queue filters, return focus, the phone Home threshold and action bars under a contracted viewport. Browser viewport contraction approximates a phone keyboard; device keyboard behavior still merits a real-phone usability session.
