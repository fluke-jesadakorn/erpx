# Samba AD on the ERPX Mac

This temporary deployment keeps ERPX development and production separate in two Alpine Linux 3.24 ARM64 VMs. Production restores the existing `erpx.test` Samba domain; development provisions an unrelated `erpx-dev.test` domain. QEMU user networking exposes SSH, LDAPS and the finite ERPX runner only on loopback. It is suitable for ERPX and local service integration, not workstation domain join.

Alpine's official repositories currently ship Samba 4.23, which is older than the UCS source at 4.24.2. Do not restore the UCS backup with that package. Build a pinned Samba 4.24.x APK, verify it, and install the same artifact in both environments before provisioning or restoring a domain. This compatibility gate fails closed.

```sh
python3 ops/samba/manage.py prepare prod
python3 ops/samba/manage.py start prod
python3 ops/samba/manage.py status prod
```

Production uses host ports 19322/19636/19444 and development uses 20322/20636/20444. Private disks, keys, seed data, certificates, journals and backups live under ignored `.scratch/samba/` with restrictive permissions.

## Preserve the UCS domain

Run `samba-tool dbcheck` on the isolated UCS clone, reconcile S4 connector rejects, and capture the immutable mapping before backup. Create the backup with `samba-tool domain backup online`; it includes directory secrets and SYSVOL unless `--no-secrets` or `--no-sysvol` is supplied. Transfer it over verified SSH and restore it with `samba-tool domain backup restore --newservername=SAMBA-PROD`. Existing domain controllers must be off before the restored DC starts.

Install `../ucs/gpo_runner.py` on the restored host without `identity_mapping_file` or `udm_url`. In that native mode it inventories and writes Samba accounts, groups, OUs and the finite GPO surface. Configure `write_base_dns`, `allowed_gpo_ids`, TLS files, a dedicated delegated account, connection IDs and a private state directory exactly as described in [the runner runbook](../ucs/README.md).

The ERPX secret profile must contain the verified alias map under `identityAliases`, keyed by lowercase Samba `objectGUID` and valued with the prior UCS `entryUUID`. This keeps ERPX Provider Identity references stable while `windowsIdentity` carries the real GUID/version used for writes. Duplicate aliases fail closed.

## Operational boundary

Keep the old UCS VM and its source disks stopped after cutover. Starting it beside the restored writable domain can create divergent directory histories. Back up the Samba domain and runner state outside this Mac before treating the temporary deployment as authoritative.
