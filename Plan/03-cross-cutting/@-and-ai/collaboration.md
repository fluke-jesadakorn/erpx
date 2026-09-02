# Collaboration — Deep `@` + AI (Production)

**Group:** `13-ai-collaboration` · **Code:** `crates/contexts/collaboration` · **Proto:** `crates/proto/proto/erpx/collaboration/service.proto` (`CollaborationCommandService, CollaborationQueryService, CollaborationStreamService` · 34 RPCs — 26 collaboration + 8 feed: `ListFeed/GetFeedItem/SearchFeed/ListMyDay/ShareFeedItem/CreateHashtagTopic/FollowHashtag/ListHashtagTopics`) · **Adapters:** `crates/adapters/collaboration-sqlx` · **UI:** `apps/console/src/features/collaboration` / `crates/ui-domains/*` · **Migrations:** `crates/migration/migrations/000*_*.sql`

**Scope:** Extends `Plan/02-capabilities/13-ai-collaboration/collaboration.md`. Central polymorphic `@` contract in `Plan/01-vision-and-scope.md` (typed `StableRecordRef` + `MentionTarget`, ordered deduped append-only, soft cap 50, explicit reorder, backlinks/search) applies verbatim.

## Purpose & actors

- **Purpose (from proto/domain):** Owns collaboration lifecycle; see `crates/contexts/collaboration/src/domain.rs` and message names in `service.proto`.
- **Actors & scopes:** `AuthorityScope::{Platform,Holding,Company,Branch,Subtree}` via `crates/kernel`; RBAC via expiring `role_assignments` (ADR-004) + purpose-bound reads (ADR-005 `Global Read Audit`) where gated.

## Services & RPCs

- **Services:** CollaborationCommandService, CollaborationQueryService, CollaborationStreamService
- **RPCs (34):** CreateThread, AddComment, EditComment, TombstoneComment, TransitionThread, SetReaction, SetFollowing, WithdrawMention, ModerateComment, ProposeActionCard, ConfirmActionCard, AddRecordRelation, RevokeRecordRelation, StartRecordThreadWithComment, PrepareAttachmentUpload, FinalizeAttachmentUpload, GetThread, SearchThreads, ListThreadsByRecord, ListMentionInbox, ListRecordRelations, GetThreadV2, ListThreadsByRecordV2, GetAttachmentDownload, AutocompleteTargets, Subscribe, ListFeed, GetFeedItem, SearchFeed, ListMyDay, ShareFeedItem, CreateHashtagTopic, FollowHashtag, ListHashtagTopics
- **Social Feed (built-in):** `crates/kernel/src/feed.rs` + `crates/ui-foundation/src/feed.rs` + `crates/contexts/collaboration/src/feed.rs` + `crates/adapters/collaboration-sqlx/src/feed.rs` — single shared rebuilt projection `holding_write.feed_items` (outbox→projector), `hashtag_topics`/`hashtag_follows`; `FeedItemId`/`HashtagTopicId`, `validate_hashtag`/`normalize_hashtag` (`^[a-z0-9_]{2,64}$`, cap 20, dedup), `day_bucket` (UTC date) + label (`Today`/`Yesterday`/`%d %b %Y`), `FeedFilter`/`FeedItem`/`HashtagTopic`, `paginate_feed` cursor (`base64(millis:uuid)`), `is_visible_to`/`can_share_to` (narrow-only stricter-wins). No numeric job levels; level = `AuthorityScope` depth (`Platform>Holding>Company>Branch>Subtree`).
- **Key surfaces:** Command writes are transactional with outbox; Query reads are projection-backed with `has_more`/`next_before_*` pagination; Stream where listed is `CollaborationStreamService`-style subscription.


## RPC table (code-derived)

| RPC | Service | Data touch |
|-----|---------|------------|
| `CreateThread` | Command/Query per service | `collaboration` aggregate + outbox |
| `AddComment` | Command/Query per service | `collaboration` aggregate + outbox |
| `EditComment` | Command/Query per service | `collaboration` aggregate + outbox |
| `TombstoneComment` | Command/Query per service | `collaboration` aggregate + outbox |
| `TransitionThread` | Command/Query per service | `collaboration` aggregate + outbox |
| `SetReaction` | Command/Query per service | `collaboration` aggregate + outbox |
| `SetFollowing` | Command/Query per service | `collaboration` aggregate + outbox |
| `WithdrawMention` | Command/Query per service | `collaboration` aggregate + outbox |
| `ModerateComment` | Command/Query per service | `collaboration` aggregate + outbox |
| `ProposeActionCard` | Command/Query per service | `collaboration` aggregate + outbox |
| `ConfirmActionCard` | Command/Query per service | `collaboration` aggregate + outbox |
| `AddRecordRelation` | Command/Query per service | `collaboration` aggregate + outbox |
| … | … | … |

