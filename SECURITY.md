# Security policy

## Supported source

The `development` branch receives security review during the Rust migration. There is no community production release or published stable support window established by this policy. Installed private services and dated verification receipts do not imply support for every historical snapshot.

## Report a vulnerability privately

Use [GitHub private vulnerability reporting](https://github.com/fluke-jesadakorn/erpx/security/advisories/new) when it is available for this repository. The initial security contact is [@fluke-jesadakorn](https://github.com/fluke-jesadakorn). If private reporting is unavailable, open an issue containing only a request for a private security contact, without technical details, personal data or credentials. The maintainer will arrange a private channel before requesting details.

Include the affected commit or version, a minimal reproduction using fictional data, impact and any proposed mitigation. Share sensitive material only through the agreed private channel. Do not place live tokens, database dumps, customer records or private logs in an issue, PR, browser trace or public artifact.

The maintainer aims to acknowledge a report within five business days and agree on the next update. Fix and disclosure timing depends on impact and available evidence; there is no guaranteed response or resolution service level. Coordinate publication after a fix and an appropriate migration path are available.

## Safe verification

Use isolated disposable databases and synthetic accounts. Do not probe production, staging, another Holding or an external provider without explicit authorization. Normal test credentials and local configuration must never grant access to installed environments. A security report does not authorize destructive testing or access to someone else's data.

If a real secret is discovered, privately identify its location rather than repeating its value. Revoke or rotate it before relying on history cleanup. Removing a working-tree file does not remove it from Git history, previous releases, LFS or other copies.

## Contribution and release boundaries

PR verification uses read-only permissions, public dependencies and disposable test services. Deployment credentials, repository visibility changes, public artifact publication and production acceptance are not ordinary PR actions. See [CONTRIBUTING.md](CONTRIBUTING.md) and the [public preparation gates](docs/community/open-source.md).
