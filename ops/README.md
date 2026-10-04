# ERPX operations

The current runtime is pinned Node.js 24.20.0 / Bun 1.2.17 / PostgreSQL 18.6, with private file-store adapters and PostgreSQL jobs behind Cloudflare Tunnel. [Runtime and private production](macos/README.md) owns service, backup, restore and release commands.

- [Incident handling](runbooks/nextjs-incident.md), [recovery](runbooks/nextjs-recovery.md), [canonical attachment rollout](macos/attachment-rollout.md)
- [Shared PostgreSQL topology](macos/postgres-consolidation.md), [Front applications](macos/front-apps.md)
- [UCS runner](ucs/README.md), [temporary Samba boundary](samba/README.md), [Windows compatibility and acceptance](windows/README.md)
- [Staging-only operations](macos/staging.md); use only on an explicit staging request

Development console: `https://console-dev-erpx.jesadakorn.com` → 3100. Private production: loopback 3101. Shared PostgreSQL: 5432. Preserve reserved ports 3000, 3001 and 8080 and retired PostgreSQL clusters 5433/5435. Dated installation/incident records are evidence of their original topology; they do not override current routing.
