# G8-FIN-013 — Provider-neutral Finance banking adapter boundary

## Delivered behavior

This bounded sidecar adds an injected `BankProviderAdapter` boundary in
`erpx-infrastructure`. Provider implementations return only the fixed typed
DTOs in the boundary:

- `BankProviderAccountRef` accepts only bounded opaque provider/account
  references and exposes no credential, token, card, PAN, CVV, or payment
  instrument field.
- `BankProviderAccountBinding` binds the opaque account to a local account ID,
  holding, legal company, explicit company/branch `HoldingScope`, currency, and
  display label. The existing validated `BankAccount` constructor is used to
  reject scope mismatches.
- `BankProviderCursor`, `BankStatementPageRequest`, and
  `BankProviderRequestBinding` enforce bounded pagination and produce a
  deterministic SHA-256 request identity over the typed request, including
  scope, account, date window, cursor, and page size.
- `ProviderBankTransaction` and `BankStatementFacts` validate fixed statement
  facts, currency/date containment, non-zero amounts, bounded descriptions,
  unique deterministic transaction ordering, and opaque transaction and
  statement references. Raw provider payloads are not represented.
- `BankStatementPage` binds output to the exact request, rejects repeated
  cursors and inconsistent final-page markers, exposes a deterministic content
  digest, and maps only a complete validated page to a domain-ready
  `BankAccount` plus `BankStatementImport` pair. Transaction UUIDs are stable
  across retries. The pair is suitable for the existing
  `SqlxFinanceBankingStore::register_account` and `import_statement` methods;
  this packet does not persist it.
- `BankProviderError` is a closed, redacted error vocabulary. It contains no
  provider message, URL, response body, credential, or token.
- `BankProviderAdapter::fetch_validated_statement_page` is the injected async
  seam that checks adapter/provider identity and validates request/response
  binding. A fake adapter test proves the contract without network access.

Provider-facing DTOs implement `Serialize` for deterministic request/content
hashing but intentionally do not implement `Deserialize`; external SDK values
must pass the explicit validating constructors rather than bypassing the
boundary through unchecked deserialization.

## Files changed

- `crates/infrastructure/src/finance_banking_provider.rs`
- `crates/infrastructure/src/lib.rs` (exports for this packet only)
- `crates/infrastructure/tests/finance_banking_provider.rs`
- `erpx/audits/G8-FIN-013/README.md`

No server, worker, protobuf, domain, migration, Folio, environment, or
runtime files were changed by this packet. No database effects or migrations
were introduced.

## Verification

Run from `erpx/`:

```text
cargo fmt --all
cargo check -p erpx-infrastructure
cargo clippy -p erpx-infrastructure --all-targets -- -D warnings
cargo test -p erpx-infrastructure --test finance_banking_provider
```

Results:

- `cargo fmt --all --check`: PASS after G6-PRV-011 repaired the adjacent scheduler packet; the packet files also pass direct `rustfmt --check`.
- The focused provider contract test: 6 passed, 0 failed in the packet
  integration run.

The focused tests cover page-size and cursor bounds, repeated cursor
rejection, incomplete-page mapping rejection, holding/holding/company scope
binding, opaque-reference credential-marker rejection, deterministic request
and response binding, stable domain transaction IDs, provider-error
redaction, and an injected fake adapter with no network.

The provider module and focused test also pass direct `rustfmt --check`. The
previously observed adjacent scheduler source problem was repaired by
G6-PRV-011 and independently verified. The fresh post-repair workspace wave
passed formatting, strict clippy, active workspace tests, Dioxus web/desktop
checks, Folio lint (four existing warnings), and TypeScript checking. The
provider packet itself still does not claim production provider connectivity.

## Security review

- Credentials and payment instruments cannot be represented by the DTOs or
  the closed provider error type.
- Opaque references and cursors are bounded and restricted to a safe reference
  alphabet; common credential-shaped markers are rejected.
- Provider output cannot choose holding/company scope. Scope comes from the
  caller-owned account binding and is included in the deterministic request
  binding.
- The adapter wrapper rejects a response produced for another request,
  provider, cursor, date window, page size, account, or currency.
- Provider transaction order is deterministic and duplicate/non-monotonic
  references are rejected rather than silently reordered.
- The mapping method refuses a non-final page, so a partial provider page
  cannot be persisted as a complete statement import.
- Provider failures are reduced to stable redacted variants before leaving the
  adapter boundary.

## Unfinished work

This packet intentionally does not implement provider SDKs, credential
vaulting, account activation, bank connectivity, network retries, gRPC or HTTP
transport, worker scheduling, Stripe, GL matching/posting, reconciliation
commands, or persistence. A future provider integration must own credential
retrieval outside this DTO boundary, translate only validated facts, call the
authoritative RBAC/entitlement checks, and pass the mapped values to the
existing SQLx write methods. Multi-page statement accumulation remains a
caller-owned typed workflow that must map only after the final page.

## Integration order

1. Keep `BankProviderAdapter` injected at the application boundary.
2. Resolve active holding/company scope and Finance entitlement through the
   authoritative command path.
3. Fetch pages through `fetch_validated_statement_page` and accumulate only
   typed `BankStatementFacts`.
4. Map the complete final facts with `BankStatementPage::map_to_domain`.
5. Persist the returned account/import through G8-FIN-011's validated SQLx
   methods; the later worker projects the resulting source events through
   G8-FIN-012.
6. Add provider-specific live/integration evidence independently; do not claim
   this packet as production provider integration.

## Rollback

Rollback is additive and isolated: remove only
`finance_banking_provider.rs`, its exports in `crates/infrastructure/src/lib.rs`,
the focused test, and this audit document. There is no database or migration
rollback and no change to existing banking persistence.
