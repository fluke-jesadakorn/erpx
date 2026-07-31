# G6-PRV-033 — Activation-time organization root initialization

## Delivered

- Tenant resource provisioning now requires the registered owner seed before
  physical initialization continues.
- After tenant write/read migrations, provisioning creates a deterministic
  tenant-local `organization.holding` snapshot and
  `organization.holding.initialized` event in the write database.
- The read database receives the corresponding Operations read row and
  projection checkpoint under the same holding scope.
- Both transactions set `erpx.holding_ref`; the existing forced-RLS policies
  remain the isolation boundary.
- Initialization is idempotent and does not become a long-term dual-write
  path. The write event remains the source fact; the read row is a bootstrap
  projection that can be rebuilt by the worker.
- The holding UUID is the same deterministic scoped identity used by the
  control-plane holding-owner assignment.

## Evidence

- `cargo check -p erpx-infrastructure` — PASS.
- `cargo check -p erpx-server` — PASS.
- `cargo clippy -p erpx-infrastructure --test provisioning -- -D warnings` —
  PASS.
- Disposable four-database proof
  `tenant_organization_bootstrap_is_write_authoritative_and_read_projected` —
  PASS; control write/read plus tenant write/read databases were migrated,
  bootstrap replay was stable, write counts were `(snapshot=1,event=1)`, read
  counts were `(row=1,checkpoint=1)`, and temporary databases were removed.

## Remaining scope

This seeds only the holding root because provisioning currently has no legal
company or branch input. Company, headquarters, branch, department, team,
delegated administration, and their complete command/read APIs remain part of
the G7 organization work. The broader G6/G7/G14 gates remain open.
