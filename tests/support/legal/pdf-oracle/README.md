# Offline PDF verifier conformance

This is development tooling. Python and pyHanko must not be packaged into the Rust worker or used as a runtime fallback.

`validate_pdf.py` is the B0 validator from commit `a5ed8737a`, with one testing change: the validation clock is injected. `generate.py` uses pyHanko 0.37.0 and cryptography to create real synthetic PDF/CMS signatures, X.509 certificates, CRLs, OCSP responses and RFC3161 timestamp tokens. It asks the independent B0 validator for the expected result. It does not obtain expected results from the Rust implementation. Generated signing keys stay in memory and are discarded; the committed corpus contains synthetic public evidence only.

The main corpora contain 96 scenarios: 95 exact B0 overall/per-signature state comparisons and one explicitly approved migration security fix described below. A separate 12-case CRL guard corpus records unresolved behavior; it is not counted as completed parity. The native tests compare every signature's field, integrity, certificate chain, certificate dates, revocation, timestamp and overall state. Document timestamps preserve B0's explicit indeterminate document result; they do not receive invented timestamp validity. Scenarios include:

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

## Reproduce

From the repository root, install the existing pinned oracle requirements into an ignored development environment, then regenerate explicitly:

```sh
python3 -m venv .scratch/dev/rust-pdf-oracle
.scratch/dev/rust-pdf-oracle/bin/python -m pip install -r scripts/legal/requirements.lock
.scratch/dev/rust-pdf-oracle/bin/python tests/support/legal/pdf-oracle/generate.py fixtures/legal/pdf-signatures/conformance.v1.json
.scratch/dev/rust-pdf-oracle/bin/python tests/support/legal/pdf-oracle/generate_policy.py fixtures/legal/pdf-signatures/conformance-policy.v1.json
.scratch/dev/rust-pdf-oracle/bin/python tests/support/legal/pdf-oracle/generate_crl_guards.py fixtures/legal/pdf-signatures/conformance-crl-guards.v1.json
cargo test -p erpx-server-legal --features pdf-verifier --test pdf_validation --locked
cargo test -p erpx-server-legal --features pdf-verifier --test pdf_process --locked
```

Regeneration intentionally changes synthetic keys, nonces and document bytes. Review changes to expected results independently; do not overwrite a failing expectation to match Rust. Routine tests read the committed corpus and need no Python, database, network or current-clock access.

The operations/conformance CLI is:

```sh
cargo run -p erpx-server-legal --features pdf-verifier --bin erpx-validate-pdf --locked -- input.pdf prepared-trust-directory 2026-10-27T06:00:00Z
```

The trust directory contains only approved `roots/*.pem`, `intermediates/*.pem`, `revocation/*.crl` and `revocation/*.ocsp`. It never loads system/browser trust roots or fetches certificate/document URLs. The CLI is for controlled conformance/operations. Untrusted runtime work must use the bounded adapter described below; the standalone macOS CLI does not enforce a resident-memory ceiling itself.

## Resource and runtime boundary

The owner retains the current 10 MiB document limit. PDF parsing is strict, refuses repaired cross-reference data and limits each decoded object/xref stream to 32 MiB. Trust material is limited to 100 items and 4 MiB per item. The CLI bounds document reads and JSON output (2 MiB).

`BoundedPdfVerifier` implements the worker boundary: 32 MiB admission across bounded metadata/PDF/trust bytes, the existing 10 MiB PDF business limit, one active child, a 90-second child deadline, and 2 MiB combined stdout/stderr. Inputs use owned random 0700 directories and create-new 0600 files. Paths come from the adapter, not the job. The child receives a cleared environment and no shell. Timeout, cancellation, output/memory overflow and process failure kill and reap only the owned child before deleting files. The runtime must await `shutdown_and_drain` before dropping its Tokio executor.

The CLI installs CPU, stack, descriptor, file-size and core limits before reading input. Linux additionally uses 512 MiB DATA and 1 GiB address-space hard limits. This Mac rejects a 512 MiB DATA limit before parsing, so macOS uses an owned-PID resident-memory monitor with a 512 MiB ceiling sampled every 25 ms. It kills and reaps on overflow; allocation bursts and scheduler delays can overshoot that measurement. This is not a macOS hard resident-memory sandbox. PR2 still needs Linux/container memory isolation and platform measurements. Native tests exercise actual memory overflow using a disposable Rust helper, along with timeout, cancellation, graceful shutdown, output overflow, private files/environment, approval/scope/checksum admission and the real verifier executable.

The application `verify_company_pdf` loads an authoritative Company/document snapshot through `PdfVerificationRepository`, requires approved independently reviewed trust, and passes the injected clock to `PdfVerifierProcess`. `retain_if_current` must atomically recheck authorization, document hash and trust revisions and retain material IDs/versions/kinds/raw checksums. An application result requires a retained inspection ID. Infrastructure failure remains an explicit failure that the processing workflow must retain/retry as indeterminate; it cannot release a document as verified.

`PgPdfVerificationRepository` implements Legal-owned document/trust reads and inspection/access-event retention in the caller's serializable Holding transaction. It bounds body/trust reads, retains Company and preservation locks, refuses object-key-only documents without a canonical attachment link, and rechecks current authority, original-source and trust witnesses before retention. Three guarded PostgreSQL tests exercise a real signed PDF, real Company trust rows, the actual Rust child and actual retained evidence; they also prevent retention after authority/original/trust changes. Identity, Commercial and Files behavior in these tests is supplied by explicit synthetic bridges, not established as production authorization.

