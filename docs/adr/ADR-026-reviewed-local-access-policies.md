# Reviewed local access policies

**Accepted — 2026-09-16.** ERPX manages Company-owned groups, attributes and access policies without writing to connected directories. Automatic membership creates individual access proposals, not grants: approval of a rule or attachment does not authorize future beneficiaries. Every new grant retains ADR-025's independent Person review, delegation ceiling, expiry and selected-assignment boundary.

Policy eligibility is checked by database authorization as well as the application. Changes that remove eligibility permanently invalidate the affected source before a later rejoin can revive it; rejoining requires a fresh proposal. Reconciliation can propose work but cannot approve it. Source-specific removal preserves unrelated assignments. Ownership and recovery authority remain directly assigned, avoiding an automated rule removing the final recovery administrator.

Custom attributes are Company-scoped even when a Person has other identities or engagements. Attributes and local groups are ERPX metadata; external directory objects remain read-only. Moving staff between departments remains an HR operation because it changes workflows beyond access management.
