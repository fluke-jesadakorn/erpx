# Offline PDF verifier conformance

This is development tooling. Python and pyHanko must not be packaged into the Rust worker or used as a runtime fallback.

`validate_pdf.py` is the B0 validator from commit `a5ed8737a`, with one testing change: the validation clock is injected. `generate.py` uses pyHanko 0.37.0 and cryptography to create real synthetic PDF/CMS signatures, X.509 certificates, CRLs, OCSP responses and RFC3161 timestamp tokens. It asks the independent B0 validator for the expected result. It does not obtain expected results from the Rust implementation. Generated signing keys stay in memory and are discarded; the committed corpus contains synthetic public evidence only.

The corpora contain 124 signed scenarios: 122 exact B0 overall/per-signature state comparisons and two explicit migration security corrections described below. The frozen R2 12-case CRL guard receipt remains unchanged; its historical guard states describe R2, while the current tests require 11 exact B0 outcomes and the separate revocation correction. A new 16-case scope corpus exercises independent positive and negative evidence. Tests compare every original signature field, integrity, certificate chain, certificate dates, revocation, timestamp and overall state. Document timestamps preserve B0's explicit indeterminate document result; they do not receive invented timestamp validity. Scenarios include:

- Trusted, missing, stale, forged and revoked CRL evidence; trusted, stale, future, issuer-mismatched and revoked OCSP evidence.
- OCSP nonce handling, B0's one-second tolerance and its 30-minute fallback when `nextUpdate` is absent.
- Delegated OCSP responder authorization, including CA-issued no-check and missing responder revocation evidence.
- RSA PKCS#1, RSA-PSS, EC P-256, SHA224, weak SHA1 and weak RSA key policy.
- Missing, expired, future, untrusted and forged timestamp evidence.
- Content/CMS tampering, encrypted/truncated documents, unsupported hashes, multiple signatures, DocMDP form changes, allowed DSS maintenance, malformed DSS and attempted later redefinition of signed permissions.
- FieldMDP All/Include/Exclude, including allowed changes to an unlocked field.
- Seed handler/subfilter, digest, signing reason, trusted timestamp, approval/certification type, signer/issuer certificate whitelists, subject-DN and key-usage profiles, including mismatches and alternative profiles. Policies are read from the original signed revision.
- Existing DSS/VRI maintenance and XMP metadata updates, malformed XML, entities, incorrect XMP roots and an attempted drawing update through a DSS alias.
- Complete/delta CRL pairing, advertised versus unadvertised deltas, missing bases, removal of a prior revocation, CA-only scope and inherited same-issuer indirect CRL entry issuer context.
- Named URI, relative and canonical directory distribution points; case-sensitive URI paths; mismatches, multiple points, incomplete reason coverage, complete reason unions, partitioned revocation and consistent delta removals.

## Reproduce

From the repository root, install the existing pinned oracle requirements into an ignored development environment, then regenerate explicitly:

```sh
python3 -m venv .scratch/dev/rust-pdf-oracle
.scratch/dev/rust-pdf-oracle/bin/python -m pip install -r scripts/legal/requirements.lock
.scratch/dev/rust-pdf-oracle/bin/python tests/support/legal/pdf-oracle/generate.py fixtures/legal/pdf-signatures/conformance.v1.json
.scratch/dev/rust-pdf-oracle/bin/python tests/support/legal/pdf-oracle/generate_policy.py fixtures/legal/pdf-signatures/conformance-policy.v1.json
.scratch/dev/rust-pdf-oracle/bin/python tests/support/legal/pdf-oracle/generate_crl_scope.py fixtures/legal/pdf-signatures/conformance-crl-scope.v1.json
cargo test -p erpx-server-legal --features pdf-verifier --test pdf_validation --locked
cargo test -p erpx-server-legal --features pdf-verifier --test pdf_process --locked
```

Regeneration intentionally changes synthetic keys, nonces and document bytes. Review changes to expected results independently; do not overwrite a failing expectation to match Rust. Routine tests read the committed corpus and need no Python, database, network or current-clock access.

The R2 `conformance-crl-guards.v1.json` and forbidden-only seed receipt are retained evidence: do not regenerate them in place. `generate_crl_guards.py` can reproduce the historical experiment into an ignored scratch path.

The operations/conformance CLI is:

```sh
cargo run -p erpx-server-legal --features pdf-verifier --bin erpx-validate-pdf --locked -- input.pdf prepared-trust-directory 2026-10-27T06:00:00Z
```

The trust directory contains only approved `roots/*.pem`, `intermediates/*.pem`, `revocation/*.crl` and `revocation/*.ocsp`. It never loads system/browser trust roots or fetches certificate/document URLs. The CLI is for controlled conformance/operations. Untrusted runtime work must use the bounded adapter described below; the standalone macOS CLI does not enforce a resident-memory ceiling itself.