## Social Feed — Facebook-like Operations Timeline with Level-Based Visibility (Built-in, All Workspaces + Home Aggregation)

- **Built-in:** every context auto-publishes to one shared rebuilt projection; every workspace exposes `Feed` tab at `/<workspace>/feed` and inline record threads; Home (`/`) aggregates all workspaces into one day-grouped infinite scroll with workspace chips, level badges (`Platform/Holding/Company/Branch/Subtree` via `level_badge_for_scope`), and scope-filter chips. No per-context opt-out in v1.
- **Single shared source:** one rebuilt `feed_items` in `holding_write` (`0012_social_feed.sql`) + `hashtag_topics`/`hashtag_follows` (control mirror `0008_social_feed.sql`). Outbox from all 31 contexts projects to same table (`holding_id, anchor_record_ref, author, audience, visibility_scope JSONB, classification, created_at, day_bucket, hashtags TEXT[], record_refs JSONB, share_of, reaction_counts, reply_depth ≤3`). Indexed `(holding_id, day_bucket DESC, created_at DESC, feed_item_id)`.
- **Visibility:** `AuthorityScope.contains()` hierarchy — CEO at `Holding` sees all threads/comments/reactions/attachments in that holding; managers at `Company`/`Subtree` see only their subtree; branch staff sees only branch. No new RBAC. `@` mention validation via `ReferenceQueryService` + `validate_mention_targets`; out-of-scope renders as tombstone. Roles do not filter reads (scope-only), only gate writes (post/react/share/follow).
- **Contracts (extend `CollaborationQuery/CommandService`):** `ListFeed { holding_id, authority_scope, actor_principal_id, filter{contexts[], day?, hashtag?, topic_record?, author_me?, following_only?}, page_size, cursor } -> { feed_items[], next_cursor, authorized_count, freshness }` + `GetFeedItem`, `SearchFeed` (via `ReferenceQueryService.search` index), `ShareFeedItem { source_item_id }` (copies `Audience`/`classification`, stricter-wins), `ListMyDay { date }`, `CreateHashtagTopic`, `FollowHashtag`; reuse `SetReaction`/`AddComment(parent_comment_id)` (depth ≤3). Home calls `ListFeed(filter.contexts=[])`; workspace tab calls `filter.contexts=[<slug>]`. Auth via `AuthorizedFeedAccess` + `viewer_scope.contains(item.visibility_scope) && audience.includes(principal)`.
- **UX (single global + per-workspace):** `Home /` — infinite scroll, day separators (`Today / Yesterday / 28 Aug 2026`), composer with inline `@` chip picker (`AutocompleteTargets`) + `#` autocomplete, filter bar by context/day/topic/role. Each card: author avatar, record chip(s) `@ctx/id`, hashtag chips, body, action row (Like/React, Comment, Share, Follow), nested replies, `Stale/tombstone/offline` states. `My Day` — `ListFeed author_me=true` grouped by day. `Topic page` — `ListThreadsByRecord` + `ListFeed(topic_record)` merged. All writes inherit parent `Audience`; reactions/comments/shares cannot broaden audience, only `NarrowAudience` (`can_narrow_to`).
- **Cross-cutting:** notifications via `opaque_mention_inbox` + `followers` → `feed_item_created / mention / reaction / share`; `ListMentionInbox` remains inbox; `Subscribe` stream emits feed deltas.
- **Implementation:** `crates/kernel/src/feed.rs` (shared kernel) + `crates/ui-foundation/src/feed.rs` (view helpers `FeedCardView`/`DayGroupView`/`group_by_day`/`HomeFeedFilter`/`ComposerState`/`HomeAggregation` + 31 `all_workspace_slugs()`), `crates/ui-domains/enterprise-workspaces/src/feed.rs` (`WorkspaceFeedState`/`HomeFeedState::aggregate()`), `apps/console/src/feed_surface.rs` (HomeFeedPage + WorkspaceFeedPage, `level_badge_for_scope` + day grouping + composer validation, `WorkspaceResource` intercepts `resource==feed`), `apps/console/assets/feed.css`, `apps/console/src/routes.rs` (`Home` at `/`, per-workspace `/<ws>/feed`). Global `Holding` owns projection; no per-workspace tables/unions; chronological + day groups, no algorithmic ranking v1.

