# 04 — Social Paradigm (ERP as Social Network)

> **Canonical engagement layer — 2026-09-03.** Every business record is a **Post**; every context is a **Group**; work is discovered via **Feed**, **@**, **#**, **Follow**. This doc is the single source all 31 per-context `Social surface` blocks point to (mirrors `_shared-contract.md`). System of record (aggregates, outbox, `HumanDecisionProof`, holding isolation) is untouched — social is engagement, never decision. See `docs/plan/01-vision-and-scope.md` (spine), ADR-017 (collaboration rails), ADR-019 (typed `@`), ADR-021 (this layer's decision).

## Metaphors (ERP ⇔ social)

| ERP concept | Social surface | Authority |
|---|---|---|
| Record (`sales/quote-2026-042`) | **Post** (feed card; links canonical record) | Owner context owns truth; post is projection |
| Context (`sales`) | **Group/Page** (`/sales` group home) | Same routes, social framing |
| Holding/Company/Branch/Team | **Audience circle** (visibility scope, never access grant) | ADR-017 §5 placement rules |
| State transition | **Post update event** (e.g., `Sent → Accepted`) | Owner domain validates |
| Approval request | **Role-lane card** (needs-your-role) | `HumanDecisionProof` still required |
| Comment thread | Existing `RecordActivityRail` (ADR-017 §1) | Collaboration owns infra |
| Backlink | "Mentioned in" line on the post | `ListBacklinks` |
| Spine journey (`_journeys.md:3`) | **Followable story chain** (chain card with edge labels) | `record_reference_edges` |
| AI suggestion | Draft assistant on the composer | ADR-019 lifecycle |

## Feed (newest-first + role lanes, no ranking algo)

Fixed section order, newest on top inside each:

1. **Role Lane** — items needing *your role*: approvals, SLA breaches (<2h support), period gates, incidents. Sorted by due-ness, then newness.
2. **Following** — strict reverse-chronological posts from followed records/people/tags.
3. **Hashtags** — followed-tag streams, same chrono rule.
4. **System** — holding broadcasts (period close, incidents via `BroadcastIncident`).

Rules: no engagement scoring, ever; freshness chips (`Fresh/Stale/Offline/Denied`) on every card; feed is a rebuildable projection — audit lives on records, not the feed; tombstoned content renders as tombstone with evidence link kept for authorized readers.

## Composer (one box everywhere)

`@`-picker → `ReferenceQueryService.search` (existing); `#`-picker → scoped tag vocabularies; per-context post templates live in each light file's `Social surface` block. Agent output enters as `proposed` draft; a human publishes (ADR-017 §4).

## Hashtag governance (3 scopes)

- **Local** (`#team-offsite`) — team/node visibility, no moderation.
- **Holding** (`#month-close`, `#win`) — moderated vocabulary, holding-wide discovery.
- **System-reserved** (`#incident`, `#period-gate`) — created only by owner contexts.
- Trends compute **within placement**: `#urgent` in Branch A never surfaces in Branch B without a Branch-to-Branch agreement; cross-holding tag reads denied + Global Read Audit (ADR-005).

## Follow graph + notifications

Follow record/person/tag → feed inclusion + inbox events (mention, lane assignment, digest). Digest mode via existing `ListMyDay`; unread counts only on lanes + mentions. Following never grants access (ADR-004/017).

## Guardrails (edge cases, resolved)

- *Like on a payment release* → nothing happens. Reactions are append-only signals (`SetReaction`), never transitions; the human-only registry (ADR-017 §4) is unchanged.
- *Tombstoned comment with evidence* → tombstone renders; evidence link persists for authorized; audit intact.
- *Agent drafts a post* → `proposed` state; human publishes with proof.
- *Cross-holding hashtag search* → denied, audited; no existence leak (labels resolved post-authorization).
- *Stale feed item* → `Stale` chip; never auto-refreshes into a decision surface.
- *DMs* → reuse `agent` private sessions + `support` sessions; no new DM aggregate. *Stories* → out of scope.

## UX bar (stunning & easy)

One-thumb mobile; optimistic post with outbox-backed true state; skeletons over spinners; empty states teach ("Follow `#month-close` to see close parties here"); every post links its canonical record; lane cards show exactly what proof is needed before acting.

## Living routes (→ `docs/contracts/next/route-table.md`, TBD)

`GET /feed` · `GET /today` · `GET /tags/:tag` · `GET /posts/:id` · `GET /notifications` — served by the existing 8 collaboration feed/hashtag RPCs (`collaboration.md:3,10`).

## Verification (→ per-batch E2E)

Post quote → followers' feeds newest-first; expiring quote → sales lane; `#month-close` follow → close digest; tombstone round-trip; **negative**: cross-holding tag invisible; 1,000 reactions move no workflow; chronological-order property test on projection rebuild; lane evaluation inside projection snapshot (ADR-017 Phase-1 pattern).

## Revision

Code SHA at PR time (Q18=A). Per-context rows: `docs/plan/MAP.md` Social column. Full RPC inventory stays in `docs/plan/traceability.md`.