## Resource and runtime boundary

The owner retains the current 10 MiB document limit. PDF parsing is strict, refuses repaired cross-reference data and limits each decoded object/xref stream to 32 MiB. Trust material is limited to 100 items and 4 MiB per item. The CLI bounds document reads and JSON output (2 MiB).

`BoundedPdfVerifier` implements the worker boundary: 32 MiB admission across bounded metadata/PDF/trust bytes, the existing 10 MiB PDF business limit, one active child, a 90-second child deadline, and 2 MiB combined stdout/stderr. Inputs use owned random 0700 directories and create-new 0600 files. Paths come from the adapter, not the job. The child receives a cleared environment and no shell. Timeout, cancellation, output/memory overflow and process failure kill and reap only the owned child before deleting files. The runtime must await `shutdown_and_drain` before dropping its Tokio executor.

The CLI installs CPU, stack, descriptor, file-size and core limits before reading input. Linux additionally uses 512 MiB DATA and 1 GiB address-space hard limits. This Mac rejects a 512 MiB DATA limit before parsing, so macOS uses an owned-PID resident-memory monitor with a 512 MiB ceiling sampled every 25 ms. It kills and reaps on overflow; allocation bursts and scheduler delays can overshoot that measurement. This is not a macOS hard resident-memory sandbox. PR2 still needs Linux/container memory isolation and platform measurements. Fourteen native supervisor tests exercise actual memory overflow using a disposable Rust helper, along with timeout, cancellation, graceful shutdown, output overflow, private files/environment, approval/scope/checksum admission, strict-proof protocol checks and the real verifier executable.

`certificateChain` preserves the original B0 *soft-fail* pass as a diagnostic. B0 can report this diagnostic as `indeterminate` while its separate required-revocation path proves a `valid` signature. The native addition `requiredCertificateChain` records that strict offline proof explicitly. It becomes `valid` only after the current path and complete revocation evidence succeed. Signature aggregation and the child protocol require this field; a missing or unknown proof cannot produce a valid result. Real-child tests cover both a valid strict path with an indeterminate soft diagnostic and an actually incomplete strict path. Existing TypeScript inspection results are open JSON records and do not narrow or reconstruct these fields; no route is switched to Rust by this clarification.

The application `verify_company_pdf` loads an authoritative Company/document snapshot through `PdfVerificationRepository`, requires approved independently reviewed trust, and passes the injected clock to `PdfVerifierProcess`. `retain_if_current` must atomically recheck authorization, document hash and trust revisions and retain material IDs/versions/kinds/raw checksums. An application result requires a retained inspection ID. The processing-specific `inspect_company_pdf_for_processing` retains tool failure as an explicit indeterminate inspection only after those same current rechecks. It records the failure code, no invented signature or child digest, and the actual attempted material receipts. Admission, approval, checksum and cancellation failures still propagate. Unknown evidence cannot release a document as verified.

`PgPdfVerificationRepository` implements Legal-owned document/trust reads and inspection/access-event retention in a borrowed serializable Holding transaction. It bounds body/trust reads, refuses object-key-only documents without a canonical attachment link, and rechecks current authority, original-source and trust witnesses before retention. Five guarded PostgreSQL tests exercise real signed input, Company trust rows, the actual Rust child, job completion and actual retained evidence. Current source/trust changes prevent retention. Foreign Identity/Commercial/Files policy in these component tests uses explicit synthetic bridges; it is not production authorization.

Production worker composition uses short phases: `prepare_current_legal_pdf` reads authorized input and stable source-data identity, then the caller commits and drops every current witness. `compute_prepared_legal_pdf` runs outside database/Control leases and takes only owned internal input. It has no connection or opaque source argument. `retain_current_legal_pdf` runs under a newly authorized source/Identity/Commercial/Files/trust transaction, rereads actual input facts, compares exact lease/attempt/document/version/hash/source-data/trust receipts, then retains inspection and completion. Old transaction-bound tokens are never reused. The original authority framing is replaced by an explicit data-only marker before compute; the final row binds fresh authority instead. Dated `computedInput` provenance records the original preparation and input identity without changing `validatedAt`; final `created_at` records retention time. Signature states describe the explicit validation time, rather than an invented new verification at the later commit time.

The split regression executes the real Rust verifier while actual trust-retirement or permitted source-destruction writers commit, asserts no idle owner transaction remains during compute, and proves fresh retention rejects those changes. An allowed fresh authority-token change with unchanged input can succeed. A separate service composition suite exercises real current IAM/Control writers and final current-role denial. The older atomic helper remains an unmounted component/conformance path; retaining IAM/People/Org/Control locks during worker compute is not the production topology.