## Invariants (from domain.rs / domain crate)

1. All writes are holding-isolated (ADR-001/013); sharing is explicit, never implicit.
2. Assignments/roles are time-bounded and always expire; elevation is break-glass, dual-controlled, time-boxed.
3. State machines are append-only with tombstone moderation; transitions are validated server-side and mirrored in projections/outbox (ADR-002).

## Data owned

- **Write tables / outbox:** `collaboration` aggregate tables + `outbox_events` (CQRS, ADR-002) in holding/branch data pair; migrations `000*` per context.
- **Projections/read:** rebuildable read models consumed via Query RPCs; indexed for `ReferenceQueryService.search` ranking.
- **Reference graph:** shared `record_reference_edges(host ContextId+id, target StableRecordRef/MentionTarget, kind, ordinal)` — append-only, deduped by target, soft cap 50 per host, explicit reorder, backlink-indexed for `ListBacklinks`.

## Editable fields & schema binding (Q10=A, schema-bound)

- **Source of truth:** `crates/integration-contracts/src/editable_fields.rs` + `catalog::ContextId` + `CURRENT_RECORD_POLICIES`.
- **This module:** no owner-published schema — OwnerUnsupported (spec-ahead candidate)
- **Binding:** `RecordFieldPatch { source: RecordVersionRef, schema_version, changes: Vec<SuggestedFieldChange> }` validated against `record_editable_field_schema(record_type)`; `supports_agent_patches() == OwnerPublished && !fields.is_empty()` required; `MAX_FIELD_PATCH_OPERATIONS=64`, `MAX_FIELD_EVIDENCE_REFS=16`; `StableFieldRef` per field, `previous_value_digest` (SHA-256) per change, `confidence_basis_points` ≤10000.

## `@` record mentions — per-module

- **Surfaces:** inline typed chips inside every text field of this module’s forms (see Forms below) + picker/search `@`-prefix via `ReferenceQueryService.search` (ranked) + host detail page ordered chips + `ListBacklinks` panel.
- **Tokenizer:** `@<context>/<id>` → `StableRecordRef` record edge; `@user/@group/@agent-pool` → `PrincipalRef/MentionTarget` principal edge; both typed, not free-text.
- **Edge roles (typed):** dependsOn, blocks, billedBy, fulfills, authoredWith, quotedFrom as applicable; `kind` is an attribute — flat ordered list is storage primitive (Q8=A+B).
- **Multi-reference:** host may hold ≥3 edges so coverage is start→present (cross-module spine: `projects` → `procurement`/`inventory` → `sales` → `finance` → `assets`/`operations` → `support`/`field_service`).
- **Resolve & degraded:** `ReferenceQueryService.resolve` with `AuthorityScope`+purpose; denied/missing → tombstone chip + `Global Read Audit` (ADR-005); chips render `Stale/Degraded/Offline` when source version lags or index stale (Q16=A).

## Per-module field AI — read refs, ghost-fill form (Q4=all, Q5=A, Q10=A)

- **Capability:** `SuggestionCapability::RecordFieldPatch { owner: collaboration }` (plus any existing `sales.quote.compose`/`finance.*` where `suggestion.rs` already defines them).
- **Suggestion:** materialized `RecordFieldPatch` built from all pinned refs; each change carries `evidence: Vec<FieldSuggestionEvidence::Record{ record: RecordVersionRef }>` per source + `FieldSuggestionProvenance { provider_kind, model_ref, prompt_digest }` + `output_digest_sha256`; `schema_version` pinned at generation.
- **Lifecycle:** `Queued → Running → Ready | Conflicted | Failed → Applied | Rejected`; human `Apply`/`Reject` only; ghost/diff preview inside host form (inline chips + single `FieldSuggestionCard` rail, Q13=B); no silent write.
- **Versioning:** on `Apply`, server re-validates `current_source_version` + `current_payload_digest_sha256` against pinned evidence; drift → `Conflicted` (Q11=A, strict, no auto-rebase, no force-apply).
- **Auth:** AI reads every source **as the human author** (`AuthorityScope`+purpose) via `ReferenceQueryService.resolve/search`; unreadable sources omitted and flagged in evidence; `Apply` re-checks scope (Q12=A, no system-global read).

## Forms & views — granular fields

