# HR/IT separation and Technology verification

> Recorded report — findings, counts, gaps and hostname observations below belong to the original run. Current workflows: [owner catalogue](../contracts/next/catalogue.md); latest recorded engineering/provider/release boundaries: [verification inventory](../status/capability-delivery-matrix.md). This report is retained evidence, not an active plan.

2026-09-18. Scope and APIs: [implementation contract](../contracts/next/hr-it-technology.md).

## Automated checks

- Full integration run: **71 files passed, 674 tests passed, 2 files / 3 tests skipped**. The skipped tests require separately opted-in external services. Log: ignored `.scratch/dev/hr-it-integration-final.log`.
- Following provider hardening and the final People/Technology changes: **95 tests passed in 4 files** (`directory-operations`, `people-lifecycle`, `home-streaming`, `workplace-navigation`). Includes the additional existing-GPO settings/impact test. Log: `.scratch/dev/hr-it-focused-final.log`.
- Python runner unit tests: **4 passed**. They cover finite settings, explicit computer targeting, preservation of unsupported preference nodes, GPMC order/flags and journal replay/uncertain-write handling.
- Typecheck passed. Lint has no errors; three existing unused-variable warnings remain in settings, service-navigation and PMS workspace.
- Isolated standalone release build passed at `.runtime/releases/2026-09-18T08-16-21-552Z`. The build used its own Next output, preserving development availability.

Lifecycle coverage includes HR-first placement without a package/account, IT-first shared packages, bulk application, pinned access after a real HR position change, supplemental source preservation/withdrawal, expiry, independent role reviews, suspension and HR return without automatic restoration. Directory coverage includes direct IT apply, reviewed legacy work, scope denial, group/OU cycles and partial-step retry with the same operation IDs.

## Real UCS lab

The existing isolated QEMU clone was restarted after SSH timed out. UCS 5.2-6 / Samba 4.24.2 became reachable again. Source disks were preserved. A dedicated delegated Samba service identity writes only owned test GPOs and approved lab OU links. TLS and SSH host-key verification remain enabled. Profiles and credentials remain in ignored/private files.

- UDM opt-in integration passed: disabled account create/replay, update, stale version, membership, enable, move/replay, disable, OU create/update, group moves, nested groups, populated-container rejection and empty-OU movement with unchanged IDs. The final test explicitly moves children out and returns them after moving the OU. Log: `.scratch/dev/hr-it-udm-live.log`.
- GPO HTTPS integration passed through the application's real provider adapter: create/replay, settings update, stale version, filtering, link flags/order, inheritance and read-back. Log: `.scratch/dev/hr-it-gpo-https.log`.
- Extended final Samba verification **passed** (`.scratch/dev/hr-it-samba-verifier.log`): both policy scopes, USB/lock/wallpaper/Drive/Printer output, unsupported Registry.pol/XML/GPT preservation, preservation of existing links/filtering during settings patches and an injected connection loss after a real commit. Retrying that uncertain operation was rejected; a fresh preview safely unlinked its fixtures and restored inheritance.

Verification fixtures remain in the isolated lab for inspection; final successful GPO verification unlinks its own new fixtures and restores the prior inheritance flag. It does not remove unrelated policies. No real employee role grants were submitted through the public browser.

## Browser and availability

The existing authenticated session verified the public Technology console in dark mode: OU tree/table/properties, real inventory refresh, account-to-group drag preview, affected objects/Company warning, read-only Apply enforcement, IT position package editor and employee selection. At **390 × 844**, employee details/provisioning forms and Directory toolbar/button alternatives were inspected; the temporary viewport was reset.

People organization details showed HR fields without the package editor. HR Explorer showed people and placement targets without the duplicated Directory/access editor. Technology Resources retains the account/group/policy editor.

After the final worker/dev restart and completed lab verification, the public console refreshed **228 Directory objects**, displayed the real test GPO's USB deny, 1200-second lock, wallpaper and Drive/Printer values, and successfully previewed its settings patch with existing targeting preserved. Apply remained disabled for the read-only connector. The console was left open with the normal desktop viewport.

Development and its durable worker run detached with pinned Node 24.20.0 and Bun 1.2.17, logs/PIDs under `.scratch/dev/`. The public Tunnel remains on development port **3100**. The installed production service/configuration on **3101** was not changed. Local and public `/api/health` returned **200** after the detached launcher returned.

## Acceptance boundaries

- Populated UCS OU movement needs the explicit child-out / empty-OU-move / child-back sequence because UDM recreates populated containers. ERPX refuses a direct populated-container move before writing.
- The public lab connector remains read-only until explicitly activated and configured; real write tests use the separately approved isolated test profile.
- After UDM create/move and S4 replication, reconcile trusted UDM/Samba mapping before GPO work. An unmapped or changed identity fails closed.
- **GPO server write/read-back is verified separately from endpoint enforcement.** Windows client `gpupdate`, included/excluded endpoint behavior, actual printer/share reachability, Windows/RSAT acceptance and resource filesystem ACL/share creation remain outside this iteration.
