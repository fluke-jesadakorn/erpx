# ADR-023: Three-level workspace authorization and canonical slug routing

**Status:** Accepted (2026-09-06)

## Context

The former short-form Holding contract exposed storage-oriented identifiers and conflated Company and Branch placement with authorization levels. ERPX needs stable human-readable navigation while preserving opaque database routing and strict separation between control-plane and customer authority.

## Decision

ERPX has three authorization levels: Platform, Holding, and Local. Local roles are assigned to Branch or Department scopes; a Department nested beneath a Branch is still Local rather than a fourth level. Platform assignments are stored separately and never inherit into Holding data. Holding assignments authorize descendants, Branch assignments authorize the Branch and its nested Department subtrees, and Department assignments authorize only that Department subtree. The most-specific eligible active assignment wins unless an eligible selected assignment preference exists.

Public routes use immutable, parent-scoped slugs:

- `/platform/{destination}`
- `/holdings/{holdingSlug}/{destination}`
- `/holdings/{holdingSlug}/branches/{branchSlug}/{destination}`
- `/holdings/{holdingSlug}/departments/{departmentSlug}/{destination}`
- `/holdings/{holdingSlug}/branches/{branchSlug}/departments/{departmentSlug}/{destination}`

Installable Company storefront apps may use `/front/{holdingSlug}/{companySlug}/pos/` and `/front/{holdingSlug}/{companySlug}/pms/`. The Company slug identifies which app pair to install; it is not an authorization scope or a source of access. Every storefront request resolves the Company and any selected Branch or Department inside the Holding database, then revalidates the signed-in Principal's active assignment and lineage. Storefront entry may offer only Local workspaces the Principal can already open. POS and PMS business APIs continue to use the canonical Holding/Local authorization rules.

The API mirrors these workspace prefixes below `/api/v1`. There are no redirects or aliases from the removed short-form namespace. Holding slugs resolve through control-plane bindings; Local slugs and lineage resolve inside the selected Holding database. Opaque Holding IDs remain in storage keys, queues, audit evidence, and binding resolution.

Company remains a legal and data-policy boundary but is not a public URL or assignment level. Holding workspaces require a validated Company selection. Local workspaces derive Company from organization lineage and reject overrides.

Roles are scope-valid: `holding_owner` is Holding-only, reusable business roles declare their allowed Holding/Branch/Department scopes, and `platform_*` is reserved for Platform-only roles. Module discovery is gated by the selected effective assignment; direct module URLs remain presentationally accessible and are not a security boundary.

## Consequences

Displayed organization names may change without changing bookmarked URLs. Public links never expose opaque Holding IDs. Every authorization decision uses resolved IDs and validated lineage rather than trusting slugs or client preferences. The cutover is intentionally breaking, so integrations and bookmarks must adopt canonical routes immediately.

This ADR supersedes only ADR-001's public-routing clause; ADR-001's physical Holding-isolation decision remains authoritative.

The `/front/` Company slug is a deliberate routing exception for installable app identity only. Company-brand settings do not alter legal document issuers; the storefront apps remain online-only and do not cache Company business data for offline use.