- **Host detail:** header with ordered reference chips (reorder handles), backlinks panel paginated after 50, thread/comments if module hosts collaboration.
- **Forms:** each editable field above renders as input (kind-appropriate: Text/Integer/Date/DateTime/Choice/Money), validated per `EditableFieldDefinition` (`required`, `max_text_bytes`, `minimum/maximum`, `choices`, `classification`, `risk`); `@`-chips inject inline in any Text field; picker surfaces show `@`-prefixed search results.
- **AI rail:** single `FieldSuggestionCard` per host shows per-field proposed value, `Evidence` pills (source ref+version), `Provenance` line, `output_digest`; `Apply`/`Reject` buttons enforce human approval.
- **Access & audit:** RLS per holding/branch (ADR-001) + expiring RBAC (ADR-004); every resolve/apply audited (ADR-005).

## Business flows (real, cross-module via `@` + AI)

1. **Happy path:** CreateThread → AddComment/Edit/Tombstone → Reaction/Following → ActionCard propose/confirm → RecordRelation — creates typed edges (e.g., `@sales/…`, `@finance/…`) and is queryable via `search`/`backlinks`.
2. **Exception/reversal:** exception: ModerateComment — validates invariants and degraded states.
3. **AI-assisted:** user pins ≥3 refs (e.g., `@projects/proj-…`, `@procurement/req-…`, `@finance/inv-…`), requests `RecordFieldPatch` → ghost preview materialized from pinned versions → `Apply` only on version match else `Conflicted` → re-request; covers `field_service`/`assets` ↔ `maintenance` cross-links.

## Data & contracts

- **Write:** host record + outbox → `record_reference_edges(host,target,kind,ordinal)` (append-only, deduped, indexed); field patch via `Agent*Service.RequestFieldSuggestion / Start / Complete / ApplyFieldSuggestion / RejectFieldSuggestion` (see `crates/contexts/agent-orchestration/src/suggestion.rs` + `suggestion_application.rs`).
- **Read:** `ReferenceQueryService.Resolve/Search/ListBacklinks` + `CollaborationQuery/Stream` where applicable.
- **Invariants preserved:** RLS/isolation, CQRS/outbox, human-approved AI (Q5=A), transfer/scoping protections; no free-text mentions, no silent auto-apply, no system-global read, no auto-rebase.


## Example referenceable IDs

- `@collaboration/example-28-01-2026-0001` → `/reference/item/:id` via `ReferenceQueryService.Resolve`; viewer scope enforced; missing/unauthorized → tombstone chip.
- Additional examples: `@projects/proj-2026-001`, `@sales/quote-2026-042`, `@finance/journal-2026-117`, `@inventory/move-2026-883`, `@support/ticket-2026-509`.

## Tables & projections (code-derived)

- **Writes:** context-owned aggregate tables (one per aggregate) + `outbox_events` (transactional, ADR-002); holding-isolated (ADR-001/013).
- **Reads:** rebuildable projections indexed for search ranking; `erpx_control.*` and `erpx_*` read tables per migration prefix.
- **Reference edges:** `record_reference_edges` (host_ctx, host_id, target_ctx, target_id, kind, ordinal, created_by, created_at) — append-only, `UNIQUE(host,target)`, `CHECK ordinal < 50` soft cap, indexed `(target_ctx,target_id)` for `ListBacklinks`.

## Field-level detail (granular)

- Each Text field with `@`-chips uses `contenteditable` chip model: chip is typed span with `data-ref="context/id"` + `data-kind` + `data-ordinal`; serialization strips to edge list on save.
- Picker (`ReferenceQueryService.Search`) ranks by recency + textual match + scope proximity; `@`-prefix triggers debounced search; keyboard nav + `Enter` inserts chip.
- Validation: `required`/`max_text_bytes`/`choice` enforced server-side; `classification` (`Public<Internal<Confidential<Restricted`) propagated `stricter-wins`; `risk=Moderate` fields require explicit permission `human_apply_permission` on `Apply`.

## Flow walkthroughs — step detail

**Flow 1 — Happy:** actor with `AuthorityScope=Holding` creates host → adds 2–3 typed chips via inline insert + picker → saves (outbox emits `ReferenceEdgesAppended`) → search finds host by chip text → target detail `ListBacklinks` shows host with role labels → audit trail includes `correlationId`.

**Flow 2 — Exception:** host references denied target → chip renders tombstone + `Global Read Audit` entry; or `Resolve` returns `Offline` → chip shows `Stale` with retry; or version drift before AI `Apply` → `Conflicted` with `expected vs current` digests, requires `Re-request`.

