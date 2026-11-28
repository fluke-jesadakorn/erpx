# Company installation of owner-reviewed plugins

ERPX accepts declarative Tiles and Cards from external contributors. The [approval registry](../../contracts/plugins/registry.json) on the `codex/rust-backend-migration` branch is the approval source. The registry path requires code-owner review. An entry pins the GitHub repository's stable ID, commit and SHA-256 digest of `erpx-plugin.json`. GitHub search and pasted repository links discover candidates. Both paths use the same approved versions when a Company admin installs.

Company installation is independent of package entitlement and staff authority. An installation can expose only the explicit card sources and destinations registered by the ERPX owner. CRM continues to own appointment visibility. Withdrawing an installation removes its tiles and stops delivery for that Company. Installation audit and source records remain.

ERPX records business state in existing PostgreSQL owner transactions. A transactional outbox carries versioned events to installations. The CRM card reads the current appointment only when the matching activity version is present in the outbox. Replaying a committed event cannot produce a second card. On the next read, the card reflects committed changes and the current installation. CRM authorizes the appointment independently of the installation.
