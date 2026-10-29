# Native scheduled Legal release evidence

The Platform Administration and PMS Rust components preserve the existing scheduled Legal readiness dimension under [ADR-052](../../adr/ADR-052-rust-backend-and-domain-aligned-frontend.md). They are borrowed owner readers, not release submission/approval commands or general personal authorization. Both owners remain incomplete in the [migration register](../../status/rust-backend-migration.json); their TypeScript runtime remains active.

Platform owns the latest per-Company Legal acceptance, configuration revision and seven current independent gate reviews. Organization supplies its current setup status/version through its public reader; Identity supplies current reviewer assignment, principal, permission and independent-person checks through its public reader. Platform does not read those owners' private tables. IAM samples the database clock for its existing immutable predicates. A revoked latest acceptance never falls back to an older accepted row. Legal scope keeps its own exact software/configuration/setup identity and does not inherit hotel branch-count or currency restrictions.

PMS owns the Legal rollout. In explicit production mode, both `pilot` and `available` require the Platform acceptance predicate; `off` denies it. Existing isolated engineering evidence remains a separate path. Explicit development mode preserves the prior scheduled predicate's development behavior. Mode and artifact version are trusted composition inputs without request deserialization or environment fallback inside the portable domain.

## Fresh source leases

Call `lock_scheduled_legal_sources` together with every other participating owner source lease before the fresh Holding transaction's first data query. Its utility locks include Platform, Organization and Identity sources. The readers take tuple fences and preserve SQLSTATE errors for whole-unit retry. A table lease cannot refresh a previously established snapshot; these APIs do not claim that an arbitrary old transaction becomes fresh. Production worker composition must own this ordering.

`PgLegalReleaseReader` returns separate acceptance and engineering results, an opaque revision and a private transaction-bound witness. Revalidation rejects another transaction even on the same backend. These are release facts only: current personal authority, actual record access, purchased capabilities and preservation policy remain mandatory independent gates. A final short worker transaction obtains fresh evidence rather than transporting an initial transaction's witness across expensive computation.

## Isolated engineering

Engineering configuration requires the exact local environment, gateway and simulation port3104 and a12-character lowercase hexadecimal run. Every read rechecks canonical paths, attestation/release source identity and the actual release manifest hash. The database must be one of that run's three exact browser databases and contain a matching enabled, unexpired Company enrollment. Expiry uses the database clock. Production inputs, another run/database, changed files or a redirected manifest do not acquire this exception. The diagnostic attestation helper reports a file match, not an enrollment or permission grant. Enrollment commands and complete engineering tooling migration remain pending.

## Evidence and limits

The frozen oracle executes the unchanged B0 `acceptedReleasePredicate` generator on pinned Node26.11.1, retaining its exact SQL, input-source hashes and seven criteria. Disposable PostgreSQL cases compare this predicate with the real Rust owner readers for passed/missing-current review, wrong permission, same-requester/assignment mismatch, revocation, changed setup/configuration and latest revoked acceptance. Separate checks cover an unbuilt artifact, rollout-off behavior, explicit development behavior and same-backend cross-transaction witness rejection. A deterministic writer/source-lease case covers an inserted newer revocation before the first fresh snapshot. File tests cover exact engineering configuration, manifest tampering and symlink redirection.

These component receipts do not certify a Company, a public browser journey, the complete owner, production deployment or Docker readiness. No installed grant, rollout or acceptance is changed by the synthetic database tests.