**Flow 3 — AI-assisted:** user pins refs `@a`, `@b`, `@c` → `RequestFieldSuggestion(owner=collaboration, hostId, pinned=[RecordVersionRef...])` → `Queued→Running` (worker reads each source as human via `Resolve` with purpose=`suggestion`) → `Ready` with `FieldSuggestionCard` (per-field ghost diff, `Evidence` pills with `record+version`, `Provenance` line, `output_digest`) → human `Apply` (server re-validates `current_source_version` + `previous_value_digest`) → `Applied` or `Conflicted` → `Reject` clears rail; `Failed` surfaces evidence gap.

## UI placement

- Host detail header: ordered chips row (drag-reorder where allowed, explicit save creates new ordinal).
- Inline form: chips inside inputs with caret-aware insert; `@` mention autocomplete dropdown anchored to caret.
- AI rail: right/bottom `FieldSuggestionCard` with per-change diff, confidence basis points, and `Apply`/`Reject` (no auto-write).

## Out-of-scope / not fabricated

- `catalog` and other thin contexts without proto services are marked `spec-ahead`; no fake RPCs are invented — spec defines intended contract as if `Create/List/Get` existed, flagged explicitly.
- No free-text mentions; no silent auto-apply; no system-global read; no auto-rebase on `Conflicted`.

## Verification (Q15=C)

- **E2E:** Chromium: type `@` in a field → picker resolves via `search` → chip persists after reload → `ListBacklinks` on target shows host (ordered, deduped, multi-ref ≥3, role-labeled); request AI on host with N refs → card shows diff with evidence/provenance → `Apply` writes patch only when `current_source_version`/`digest` match, else `Conflicted`; `Reject` clears. Covers tombstone for denied/missing and `Stale` chip.
- **Harness:** `cargo xtask test-affected` for `crates/contexts/collaboration` + `reference`/`collaboration`/`agent-orchestration`; disposable-DB round-trip for edges and patch lifecycle (`Request→Start→Complete→Apply/Reject`) with digest/pinned-version checks; outbox → projection convergence.


## Cross-module spine (canonical example)

`projects/proj-2026-001` —dependsOn→ `procurement/req-2026-014` —fulfills→ `inventory/move-2026-883` —billedBy→ `finance/journal-2026-117` —authoredWith→ `people_payroll/assign-2026-042` —blocks→ `support/ticket-2026-509`. Any host may hold this chain; `ReferenceQueryService.Search` and `ListBacklinks` make it navigable end-to-end.

## Security & compliance

- RLS: row-level holding/branch isolation in every query; `AuthorityScope` checked on `Resolve` and `Apply`.
- Audit: Global Read Audit + suggestion provenance (`provider_kind`, `model_ref`, `prompt_digest`, `output_digest_sha256`) stored immutably.
- No system-global read: AI worker impersonates human author; denied refs flagged, never silently omitted.


## Notes

- Keep <170 lines nominal; overflow goes to `adr/` not here.
- Chips are UTF-8 safe: `@` tokenizer respects word boundaries, not inside emails/URLs.


## ASCII UX — Collaboration (document-first)

Route: `/work (threads) + /reference/item/:id` · Reference: `@` chips + `AI FieldSuggestionCard` + `ListBacklinks`

