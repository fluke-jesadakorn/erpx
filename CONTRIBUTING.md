# Contributing to ERPX

ERPX is migrating its business backend to a Rust modular monolith while keeping Next.js for presentation. Follow [ADR-052](docs/adr/ADR-052-rust-backend-and-domain-aligned-frontend.md) and the [current delivery plan](docs/production-plan.md). A foundation, passing smoke test or historical receipt does not establish that a business owner has migrated.

## Start here

Read the [documentation index](docs/TOC.md), [business glossary](CONTEXT.md), [repository instructions](AGENTS.md), [governance](GOVERNANCE.md) and [licensing policy](LICENSING.md). Maintained engineering and community documentation is in English. Preserve exact business names, source quotations and immutable historical evidence where translation would change their meaning.

Use Issues for reproducible bugs and focused feature proposals. Use a proposal issue before changing an owner boundary, public API, schema, authorization rule, licensing policy or deployment topology. Discuss the decision and add an ADR when the tradeoff meets the repository's ADR criteria. Report vulnerabilities through [SECURITY.md](SECURITY.md), with no exploit or credential in a public issue.

## Local development

Use the versions pinned by the repository: Bun 1.4.3, Node.js 26.11.1, PostgreSQL 18.6 and the Rust toolchain declared by `rust-toolchain.toml` when present. Install JavaScript dependencies with `bun install --frozen-lockfile`. Follow the [README](README.md) and [local seed guide](docs/local-seed.md) for the current development path.

Create private local configuration from `.env.example`, replace secret placeholders with generated values, and use separate disposable development databases. Examples are not deployment credentials. Keep provider adapters disabled or use their documented synthetic test adapters when no provider account is available. Do not use an operator's saved credentials, production data, private manifests or live provider account to make a public contribution pass.

For the qualified Rust Service Count, use the scoped [Dev Container quickstart](docs/contracts/next/service-count-devcontainer.md). It declares tested host, exact HTTPS origins, synthetic accounts, runtime ownership and remaining transitional dependencies. Other applications are not implicitly qualified. Existing operator-specific instructions describe that installation and are not prerequisites for contributors.

Ports `3000`, `3001` and `8080` belong to other projects. The development gateway uses `3100`; app processes use `3200`–`3299`. Inspect existing listeners before starting anything and reuse a healthy authorized process. Staging and production changes require their own explicit scope.

## Make a focused change

- Keep business rules and committed calculations in the owning Rust backend. Frontend domains own interaction state, not a duplicate business authority.
- Preserve the `domain`, `application`, `adapters` and explicit `public` boundaries. Depend on an owner's public facade and ports rather than its implementation.
- Put executable tests in their registered `tests/<lane>/<suite-owner>/` location. Keep fixtures fictional and helpers outside executable suite folders.
- Update the current owner contract when behavior changes. Keep [docs/production-plan.md](docs/production-plan.md) as the sole active delivery plan and link new documentation from [docs/TOC.md](docs/TOC.md).
- Do not rewrite dated evidence, checksums or historical acceptance statements to describe a new implementation.
- Disclose copied code, generated material and relevant AI assistance. Contributors remain responsible for understanding, testing and having the right to submit their changes.

Run checks that cover the behavior and owner you changed. Start with `bun run test:dev --app=<app-id>` or the registered suite, and run `bun run structure:check`, `bun run test:catalogue`, `bun run architecture:check` and `bun run docs:check` where applicable. Rust changes also use the repository's locked Cargo format, lint, test and architecture checks. Do not run production acceptance as a daily development check.

For a clean CI candidate, use `bun run test:ci --base=<sha> --head=<sha>` from the exact checked-out commit. Report skipped or unavailable provider/browser checks honestly. UI changes require actual browser inspection through normal authentication, using fictional test accounts.

## Pull requests and review

Use a `codex/` branch for agent work unless the maintainer requests another name. Give the PR a concrete problem, resulting behavior and relevant validation. Link the issue or ADR and identify remaining migration work. Keep commits at reviewable milestones; do not combine generated evidence with claims that it accepts an unrelated release.

Fork PR verification must work with a read-only token and disposable fixtures. It must not require deployment secrets, a private hostname or a maintainer's local environment. Ordinary PRs do not deploy, publish images, rewrite history or run production acceptance. Do not use `pull_request_target` to execute untrusted contribution code.

Follow the configured review, required-check and merge-queue rules. Do not bypass protections or replace failing checks with receipts from a different SHA. The maintainer decides merge timing; see [GOVERNANCE.md](GOVERNANCE.md).

## Sign off your contributions

Use `git commit -s` to add `Signed-off-by: Your Name <your public contribution email>`. This records your certification under the [Developer Certificate of Origin 1.1](https://developercertificate.org/) that you have the right to submit the contribution under its license. Use your own identity; do not fabricate another person's sign-off. Choose an appropriate public email because Git history retains it.

There is no project-specific copyright assignment or CLA. A sign-off on a new contribution does not retroactively clear historical code. Follow [LICENSING.md](LICENSING.md) for third-party and historical material.
