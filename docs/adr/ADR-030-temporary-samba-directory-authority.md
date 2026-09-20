# Temporary Samba directory authority on Apple Silicon

ERPX temporarily runs two isolated Alpine Linux ARM64 Samba AD virtual machines on the development Mac. Production restores the existing `erpx.test` authority and domain SID from a supported Samba backup; development is a separate `erpx-dev.test` authority and can never write production. Both use QEMU HVF, loopback-only host forwarding, verified TLS, pinned SSH host keys, finite runner operations, and the same pinned Samba 4.24.x APK. The UCS VM remains stopped recovery evidence after cutover and must not run concurrently with the restored writable domain.

Provider Identity remains ERPX's durable external reference. A verified one-to-one alias from the restored Samba `objectGUID` to the prior UCS `entryUUID` preserves existing person, account, group and package links; the real Windows identity remains write evidence. Alias collisions fail closed and history is never rewritten. Preview, approval, MFA, idempotency journal and read-back remain required for directory writes.

This is an accepted single-host availability compromise, not the permanent production topology. A Mac outage stops directory service, and production readiness therefore requires tested restore, encrypted backup copied off the Mac, restart verification, and explicit cutover evidence. Workstation domain login remains out of scope.

Status: Accepted temporarily on 2026-09-19.

