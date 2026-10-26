# Apps owns Feed and replaces the Work product

Accepted 2026-10-26 for development. ERPX Apps is the main workplace: its root restores the current account's last authorized Company and Workspace and opens Feed; `/applications` retains Company → Workspace → App discovery. The separate Work application, host and SSO audience are retired without a redirect. Apps remains in `apps/launcher` at port3216, while explicit port assignments preserve the other applications and leave3201 unused.

This intentionally makes Apps a product rather than a shared discovery host. Feed and its existing pages/APIs are composed from `feature-apps` and the unchanged business owner packages. Keeping Work as a second application would leave competing home identities and a second SSO audience. Existing Work cookies are not migrated; Account and the other products retain normal SSO.

Commercial package IDs `work`, `work-social`, `work-messages`, `work-routines` and all their capability and dependency IDs remain immutable purchase identities. Their presentation owner is Apps; no grant, historical right, role or record authority is added or rewritten. `ERPX_APPS_PACKAGE_ENFORCEMENT` retains the previous development enforcement mode. My work, Work Item and `/work` remain business concepts.

Unknown development product hosts are rejected with generic404 before compatibility dispatch, including WebSocket upgrades. Work's exact Tunnel hostname, DNS records and cache-rule membership must be removed while preserving all other routes and the Tunnel catch-all. Cloudflare administration remains a separate completion gate when its existing token is unauthorized. This decision does not authorize staging access, private production deployment or migration acceptance.
