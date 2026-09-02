# FSD-002 — Shared Platform Capabilities

- Status: Accepted
- Owner: Product Council
- Effective baseline: 1.0.0
- Supersedes: none

## Decision

ERPX provides Enterprise Home, universal work, in-app authoritative
notifications, governed imports and reports, federated authorized search,
distributed domain-owned reference data, provider-backed e-signature,
assistive AI, allow-listed extensions, progressive delivery, and governed
customer exit.

Web is the primary channel. E-commerce and Recruiting & Talent are Web-only;
the other nineteen workspaces target Web, Desktop, iOS, and Android. Offline
operation is read-only. Internal typed interfaces and ERPX-controlled adapters
are the only developer integration surface.

## Quantitative controls

Projection lag is at most 60 seconds, RPO at most 24 hours, RTO at most four
hours, a verified backup is no older than 25 hours, and PITR rehearsal occurs
within 31 days. Other performance measures are reported but not release gates
until a later accepted decision sets thresholds.
