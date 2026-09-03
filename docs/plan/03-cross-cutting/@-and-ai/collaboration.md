# Collaboration — Deep `@` + AI (Production)

**Group:** `13-ai-collaboration` · **Code:** `crates/contexts/collaboration` · **Proto:** `crates/proto/proto/erpx/collaboration/service.proto` (`CollaborationCommandService, CollaborationQueryService, CollaborationStreamService` · 34 RPCs — 26 collaboration + 8 feed: `ListFeed/GetFeedItem/SearchFeed/ListMyDay/ShareFeedItem/CreateHashtagTopic/FollowHashtag/ListHashtagTopics`) · **Adapters:** `crates/adapters/collaboration-sqlx` · **UI:** `apps/console/src/features/collaboration` / `crates/ui-domains/*` · **Migrations:** `crates/migration/migrations/000*_*.sql`

**Scope:** Extends `docs/plan/02-capabilities/13-ai-collaboration/collaboration.md`. Central polymorphic `@` contract in `docs/plan/01-vision-and-scope.md` applies verbatim — see `_shared-contract.md` for @ tokenizer, AI lifecycle, Forms, Verify, Spine, ASCII frame.

## Services & RPCs (34)

- **Services:** collaborationCommandService, collaborationQueryService
- **RPCs:** ** CreateThread, AddComment, EditComment, TombstoneComment, TransitionThread, SetReaction, SetFollowing, WithdrawMention, ModerateComment, ProposeActionCard, ConfirmActionCard, AddRecordRelation, RevokeRecordRelation, StartRecordThreadWithComment, PrepareAttachmentUpload, FinalizeAttachmentUpload, GetThread, SearchThreads, ListThreadsByRecord, ListMentionInbox, ListRecordRelations, GetThreadV2, ListThreadsByRecordV2, GetAttachmentDownload, AutocompleteTargets, Subscribe, ListFeed, GetFeedItem, SearchFeed, ListMyDay, ShareFeedItem, CreateHashtagTopic, FollowHashtag, ListHashtagTopics

| RPC | Service | Data touch |
|-----|---------|------------|
| `** CreateThread` | Command/Query | `collaboration` aggregate + outbox |
| `AddComment` | Command/Query | `collaboration` aggregate + outbox |
| `EditComment` | Command/Query | `collaboration` aggregate + outbox |
| `TombstoneComment` | Command/Query | `collaboration` aggregate + outbox |
| `TransitionThread` | Command/Query | `collaboration` aggregate + outbox |
| `SetReaction` | Command/Query | `collaboration` aggregate + outbox |
| `SetFollowing` | Command/Query | `collaboration` aggregate + outbox |
| `WithdrawMention` | Command/Query | `collaboration` aggregate + outbox |
| `ModerateComment` | Command/Query | `collaboration` aggregate + outbox |
| `ProposeActionCard` | Command/Query | `collaboration` aggregate + outbox |
| `ConfirmActionCard` | Command/Query | `collaboration` aggregate + outbox |
| `AddRecordRelation` | Command/Query | `collaboration` aggregate + outbox |

## Invariants (domain — keep)

- Comments/reactions/following are append-only with moderation tombstones; mentions are typed.

Generic 3 (holding-isolation / time-bounded / append-only) — see `_shared-contract.md`.

## Data owned (delta)

- Write: `collaboration` aggregate + `outbox_events` (CQRS, ADR-002) in holding/branch pair
- Read: rebuildable projection for search; `collaboration_threads/comments/reactions/mentions (0003..0006)` (light `collaboration.md:16` canonical)
- Reference graph: see `_shared-contract.md` `record_reference_edges` definition

## Editable fields & schema binding

- **This module:** ** no owner-published schema — OwnerUnsupported (spec-ahead candidate)
- Binding: see `_shared-contract.md` (`MAX_FIELD_PATCH_OPERATIONS=64`, `StableFieldRef`, etc.)

## Business flows (unique)

1. **Happy:** CreateThread → AddComment/Edit/Tombstone → Reaction/Following → ActionCard propose/confirm → RecordRelation — creates typed edges (e.g., `@sales/…`, `@finance/…`) and is queryable via `search`/`backlinks`.
2. **Exception:** see original deep — validates invariants + degraded `Stale/Degraded`.
3. **AI-assisted:** see `_shared-contract.md` (pins ≥3 refs → ghost → Apply/Conflicted).

## Feed mapping (paradigm: `docs/plan/04-social-paradigm.md`)

- **Emit:** threads ARE posts; `ShareFeedItem`→re-post with provenance; hashtag topics scoped per `CreateHashtagTopic`.
- **Comments:** canonical infra (`RecordActivityRail`); moderation tombstones propagate to feeds.

## Route & UI delta

- **Route:** `/work (threads) + /reference/item/:id` + `/work + /reference/item/:id`
- Shared frame: see `_shared-contract.md` ASCII (title COLLABORATION, list collaboration-001/002)

## Traceability

- **Proto:** `crates/proto/proto/erpx/collaboration/service.proto` + `reference/service.proto`
- **Domain:** `crates/contexts/collaboration/src/domain.rs`
- **Adapters:** `collaboration-sqlx`
- **UI:** `apps/console/src/features/collaboration`
- **Verify:** see `_shared-contract.md` (Q15=C E2E + `cargo xtask test-affected` for `collaboration`)

## Revision

- Code SHA: fill at PR time (Q18=A). See `docs/plan/MAP.md` for flat index.