```ansi
[90m┌─ ERPX Console ─────────────────────────────────────────────────────────┐[0m
[90m│[0m [1mCOLLABORATION [0m [90m│[0m Holding: Acme (H-001)  Branch: BKK-01  [90m│[0m [32m@user/me[0m [90m│[0m
[90m├─ Sidebar ────┼─ List / Workspace ──────────────────┼─ AI Rail ───────┤[0m
[90m│[0m /work       [90m│[0m [1m/work (threads) + /reference[0m [90m│[0m [33mFieldSuggestionCard[0m [90m│[0m
[90m│[0m /finance    [90m│[0m [ ] collaboration‑001  [32m@projects/proj-2026-001[0m  [90m│[0m evidence:       [90m│[0m
[90m│[0m /sales      [90m│[0m [ ] collaboration‑002  [32m@sales/quote-2026-042[0m   [90m│[0m  [36m@finance/journal-2026-117[0m[90m│[0m
[90m│[0m /procurement[90m│[0m [x] collaboration‑003 ◀ [32m@inventory/move-2026-883[0m[90m│[0m  [36m@procurement/req-2026-014[0m [90m│[0m
[90m│[0m /inventory  [90m│[0m     chips: [32m@projects/proj-2026-001[0m     [90m│[0m  provenance:     [90m│[0m
[90m│[0m /projects   [90m│[0m            [31m[tombstone][0m [90mStale[0m [90m│[0m  [90mollama/mistral[0m [90m│[0m
[90m├──────────────┼─ Detail + Form ─────────────────┼─────────────────┤[0m
[90m│[0m Backlinks  [90m│[0m Title: [32mcollaboration detail #003[0m              [90m│[0m [Apply] [Reject] [90m│[0m
[90m│[0m [36m@finance/…[0m [90m│[0m Notes: hello [32m@customer_360/note-001[0m  [90m│[0m ghost diff:     [90m│[0m
[90m│[0m [36m@support/…[0m [90m│[0m        + picker [90m[@…][0m          [90m│[0m  - old → [32m+ new[0m  [90m│[0m
[90m│[0m [90m(refs: 3/50)[0m[90m│[0m Status: Ready | [31mConflicted[0m on drift [90m│[0m digest: a1b2… [90m│[0m
[90m└──────────────┴─────────────────────────┴─────────────────┘[0m
```

Fallback (plain):

```
+-- ERPX Console ----------------------------------------------------------+
| COLLABORATION  | Holding: Acme  Branch: BKK-01  | @user/me           |
+-- Sidebar ----+-- List / Workspace ------------+-- AI Rail -------------+
| /work        | /work (threads) + /reference | FieldSuggestionCard |
| /finance     | [ ] collaboration-001  @projects/proj-2026-001 | evidence: @finance/... |
| /sales       | [ ] collaboration-002  @sales/quote-2026-042    | @procurement/...       |
| /procurement | [x] collaboration-003 < @inventory/move-2026-883| provenance: ollama     |
| /inventory   |     chips: @projects/proj-2026-001    | [Apply] [Reject]       |
| /projects    |            [tombstone] Stale       | ghost diff: -old +new  |
+--------------+-- Detail + Form ---------------+------------------------+
| Backlinks    | Title: collaboration detail #003       | Status: Ready/Conflicted|
| @finance/... | Notes: hello @customer_360/note-001 + picker [@] | digest: a1b2 |
| (refs: 3/50) |                                    |                        |
+--------------+------------------------------+------------------------+
```

### Spec paired to this mockup

| Field | Kind | Required | @-able | AI suggestible | Validation / RBAC | Notes |
|-------|------|----------|--------|----------------|-------------------|-------|
| — | — | — | — | — | OwnerUnsupported (`spec-ahead`) | no runtime schema yet; patch validated fail-closed |
| @-chips (all Text) | Chip | — | yes (inline) | — | `AuthorityScope` read, tombstone on deny, `Stale/Degraded` on offline | typed `StableRecordRef`/`MentionTarget`, ordered deduped, cap 50 |
| AI rail | Card | — | — | owner=`collaboration` `RecordFieldPatch` | `human_apply_permission` on Apply, `previous_value_digest` + `current_source_version` re-check → `Conflicted` | materialized `Evidence`/`Provenance`/`output_digest`, human-approved only |

- **Cross refs in this frame:** `@projects/proj-2026-001` (dependsOn), `@inventory/move-2026-883` (fulfills), `@finance/journal-2026-117` (billedBy), `@support/ticket-2026-509` (blocks) — covers spine.
- **Lifecycle shown:** `Queued→Running→Ready` ghost vs `Conflicted` on drift; `Reject` clears; `Failed` = evidence gap.

## Traceability

- **Proto:** `crates/proto/proto/erpx/collaboration/service.proto` + `crates/proto/proto/erpx/reference/service.proto`
- **Domain:** `crates/contexts/collaboration/src/domain.rs` (or `crates/domain/src/*.rs`)
- **Contracts:** `crates/integration-contracts/src/editable_fields.rs`, `crates/integration-contracts/src/catalog.rs`, `crates/contexts/agent-orchestration/src/suggestion.rs`
- **Adapters:** `crates/adapters/collaboration-sqlx`, `crates/migration/migrations/000*_*.sql`
- **UI:** `apps/console/src/features/collaboration` / `crates/ui-domains/*`
- **Verify:** `cargo xtask test-affected --dry-run && cargo xtask test-affected` (owning crate); `cargo xtask verify` for full gate.

## Revision

- Code SHA: fill at PR time (Q18=A). Every edit to this file must update `Plan/traceability.md`.
