# Organize workspaces by business owner

Place owner UI and server code together under `modules/<owner>/` so a contributor can find the business rule and its presentation from the same directory. Keep app routing in `apps/`, product-specific page assembly in `products/`, and shared runtime, contracts, and UI in `platform/`. This updates the source layout defined in [ADR-052](ADR-052-rust-backend-and-domain-aligned-frontend.md).

Bun and Cargo package names follow the layout. Module IDs exposed through runtime contracts, app routes, permission keys, events, and executable names retain their existing identities. The test catalogue continues to own suite registration.
