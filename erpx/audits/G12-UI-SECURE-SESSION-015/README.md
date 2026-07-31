# G2/G12-UI-SECURE-SESSION-015 — target adapter seam

## Delivered behavior

- Added `erpx-ui`'s compile-time `session_target` boundary for WebAssembly web, desktop
  (macOS/Windows/Linux), Android, iOS, and an explicit unknown-target branch.
- Added `TargetSecureSessionAdapter`, which implements the existing `SecureSessionStore` contract
  and returns `SecureSessionError::Unsupported` for load, save, and clear.
- Added `TargetSessionDescriptor` and non-secret labels so a client can disclose whether protected
  persistence is configured without inspecting or serializing credentials.
- Wired the ERPX console authentication panel to disclose that the current target is memory-only
  and has no secure-storage implementation. No localStorage, cookie, file, or plaintext fallback
  was added.
- Existing route redaction remains the only return-path/deep-link input to session state; the
  target adapter accepts no route or token-bearing URI data.

## Files changed

- `erpx/crates/ui/src/session_target.rs`
- `erpx/crates/ui/src/lib.rs`
- `erpx/apps/console/src/main.rs`
- `erpx/audits/G12-UI-SECURE-SESSION-015/README.md`

## Tests and exact results

- `cargo fmt --all --check` — passed.
- `cargo test -p erpx-ui` — passed: 22 tests, including target selection, deep-link redaction,
  and fail-closed load/save/clear tests.
- `cargo test -p erpx-console --no-default-features --features web` — passed: 9 tests.
- `cargo check -p erpx-console --no-default-features --features desktop` — passed.
- `cargo clippy -p erpx-ui -p erpx-console --all-targets --all-features -- -D warnings` — passed.

The web and desktop checks compile the same adapter boundary on the approved client build
features. Android/iOS native APIs are not claimed or implemented by this packet.

## Unfinished platform-specific implementation

- Android Keystore and iOS Keychain adapters remain to be implemented in target applications.
- Desktop protected credential-store adapters remain to be implemented for macOS, Windows, and
  Linux.
- Browser session restoration remains intentionally disabled until an approved protected browser
  design is selected; browser localStorage/sessionStorage is not an acceptable substitute.
- Console login/refresh currently keeps credentials only in the controller's process memory.

## Security concerns

- The current adapter reports `Unsupported`; callers must treat that as a hard stop for persistence
  and restoration rather than attempting another store.
- `StoredSession` still contains bearer material in memory solely for authenticated transport and
  is not serializable; the adapter boundary does not expose debug or display representations.
- A future native adapter must bind records to the application/device context, clear on revocation,
  and avoid returning secrets through logs, deep links, UI metadata, or crash reports.

## Integration order

1. Integrate this UI-only seam and keep console sessions memory-only.
2. Add one target-specific OS-protected adapter at a time behind the existing trait.
3. Independently verify secure save/load/clear, revocation clearing, and deep-link redaction per
   target before enabling restoration.
4. Complete the broader G12 native target, offline, notification, and accessibility work.

## Rollback

Revert the three source-file changes in `erpx/crates/ui`/`erpx/apps/console` and remove this audit
directory. No database, migration, protobuf, Folio, or runtime state is changed.
