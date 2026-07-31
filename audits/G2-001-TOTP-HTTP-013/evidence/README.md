# G2-001-TOTP-HTTP-013 evidence

## Delivered

- Added authenticated `POST /auth/totp/enrollment/start` and
  `POST /auth/totp/enrollment/verify` routes.
- Added recovery-code-authenticated `POST /auth/recovery-sessions/initiate`
  and one-time-bearer `POST /auth/recovery-sessions/consume` routes.
- TOTP encryption uses only the server-composed `TotpKeyResolver`; the HTTP
  request cannot select a key reference. Missing key configuration fails closed
  with `503 totp_unavailable`.
- Enrollment and recovery lifetimes are fixed at 15 minutes. Durable SQLx rate
  limits are applied before each sensitive operation; recovery bearer limiting
  uses a one-way digest as its identifier.
- HTTP DTOs expose only the one-time enrollment secret/URI or one-time recovery
  bearer metadata, plus plaintext recovery codes only in the authenticated
  one-time issuance response. Ciphertext, key references, recovery hashes,
  passwords, and internal credential fields are not serialized.

## Verification

The focused server checks cover route matching, lifetime bounds, and response
redaction. The exact commands and results are recorded in the execution ledger
after the integration wave.

## Remaining concerns

- The durable store and identity migration are upstream dependencies of this
  HTTP packet and are not changed here. Disposable PostgreSQL end-to-end route
  tests remain to be added when the server test harness can construct an
  authenticated session without broadening this packet's allowed paths.
- The recovery exchange intentionally authenticates with the one-time recovery
  bearer because an account in `recovery_required` state cannot authorize a
  normal access token. An optional access-token header, when supplied, is still
  validated through `AuthContext`.
