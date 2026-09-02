# Context — collaboration

**Crate:** `crates/contexts/collaboration` · **Proto:** `crates/proto/proto/erpx/collaboration/service.proto` · **Adapters:** `crates/adapters/collaboration-sqlx` (where present) · **UI:** `apps/console/src/features/collaboration` / `crates/ui-domains/*` / `crates/console-features/collaboration`

## Services & RPCs

CreateThread, AddComment, EditComment, TombstoneComment, TransitionThread, SetReaction, SetFollowing, WithdrawMention, ModerateComment, ProposeActionCard, ConfirmActionCard, AddRecordRelation, RevokeRecordRelation, StartRecordThreadWithComment, PrepareAttachmentUpload, FinalizeAttachmentUpload, GetThread, SearchThreads, ListThreadsByRecord, ListMentionInbox, ListRecordRelations, GetThreadV2, ListThreadsByRecordV2, GetAttachmentDownload, AutocompleteTargets, Subscribe

Total RPCs: 26.

## Domain (sample)

Aggregates / types: —

Source: `src/domain.rs` (and `src/application.rs` present).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/collaboration-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/collaboration/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/collaboration` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
