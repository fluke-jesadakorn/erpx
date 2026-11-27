# Company installation of owner-reviewed plugins

ERPX accepts declarative Tiles and Cards from external contributors. The protected `plugin-registry` GitHub branch is the approval source. An entry pins the GitHub repository's stable ID, commit and SHA-256 digest of `erpx-plugin.json`. GitHub search and pasted repository links discover candidates. Both paths use the same approved versions when a Company admin installs.

Company installation is independent of package entitlement and staff authority. An installation can expose only the explicit card sources and destinations registered by the ERPX owner. CRM continues to own appointment visibility. Withdrawing an installation removes its tiles and stops delivery for that Company. Installation audit and source records remain.

ERPX records business state in existing PostgreSQL owner transactions. A transactional outbox carries versioned events to installations. Delivery deduplicates by installation and event ID. This preserves existing records and owner workflows while allowing a suspended projection to catch up.
