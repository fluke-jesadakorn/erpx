# Product app workspaces and development verification

Accepted for implementation, 2026-10-22. ERPX separates the fifteen products and seven shared hosts into independent Next.js applications in a Bun workspace. A development gateway retains the existing origins and forwards to independently started app processes. Shared owner implementations retain native PostgreSQL transactions; this supersedes ADR-043's single web-runtime topology, without changing Product, Commercial Package, Module or host-bound identity semantics.

Workspace contracts, owner packages and app composition roots replace application-to-application imports. Development verification selects affected dependencies, uses disposable database clones from content-addressed templates and records development-only observations. Full acceptance is exclusively an explicitly invoked pre-production operation: development, PRs, merges and scheduled jobs must not invoke it. Historical receipts remain bound to their original inputs.

The trade-off is additional app/gateway lifecycle management in exchange for smaller compilation and verification scopes. The existing Console compatibility surface remains separately available during migration; a product app cannot delegate business handling to it. Installed production, staging, Commercial activation and Console retirement are separate operations.
