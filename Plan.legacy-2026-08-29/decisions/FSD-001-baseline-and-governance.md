# FSD-001 — Baseline and Requirements Governance

- Status: Accepted
- Owner: Product Council
- Effective baseline: 1.0.0
- Supersedes: none

## Decision

ERPX uses English Markdown as the canonical requirements source. Normative
requirements use stable leaf IDs, measurable acceptance paths, deterministic
evidence IDs, role-based sign-off, source authority, scenario coverage, and a
validated traceability matrix. The current working tree is the source snapshot.
Delivery status and roadmap data are excluded.

## Consequences

Requirements may cite repository sources or accepted FSDs. A Python
standard-library validator enforces structure, coverage, links, catalogs, and
whitespace without creating a duplicate YAML registry.
