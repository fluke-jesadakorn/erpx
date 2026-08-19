# Security boundary evidence

Confirmed current gaps, not accepted behavior:

- ERPX prototype accepts caller-supplied `x-erpx-actor` metadata and has no identity/session verification.
- Production server does not yet enforce authoritative holding scope, RBAC, entitlements, support capabilities, SoD, or represented-user authority.
- Support approval currently relies on prototype HMAC metadata and a caller-supplied approver string; authenticated Holding Owner/Customer Security Head enforcement is pending G4.
- Support capability hashing/device/session binding/module/branch checks and immediate revocation cascade are pending G4.
- Live PostgreSQL RLS is enabled only for `chat.messages` and `chat.sessions`; platform/holding isolation is pending G1/G6.
- Secret redaction, signed object access, Stripe raw-body signature enforcement, and AI permission-aware tool tests are pending their context gates.
