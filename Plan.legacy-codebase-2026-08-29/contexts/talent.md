# Context — talent

**Crate:** `crates/contexts/talent` · **Proto:** `crates/proto/proto/erpx/talent/service.proto` · **Adapters:** `crates/adapters/talent-sqlx` (where present) · **UI:** `apps/console/src/features/talent` / `crates/ui-domains/*` / `crates/console-features/talent`

## Services & RPCs

CreateRequisition, ApproveRequisition, TransitionRequisition, CreateCandidateProfile, RequestCandidateErasure, CompleteCandidateErasure, SetCandidateLegalHold, SubmitApplication, AddApplicationInterview, SelectApplication, WithdrawApplication, CreateOffer, ApproveOffer, PresentOffer, DecideOffer, RequestHireHandoff, ReconcileHireHandoff, CreateProposal, SubmitProposal, ApproveProposal, ReconcileProposal, GetRequisition, GetCandidateProfile, GetApplication, GetOffer, GetHireHandoff, GetProposal, GetRecord

Total RPCs: 28.

## Domain (sample)

Aggregates / types: TalentRealm, TalentHumanDecisionEvidence

Source: `src/domain.rs` (and `src/application.rs` present).

## Ports & adapters

- Ports: `src/ports.rs`
- Application: `src/application.rs`
- Persistence: `crates/adapters/talent-sqlx` (SQLx, CQRS read/write split where applicable)

## Invariants

Enforced in `crates/domain` and `crates/contexts/talent/src/domain.rs` — validate via `cargo xtask test-affected` and `crates/contexts/talent` unit tests. Cross-context rules travel only through versioned proto contracts, never direct domain imports.
