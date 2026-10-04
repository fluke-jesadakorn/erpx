# ADR-019: Typed record mentions and human-reviewed field proposals

Accepted 2026-08-29; the typed-reference and human-review boundaries remain in effect. Current supported adapters, source/field schemas and workflows are in [the catalogue](../contracts/next/catalogue.md) and [Workplace UX](../contracts/next/workplace-ux.md). The original 31-module deep specs and Rust editable-schema constants are historical implementation descriptions.

## Decision

References identify typed owner records or principals rather than granting access through a string. Ordered/deduplicated relations, backlinks and mentions must resolve with the viewer's current scope and source permission; missing or unreadable targets cannot disclose private source metadata. Adding a reference, following or reacting never grants a role or records a business decision.

AI proposes values from pinned, authorized source versions, with evidence/provenance and a human-visible diff. Apply must recheck source version/digest, current actor authority and the owner's supported editable schema. Drift fails closed; no silent autofill, force-apply, automatic rebase or system-global read is authorized. Unsupported source/field adapters remain unavailable rather than accepting arbitrary payloads.

The original requirement for chips in every text field is a product direction, not proof that every current field implements it. Engineering acceptance covers only the named owner workflows and assertions in the scenario register. [ADR-017](ADR-017-universal-record-collaboration-and-human-approved-ai.md) retains the shared collaboration/decision ownership boundary.
