# PMS Front and POS Front rollout

Both Front applications run in the existing Next.js web process. Host and API
allowlists in `src/proxy.ts` keep their pages, authentication cookies and daily
commands apart. Front sessions use separate Better Auth cookie prefixes and
host-only cookies. A remembered location/register is only a device preference;
the server checks the employee's current assignment on every request.

| Environment | PMS origin | POS origin | Loopback service |
| --- | --- | --- | --- |
| Development | `https://dev-pms.jesadakorn.com` | `https://dev-pos.jesadakorn.com` | `127.0.0.1:3100` |
| Staging | `https://stage-pms.jesadakorn.com` | `https://stage-pos.jesadakorn.com` | `127.0.0.1:3103` |
| Production, after acceptance | `https://pms.jesadakorn.com` | `https://pos.jesadakorn.com` | private `127.0.0.1:3101` |

## Code-only staging activation

Build with `bun run deploy:build` while the development server uses its separate
`.next` directory. The upgrade script refuses a release with a different schema
hash. Its dry run needs no privilege:

```sh
python3 ops/macos/upgrade-front-staging.py --release .runtime/releases/latest --check
```

Staging's installed files and environment belong to root and `_erpx_staging`.
An operator with administrator access runs:

```sh
sudo /usr/bin/python3 ops/macos/upgrade-front-staging.py --release .runtime/releases/latest
```

The script backs up staging, installs the immutable release, adds the two exact
staging origins to `staging.env`, switches only staging web/jobs, and checks the
three local hostnames plus login, manifests and host isolation. It restores the
old release and environment if local verification fails. It does not alter
production, database schema, DNS or Tunnel routes.

After the local checks pass, add only the two development hostnames to the
existing `erpx-local` Tunnel route pointing to `http://127.0.0.1:3100`, then the
two staging hostnames pointing to `http://127.0.0.1:3103`. Add proxied DNS records
for those hostnames to the same Tunnel target as the existing ERPX records.
Extend the existing Windows/Android redirect-rule exceptions to the four Front
hostnames so an installed app stays on its own origin. Preserve all other
Tunnel routes and the private production origin.

Check `/api/health`, `/login`, `/manifest.webmanifest` and an authenticated
Front page on each public hostname. Confirm that the other app's Front API,
`/holdings`, `/platform` and administration API routes return 404. Check a fake
host returns 421. Confirm sign in, session expiry, account recovery and sign out
stay on the same Front hostname. Install both PWAs separately on Chrome and
Safari devices and test the role-specific POS/PMS task paths with a ready staging
company. Do not publish `pms.jesadakorn.com` or `pos.jesadakorn.com` until those
business and device checks pass.
