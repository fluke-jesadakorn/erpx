# Identity and access

**Purpose:** Owns local and provider Principals, scoped assignments, immutable role revisions, delegation, directory evidence, migration reviews and emergency access. Person employment/history remains owned by People.

**Current implementation:** Next.js/PostgreSQL/Better Auth. [Implementation and operations](../../../contracts/next/identity-access.md), [ADR-025](../../../adr/ADR-025-multiple-identity-providers-and-scoped-replacement.md), [acceptance](../../../testing/identity-access.md).

**Surfaces:** Customer and Platform Identity & access; Overview, Users, Roles & permissions, Groups & mappings, Access matrix, Directory connections, Reviews & audit; global and workspace My access.

**Invariants:**

- One selected Assignment supplies role permissions per work context; identity participation and mandatory business checks remain enforced.
- Explicit grants expire and remain bounded by parent scope, permission ceiling and validity. Revocation and stale evidence deny execution.
- Multiple optional directories coexist with local accounts. Email and username never link identities automatically.
- Migration applies only a fixed reviewed batch in its Company. Replacement grants are explicit; history and unrelated Company access survive.
- A different authorized Person with fresh MFA reviews sensitive changes, including linked-identity independence.
- Platform and customer authority remain separate; directory or IAM administration does not imply business-data access.
- Matrix explanations use the actual catalog/evaluator. Conditional inspection cannot authorize execution; preview modules are Unavailable.

**Verification:** `bun run typecheck`, `bun run lint`, `bun run test:integration`, IAM/auth/workflow Playwright suites, isolated `bun run deploy:build`. Live provider verification requires approved external profiles and reachable environments.
