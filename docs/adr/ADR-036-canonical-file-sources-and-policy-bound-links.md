# ADR-036: Canonical file sources with policy-bound links

> **Accepted — 2026-09-27.** Extends ADR-017's production malware-scan gate and ADR-035's MinIO storage boundary.

## Decision

ERPX stores every uploaded original as an immutable, private File Source and grants business visibility only through a purpose-specific File Link in a File Set. A source is an upload identity with a pinned checksum and detected media type; identical checksums do not silently merge ownership. Record documents gain explicit revisions or superseding links. Evidence bound to a submission, receipt, inspection, decision, or other completed action is immutable, and corrections append a new source or link.

A server-only, closed `AttachmentPolicyRegistry` resolves the exact tuple `(targetType, purposeKey, componentKind)`. The resolver derives Company, Branch/Department scope, classification, audience, required count, format limits, retention snapshot, portal visibility and publication eligibility from the live source record. Clients cannot supply those properties. An unknown tuple, missing owner adapter, stale target version or changed policy revision fails closed.

## Lifecycle and binding

All user uploads enter quarantine and remain unreadable and unbindable until server-side content validation and malware scanning succeed. Original files are never public or rendered inline. Public product, storefront, and marketing media are separately approved, metadata-stripped derivatives whose publication records carry alt text, order, and revision.

Reservation does not mutate the owner. Binding rechecks authorization, policy revision, target version, required clean-file count and checksum-pinned manifest inside the same database transaction as the business command. A bound link cannot be updated, deleted or withdrawn. A clean unbound link expires after seven days; rejected or failed scans remain sealed until an approved security-destruction process exists.

## Ownership and reuse

The Company derived from the target is canonical. Reusing a source requires current read access to the source and write access to the destination, records the complete source/destination audience expansion, and is allowed only inside that Company. Public or portal-visible media cannot expand to a different owner. Legal preservation is the sole cross-Company exception and copies bytes into a new source instead of sharing a link.

Every link retains its own policy and retention snapshot. Legal Hold overrides cleanup, and a shared source is governed by the strictest and longest active obligation. Bound evidence has no automatic deletion unless an approved automatic retention policy explicitly permits it.

## Private/public boundary

Originals are served only as authenticated downloads. Inline display uses a server-generated, bounded, metadata-stripped image or PDF preview with `private, no-store`, `nosniff` and sandboxed content policy. A public URL can address only an independently approved Media Publication derivative; revocation removes public availability without changing the private source or audit history. Authentication portals may upload only to an explicit owner and purpose and receive only the variants allowed by that purpose.

## Migration and rollback

The existing `attachments` row remains the physical-source compatibility boundary for one release. Historical bytes receive canonical ownership and links, return to quarantine, and are scanned rather than presumed clean. Unknown ownership blocks activation. Rollback disables new reserve/bind operations but keeps clean bound evidence readable through authorized links; it never downgrades or deletes evidence.

## Consequences

This source/link split avoids both duplicate bytes and accidental privilege inheritance while allowing each target to retain its own access, requiredness, retention, and legal-hold obligations. When several links share a source, the strictest active obligation controls destruction.

The scanner, current signatures and bounded preview tools become release prerequisites. Storage reconciliation must cover originals, quarantine objects and derivatives exactly once, while adding another link consumes no physical-file quota. Projections such as ledgers, balances, reports, Today and Executive refer back to the source record instead of owning files.
