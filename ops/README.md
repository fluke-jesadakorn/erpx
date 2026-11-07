# ERPX operations

The development and new-release toolchain is pinned Node.js 26.11.1 / Bun 1.4.3 / Next.js 16.4 / React 19.3 / PostgreSQL 18.6. The installed private production service remains on Node.js 24.20.0 until a reviewed release is deployed. [Runtime and private production](macos/README.md) owns service, backup, restore and release commands.

- [Incident handling](runbooks/nextjs-incident.md), [recovery](runbooks/nextjs-recovery.md), [canonical attachment rollout](macos/attachment-rollout.md)
- [Shared PostgreSQL topology](macos/postgres-consolidation.md), [Front applications](macos/front-apps.md)
- [UCS runner](ucs/README.md), [temporary Samba boundary](samba/README.md), [Windows compatibility and acceptance](windows/README.md)
- [Staging-only operations](macos/staging.md); use only on an explicit staging request

Development business entry uses the three registered Holding hosts with central Account through gateway3100; see [Holding development](macos/holding-development.md). Retired Console returns generic404 except exact health/ready reads. Private production: loopback 3101. Shared PostgreSQL: 5432. Preserve reserved ports 3000, 3001 and 8080 and retired PostgreSQL clusters 5433/5435. Dated installation/incident records are evidence of their original topology; they do not override current routing.