The processing command application requires current authority before Workflow replay, preserves the 20-per-minute Company limit and rechecks the current source/authority before atomic queue/receipt retention. `PgLegalProcessingRepository` borrows the serializable Holding transaction for bounded recovery, 10-minute claims, three-attempt interrupted-work recovery, status and completion. Scans carry an explicit cursor past ineligible older rows; each claim must commit before expensive work. The existing Holding cache-revision trigger serializes concurrent updates to the jobs table. Five unit tests and six guarded PostgreSQL tests cover replay/permission/quota, rollback, stale tokens, recovery scope, progress, `SKIP LOCKED`, source/inspection matching and transaction-bound opaque proofs. A deterministic freshness regression waits on an actual denial-table utility lock, commits a destruction writer, then requires the first fresh source snapshot to deny access. An advisory-lock `SELECT` is not used as proof of snapshot freshness.

`CurrentLegalProcessingSource` is non-deserializable and minted only from the actual locked running row, bound to database/backend/transaction, source hash and lease. Original actor JSON is historical data; it grants no permission. Production composition must produce and strictly parse Identity's complete `QueuedWorkBinding`, including explicit nullable version pins; missing fields are not inferred. `require_current` remains running-only. The explicit `require_current_or_completed` accepts completion only after Legal's actual source-taking completion has minted its private stamp, then checks the exact retained inspection/result and immutable source through the original deadline. Direct completed flags and reuse across transactions are rejected. The fresh final worker phase supplies this completion stamp. Complete owner purchase/read/record/release policy must still be assembled before runtime registration.

Company trust management has command preparation, a separate pure Rust `trust-materials` inspector, a borrowed PostgreSQL store and a concrete command repository/workflow adapter. Four native rule tests and four guarded PostgreSQL tests cover permission before replay, Company scope, independent persons across accounts, current versions, CA/expiry parsing, actual item caps, approve/retire transitions, and atomic rollback with outer receipt/audit effects. Production Identity/Workflow bridges are required interfaces with no permissive default. The `trust-materials` feature does not link the OpenSSL/PDF worker.

The registered database suites are `legal.rust-trust-postgres.integration`, `legal.rust-processing-postgres.integration`, `legal.rust-pdf-verification-postgres.integration` and the service-owned `legal.rust-queued-authority-composition.integration`. They use the shared guarded fixture, full Holding migrations and only newly created leased `erpx_dev_rust_*` databases. Run through the common runner with its authorized test-admin environment; never point these tests at a real Holding database.

## Remaining parity and integration work

This checkpoint is an implemented, tested verifier component, not a completed Legal owner or production acceptance. No TypeScript backend or Python runtime path is removed on the basis of these tests.

- Seed appearances, legal attestations and uncommon flags remain unresolved until their dedicated compatible/noncompliant corpus and implementation are complete.
- Incremental changes outside the implemented exclusive DSS/XMP, signature and field-value rules remain `indeterminate`. Hierarchical widget appearances and uncommon PDF transforms need further corpus coverage. Document timestamps are unsupported by B0's ordinary-signature entrypoint and preserve its indeterminate/no-partial-signature policy.
- CRLs delegated to a separate signer and uncommon name encodings still need more implementation and fixtures. Unsupported potentially applicable scope cannot prove good or be hidden by a separate good response. Unknown critical extensions make that CRL unusable; an independently complete supported CRL can still prove good. Named/relative points, reason unions and consistently good/removal deltas are implemented and independently exercised. Current conflicting supported revocation remains invalid, as described below.
- The current timestamp ASN.1 model and supported digest/algorithm list need broader real-world and adversarial coverage, including fractional time encodings and unusual CMS variants. Unsupported input never proves trust.
- Independent review of X.509 path/policy edge cases, parser resource attacks and signed-attribute protection remains required. The corpus is not an external security audit or a PAdES certification.
- Production current-authority/Files/Commercial/Workflow bridge assembly, OCR/export providers, native worker registration and the remaining Legal workflows still require implementation and integration. The processing state machine and offline verification composition are implemented as components; they do not establish the full current owner parent. No API/runtime activation follows from these tests. Worker PDF dependencies are behind the `pdf-verifier` feature; portable owner ports/reports remain available to the API without those dependencies.

## Operational PDF provider readiness

`BoundedPdfVerifier::probe_readiness` builds a fixed small unsigned page with the vetted `lopdf` writer and runs the actual dedicated child with no Company trust. It requires the bounded child protocol to return `indeterminate`, no signatures and exactly `NO_EMBEDDED_SIGNATURES`. It writes no inspection, job, audit, grant or database row. This is an operational process/parser/protocol probe, not signature-mathematics acceptance, OCR evidence, a Company release decision or approved trust.

