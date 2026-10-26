# Offline PDF verifier conformance

This is development tooling. Python and pyHanko must not be packaged into the Rust worker or used as a runtime fallback.

`validate_pdf.py` is the B0 validator from commit `a5ed8737a`, with one testing change: the validation clock is injected. `generate.py` uses pyHanko 0.37.0 and cryptography to create real synthetic PDF/CMS signatures, X.509 certificates, CRLs, OCSP responses and RFC3161 timestamp tokens. It asks the independent B0 validator for the expected result. It does not obtain expected results from the Rust implementation. Generated signing keys stay in memory and are discarded; the committed corpus contains synthetic public evidence only.

The current corpus contains 50 scenarios. The native test compares the overall result and every signature's field, integrity, certificate chain, certificate dates, revocation, timestamp and overall state. Scenarios include:

- Trusted, missing, stale, forged and revoked CRL evidence; trusted, stale, future, issuer-mismatched and revoked OCSP evidence.
- OCSP nonce handling, B0's one-second tolerance and its 30-minute fallback when `nextUpdate` is absent.
- Delegated OCSP responder authorization, including CA-issued no-check and missing responder revocation evidence.
- RSA PKCS#1, RSA-PSS, EC P-256, SHA224, weak SHA1 and weak RSA key policy.
- Missing, expired, future, untrusted and forged timestamp evidence.
- Content/CMS tampering, encrypted/truncated documents, unsupported hashes, multiple signatures, DocMDP form changes, allowed DSS maintenance, malformed DSS and attempted later redefinition of signed permissions.
- FieldMDP All/Include/Exclude, including allowed changes to an unlocked field.

## Reproduce

From the repository root, install the existing pinned oracle requirements into an ignored development environment, then regenerate explicitly:

```sh
python3 -m venv .scratch/dev/rust-pdf-oracle
.scratch/dev/rust-pdf-oracle/bin/python -m pip install -r scripts/legal/requirements.lock
.scratch/dev/rust-pdf-oracle/bin/python tests/support/legal/pdf-oracle/generate.py fixtures/legal/pdf-signatures/conformance.v1.json
cargo test -p erpx-server-legal --test pdf_validation --locked
```

Regeneration intentionally changes synthetic keys, nonces and document bytes. Review changes to expected results independently; do not overwrite a failing expectation to match Rust. Routine tests read the committed corpus and need no Python, database, network or current-clock access.

The operations/conformance CLI is:

```sh
cargo run -p erpx-server-legal --bin erpx-validate-pdf --locked -- input.pdf prepared-trust-directory 2026-10-27T06:00:00Z
```

The trust directory contains only approved `roots/*.pem`, `intermediates/*.pem`, `revocation/*.crl` and `revocation/*.ocsp`. It never loads system/browser trust roots or fetches certificate/document URLs. Runtime composition remains responsible for independent trust-material review, authorization to the exact immutable document and storing the material IDs/checksums alongside the result.

## Resource and runtime boundary

The owner retains the current 10 MiB document limit. PDF parsing is strict, refuses repaired cross-reference data and limits each decoded object/xref stream to 32 MiB. Trust material is limited to 100 items and 4 MiB per item. The CLI bounds document reads and JSON output (2 MiB).

These parser limits are not an overall process memory or CPU limit. Runtime composition must use a separate Rust verifier process with a 90-second wall-clock deadline, bounded pipes, a 32 MiB maximum transport envelope and an enforced memory limit. A Tokio timeout around a blocking parser does not stop it. Timeout, process failure and resource exhaustion must remain `indeterminate`; they must not release a document as verified. Deployment/container implementation follows PR1 and is not established by this corpus.

## Remaining parity and integration work

This checkpoint is an implemented, tested verifier component, not a completed Legal owner or production acceptance. No TypeScript backend or Python runtime path is removed on the basis of these tests.

- Signature seed-value (`/SV`) policies are explicitly unresolved and downgrade a would-be valid result to `indeterminate`. They need a dedicated compatible/noncompliant certificate/handler/MDP corpus before support is claimed.
- Incremental changes outside the implemented DSS, metadata, signature and field-value rules remain `indeterminate`. This includes complex existing-DSS/VRI replacement, XMP updates, hierarchical widget appearance changes, document timestamps and uncommon PDF transformation methods. Some are accepted or rejected more specifically by B0; full parity is not claimed.
- Full/delta/indirect/partitioned CRL scope and uncommon critical CRL extensions require additional implementation and fixtures. Unsupported evidence cannot prove good.
- The current timestamp ASN.1 model and supported digest/algorithm list need broader real-world and adversarial coverage, including fractional time encodings and unusual CMS variants. Unsupported input never proves trust.
- Independent review of X.509 path/policy edge cases, parser resource attacks and signed-attribute protection remains required. The corpus is not an external security audit or a PAdES certification.
- The bounded Rust worker process, repository ports, native migration/worker wiring and actual Legal workflows remain separate integration work.

Underlying implementations: [OpenSSL Rust bindings](https://github.com/rust-openssl/rust-openssl), [lopdf](https://github.com/J-F-Liu/lopdf), [RustCrypto formats](https://github.com/RustCrypto/formats), and the development-only [pyHanko](https://github.com/MatthiasValvekens/pyHanko) oracle. No AGPL PDF engine or custom cryptographic primitive is introduced.
