# ERPX contract policy

- Protobuf packages use a versioned namespace.
- Existing field numbers and meanings are never reused.
- Removed fields and enum values remain reserved.
- Commands carry an idempotency key and authoritative tenant scope.
- Every contract change updates the checked-in descriptor and compatibility
  tests.
- Intentional development-only breaking changes require a newly approved
  descriptor baseline and a cutover record.
