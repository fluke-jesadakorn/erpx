# ADR-021: Social engagement layer (ERP as social network)

> **Status:** Accepted (2026-09-03) — retained interaction/ownership decision; current supported flows are implemented in the owner services. Builds on ADR-017 (rails, human-only decisions) and ADR-019 (typed `@`, backlinks). Current behavior: [Workplace UX](../contracts/next/workplace-ux.md) and [catalogue](../contracts/next/catalogue.md).

## Context

Records are referenceable and commentable (ADR-017/019), and collaboration already ships feed/hashtag RPCs (`ListFeed`, `ShareFeedItem`, `CreateHashtagTopic`, `FollowHashtag`…), but there is no universal product language: users hunt 31 modules instead of following work. We want Facebook-grade ease (feed, post, `#`, follow) without weakening ERP rigor (holding isolation, expiring RBAC, audited human decisions).

## Decision

- **Every record renders as a Post** (projection; owner aggregate stays truth) in a **newest-first Feed** with fixed sections: Role Lane → Following → Hashtags → System. No engagement ranking, ever.
- **Hashtags** are scoped (local / holding-moderated / system-reserved) with placement-aware trends; **Follow** drives feed + notifications but never access.
- **Reactions are signals only** — the human-only transition registry (ADR-017 §4) is unchanged; a reaction can never approve/post/pay/release.
- **Agents draft, humans publish** — agent output enters `proposed`, same as field suggestions (ADR-019).
- No new DM aggregate (reuse agent private sessions + support sessions); stories out of scope.

## Consequences

- One new canonical doc (`04-social-paradigm.md`) + per-context `Social surface` deltas (31×2, additive); MAP/traceability/route-table gain feed rows; no RPC renames, no schema changes beyond the existing feed projection.
- New risks accepted with mitigations in the paradigm doc: notification fatigue (lanes + `ListMyDay` digest), hashtag anarchy (scoped vocabularies), feed/audit confusion (projection banner + canonical links), fan-out cost (outbox→projection path).
- Reversal cost is meaningful (UX language permeates 31 modules) — hence this ADR rather than a plan-only change.

## Evidence required

- E2E: newest-first ordering property test; lane surfacing by role; cross-holding tag invisibility; 1,000-reaction no-op proof; tombstone round-trip (`04-social-paradigm.md` Verification).
- `npm run lint` + `npm run build` pass; `docs/TOC.md` links the paradigm doc.