The opaque readiness value binds observed binary bytes, protocol, configured deadline/output/memory limits and the fixed probe checksum. Resource hashing streams at most 256MiB; profiles must match before and after a probe, and private paths are not serialized. The probe uses a reduced 5s supervisor budget, respects active capacity, cleanup health and stopping, and finishes owned kill/reap on failure. A reaping error or cleanup beyond 60s makes the adapter unhealthy while its task continues owning the handle, permit and files. Resource observations do not attest every dynamically linked library; release/SBOM and cryptographic conformance remain separate. These APIs and their dependencies remain behind `pdf-verifier`; default/API dependency closure remains crypto-free. Actual queue/bootstrap admission still requires its own current source, IAM, record, Files, Commercial and PMS/release checks.

## Approved security exception to literal B0 parity

pyHanko 0.37.0 `pyhanko/sign/validation/settings.py:124-126` returns from `validate_asserted_key_usage` when the required key-usage set is empty, before checking forbidden usages at lines 129-136. `pyhanko/sign/fields.py:655-662` passes these sets from the PDF seed profile. Consequently, a mandatory `/SVCert` key-usage profile `0XXXXXXXX` accepts a certificate that asserts `digitalSignature` in B0. Profiles that both require and forbid usages reach the later check as expected.

The immutable `fixtures/legal/pdf-signatures/seed-forbidden-only-b0.v1.json` retains the independent original B0 `valid` receipt, the actual signed synthetic PDF/trust evidence and its checksum. Neither generator rewrites it. `seed-forbidden-only-source.v1.json` binds the source files to the verified official PyPI wheel. Rust preserves the explicit prohibition and reports overall `indeterminate`, cryptographic `integrity: valid`, separate `policy: invalid`, and `SEED_VALUE_CONSTRAINTS_FAILURE`; it never claims that the signature mathematics failed. [ADR-053](../../../../docs/adr/ADR-053-explicit-pdf-seed-key-usage-prohibitions.md) accepts this migration security fix. It is not a literal parity pass, and actual workflow integration remains required.

## Retained R2 CRL guard evidence and current semantics

`conformance-crl-guards.v1.json` retains independent B0 receipts and the temporary R2 guard without edits. Current code resolves supported named/reason scopes before deciding applicability, combines reason coverage, and inspects every matching usable current delta. Consistently good effective states can prove good; a supported authenticated revocation cannot be erased by filename order. Identical duplicate CRLs are deduplicated, and provably excluded named/CA-only scope is ignored. Unresolved name semantics remain indeterminate.

| Scenario | B0 overall | Current Rust |
| --- | --- | --- |
| Competing good deltas | valid | valid |
| Revocation in the first competing delta | invalid | invalid |
| Revocation in a later competing delta | valid | invalid |
| Named scope with complete good evidence | valid | valid |
| Matching named scope with revocation | invalid | invalid |
| Reason scope with complete good evidence | valid | valid |
| Reason scope with revocation | invalid | invalid |
| Independent complete-CRL revocation plus unknown scope | invalid | invalid |
| Unknown critical scope plus complete good evidence | valid | valid |
| Provably excluded CA-only scope | valid | valid |
| Identical duplicate delta | valid | valid |
| Reason scope plus good OCSP | valid | valid |

Eleven cases now compare every original B0 signature component exactly. The remaining `competing_delta_revoked_second` retains B0 `valid` and requires Rust `invalid`, `integrity: valid`, `revocation: invalid`: an authenticated applicable current delta lists the signer as revoked. `competing-delta-source.v1.json` binds the original signed PDF and material hashes to R2 and to the independently verified official `pyhanko-certvalidator` 0.32.1 wheel. Its `_find_matching_delta_crl` returns the first issuer/IDP/AKI match at lines 317–344 and never reconciles the later revoked candidate. [ADR-054](../../../../docs/adr/ADR-054-preserve-proved-revocation-across-competing-delta-crls.md) accepts this conservative correction separately from ADR-053. It is not literal B0 parity or a claim that RFC5280 requires this exact competing-material policy. The conformance coverage does not establish whole-owner acceptance.

Underlying implementations: [OpenSSL Rust bindings](https://github.com/rust-openssl/rust-openssl), [lopdf](https://github.com/J-F-Liu/lopdf), [RustCrypto formats](https://github.com/RustCrypto/formats), [roxmltree's bounded XML options](https://docs.rs/roxmltree/latest/roxmltree/struct.ParsingOptions.html), and the development-only [pyHanko](https://github.com/MatthiasValvekens/pyHanko) oracle. CRL scope and delta pairing use [RFC5280](https://www.rfc-editor.org/rfc/rfc5280). No AGPL PDF engine or custom cryptographic primitive is introduced.
