# G6-PRV-006 — Production worker holding-routing composition

The worker entry point now composes the active multi-holding projection
scheduler from the control-plane registry and the server-only holding database
router. Selecting `holding_projection_scheduler` requires both routing
environment variables, one validated `{database}` template token, bounded
projection page size, and typed scheduler authorization. The default handler
registry remains unchanged and does not claim holding projection jobs.

Evidence: workspace clippy/tests and the Operations projector audit
`G7-OPS-006`. This is an integration slice only; physical provisioning,
backup/restore adapters, replacement-read rebuild activation, and operations
UI remain open.
