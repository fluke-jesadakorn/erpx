# Public origin incident — 2026-09-09

> Historical installation/incident evidence. The host/service observations below apply to 2026-09-09. Current topology and procedures are in [runtime operations](README.md); this record does not authorize restoring old Tunnel routes.

At approximately 15:26 UTC the public inbox returned Cloudflare 502.
The remotely managed `erpx-local` Tunnel configuration (version 2) routed
`erpx.jesadakorn.com` to `http://127.0.0.1:3100`. No development server was
listening on that port. The installed production service on port 3101
returned health and readiness successfully.

Restored only this hostname's ingress service to `http://127.0.0.1:3101`
(configuration version 3, 15:28:10 UTC), preserving other configuration.
Public health, readiness and login then returned HTTP 200. Development
remained stopped, demonstrating that public availability no longer depends
on the development process.

The public `/holdings/acme/inbox` still returns 404 on the installed release
`2026-09-08T18-24-25-307Z`. The inbox route exists in its build manifest;
the cause of that 404 is not yet established. Do not describe this as full
inbox recovery. Installed configuration/log inspection requires privileged
access; passwordless sudo was unavailable during this investigation.

## Subsequent user decision

The user explicitly requested that the public hostname serve dev on port
3100, with a detached server left running and public-page verification at
the end of every task. This supersedes the earlier recommendation to keep
the hostname on production. Follow the end-of-work requirements in
`AGENTS.md`: verify local dev before routing traffic, preserve production
on 3101, and do not silently switch the hostname back to production.
This does not add automatic startup after a machine reboot or guarantee
continuous uptime.

Implemented at 15:42:49 UTC: Tunnel configuration version 4 routes the
hostname to 3100 again, after local health passed. Dev runs detached with
logs and PID information in ignored `.scratch/dev/`. Local and public
health both returned 200 after the launch command ended; production health
also remained 200. The existing authenticated browser session opened the
public Acme inbox successfully and displayed the blue theme (`#1d4ed8`).