Company trust management has command preparation, a separate pure Rust `trust-materials` inspector, a borrowed PostgreSQL store and a concrete command repository/workflow adapter. Four native rule tests and four guarded PostgreSQL tests cover permission before replay, Company scope, independent persons across accounts, current versions, CA/expiry parsing, actual item caps, approve/retire transitions, and atomic rollback with outer receipt/audit effects. Production Identity/Workflow bridges are required interfaces with no permissive default. The `trust-materials` feature does not link the OpenSSL/PDF worker.

The registered database suites are `legal.rust-trust-postgres.integration` and `legal.rust-pdf-verification-postgres.integration`. They use the shared guarded fixture, full Holding migrations and only newly created leased `erpx_dev_rust_*` databases. Run through the common runner with its authorized test-admin environment; never point these tests at a real Holding database.

## Remaining parity and integration work

This checkpoint is an implemented, tested verifier component, not a completed Legal owner or production acceptance. No TypeScript backend or Python runtime path is removed on the basis of these tests.

- Seed appearances, legal attestations and uncommon flags remain unresolved until their dedicated compatible/noncompliant corpus and implementation are complete.
- Incremental changes outside the implemented exclusive DSS/XMP, signature and field-value rules remain `indeterminate`. Hierarchical widget appearances and uncommon PDF transforms need further corpus coverage. Document timestamps are unsupported by B0's ordinary-signature entrypoint and preserve its indeterminate/no-partial-signature policy.
- CRLs delegated to a separate signer, named or reason-partitioned distribution points, multiple competing deltas and uncommon critical extensions need more implementation and fixtures. Unsupported evidence cannot prove good.
- The current timestamp ASN.1 model and supported digest/algorithm list need broader real-world and adversarial coverage, including fractional time encodings and unusual CMS variants. Unsupported input never proves trust.
- Independent review of X.509 path/policy edge cases, parser resource attacks and signed-attribute protection remains required. The corpus is not an external security audit or a PAdES certification.
- Production current-authority/Files/Commercial/Workflow bridges, processing-job claiming/completion, OCR/export providers, native worker registration and the remaining Legal workflows still require implementation and integration. No API/runtime activation follows from these tests. Worker PDF dependencies are behind the `pdf-verifier` feature; portable owner ports/reports remain available to the API without those dependencies.

## Approved security exception to literal B0 parity

pyHanko 0.37.0 `pyhanko/sign/validation/settings.py:124-126` returns from `validate_asserted_key_usage` when the required key-usage set is empty, before checking forbidden usages at lines 129-136. `pyhanko/sign/fields.py:655-662` passes these sets from the PDF seed profile. Consequently, a mandatory `/SVCert` key-usage profile `0XXXXXXXX` accepts a certificate that asserts `digitalSignature` in B0. Profiles that both require and forbid usages reach the later check as expected.

The immutable `fixtures/legal/pdf-signatures/seed-forbidden-only-b0.v1.json` retains the independent original B0 `valid` receipt, the actual signed synthetic PDF/trust evidence and its checksum. Neither generator rewrites it. `seed-forbidden-only-source.v1.json` binds the source files to the verified official PyPI wheel. Rust preserves the explicit prohibition and reports overall `indeterminate`, cryptographic `integrity: valid`, separate `policy: invalid`, and `SEED_VALUE_CONSTRAINTS_FAILURE`; it never claims that the signature mathematics failed. [ADR-053](../../../../docs/adr/ADR-053-explicit-pdf-seed-key-usage-prohibitions.md) accepts this migration security fix. It is not a literal parity pass, and actual workflow integration remains required.

## Unresolved CRL guard corpus

`conformance-crl-guards.v1.json` retains independent B0 receipts and separately records the temporary safe Rust guard. Nonidentical competing current deltas are not reconciled by choosing the highest number. A fresh authenticated CRL whose potentially applicable named/reason scope is unsupported prevents a good complete CRL or OCSP response from producing `valid`. Independently proved revocation still produces `invalid`. Identical duplicate CRLs are deduplicated, and a CA-only CRL is ignored when it provably excludes the end-entity certificate.

| Scenario | B0 overall | Rust guard |
| --- | --- | --- |
| Competing good deltas | valid | indeterminate |
| Revocation in the first competing delta | invalid | invalid |
| Revocation in a later competing delta | valid | invalid |
| Named scope with complete good evidence | valid | indeterminate |
| Matching named scope with revocation | invalid | indeterminate |
| Reason scope with complete good evidence | valid | indeterminate |
| Reason scope with revocation | invalid | indeterminate |
| Independent complete-CRL revocation plus unknown scope | invalid | invalid |
| Unknown critical scope plus complete good evidence | valid | indeterminate |
| Provably excluded CA-only scope | valid | valid |
| Identical duplicate delta | valid | valid |
| Unknown scope plus good OCSP | valid | indeterminate |

The two unambiguous valid cases also compare all B0 signature components exactly. The other ten remain explicitly unresolved, including cases whose overall state agrees but timestamp/revocation components differ. Signature mathematics remains `integrity: valid`; uncertain scope is not mislabeled as a forged signature. These cases do not extend ADR-053 and do not establish acceptance. Full supported scope and delta semantics, and a decision on contradictory B0 evidence ordering, remain required after this checkpoint.

Underlying implementations: [OpenSSL Rust bindings](https://github.com/rust-openssl/rust-openssl), [lopdf](https://github.com/J-F-Liu/lopdf), [RustCrypto formats](https://github.com/RustCrypto/formats), [roxmltree's bounded XML options](https://docs.rs/roxmltree/latest/roxmltree/struct.ParsingOptions.html), and the development-only [pyHanko](https://github.com/MatthiasValvekens/pyHanko) oracle. CRL scope and delta pairing use [RFC5280](https://www.rfc-editor.org/rfc/rfc5280). No AGPL PDF engine or custom cryptographic primitive is introduced.
