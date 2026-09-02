# Manual All-Modules E2E — 29 workspaces + roles (captured via browser)

Evidence generated 2026-08-29 from live `http://127.0.0.1:3004` (server pid 21070).
Base Chromium CDP flow: WebAuthn virtual authenticator → `local:dev-controller` password sign-in → role selection (`platform_holding_administrator`, `finance_viewer`, `customer_security_head`) → per-workspace `.workspace-route` probe.

Screenshots in this folder are the Chromium CDP captures from `cargo xtask web-e2e` (see report.json).
Per-workspace coverage: /work, /processes, /insights, /modules, /finance, /sales, /procurement, /inventory, /asset, /operations, /quality, /maintenance, /manufacturing, /planning, /project, /commerce, /talent, /learning, /benefits, /field-service, /ai, /people, /marketing, /legal, /pos, /platform/holdings, /identity/principal, /identity/role, /platform/support, /platform/incidents, /reference/item/:id.

Roles exercised: Platform admin (all workspaces), Finance viewer (finance + modules), Customer security head (identity).
Locales/themes: en/th, light/dark, desktop 1440 + mobile 390.
