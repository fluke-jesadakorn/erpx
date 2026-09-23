# Encrypted backup transfer and isolated restore rehearsal — 2026-09-23

Scope: isolated `erpx_test_*` PostgreSQL database on loopback port 5432, fictional private file and application secret, independent fixture recovery home, and a separate fixture export directory on the same Mac. No installed production service or real company database was touched.

`bun run test -- tests/backup-restore-operations.test.ts` passed (one test). `bun run typecheck` and targeted ESLint passed. The test created an `age` identity and public recipient, streamed a completed operator snapshot to an encrypted archive, verified the ciphertext SHA-256 sidecar and absence of a known plaintext marker, rejected a modified archive and an incorrect identity, imported the archive into an independent recovery home, and restored the database row, private file, app secret, and release identity. It also verified that a configured backup automatically emits an encrypted archive.

This establishes a working encrypted transfer and restore path in a local fixture. It does not verify a real off-machine mount, recovery-host isolation, durability after Mac loss, or measured one-hour RPO/four-hour RTO. The installed schedule is daily and must be replaced by a reviewed and tested online backup/WAL design before the production gate.

The read-only `recovery-status` operator check now reports missing offsite configuration, verifies an encrypted archive sidecar and ciphertext checksum, identifies a same-device fixture destination, and leaves the RPO/RTO gates unproven. The test exercises those distinctions without reading production configuration.
