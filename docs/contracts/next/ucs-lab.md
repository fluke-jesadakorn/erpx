# UCS lab and People directory operations

Updated implementation: [HR/IT separation and Technology](hr-it-technology.md) and [UCS Samba runner](../../../ops/ucs/README.md). Samba GPO writes are now implemented and tested through the isolated runner; Windows endpoint acceptance remains deferred. The application connection remains read-only until explicitly activated/configured. Directory operations support direct IT approval; ERPX roles retain independent review.

The user selected `/Users/fluke/Desktop/Work/Tools/erpx-ucs-20260729`. The running VM uses an APFS clone at ignored `.scratch/ucs-lab/ucs-lab.qcow2`; the source disk and rescue overlay remain unchanged. The clone's backing reference was repaired to the source base image after the directory had moved.

The lab is UCS 5.2-6, hostname `ucs.erpx.test`, domain `erpx.test`. Samba AD and the S4 connector are installed in this clone to retain the Windows compatibility boundary. Windows/RSAT testing is deferred; UCS is the current acceptance target.

## Running and access

```sh
bun scripts/local/ucs-lab.ts --status
bun scripts/local/ucs-lab.ts
```

The launcher reuses the existing VM, checks its PID and disk path, and refuses occupied ports. It detaches QEMU, keeps its log/PID/QMP socket in `.scratch/ucs-lab/`, and leaves ERPX development and production processes alone. These forwards are loopback only:

| Host port | Lab service |
| --- | --- |
| 18322 | SSH, guest 22 |
| 18443 | UCS HTTPS / UDM REST, guest 443 |
| 18636 | UCS OpenLDAP TLS, guest 7636 |
| 18389 | UCS OpenLDAP, guest 7389; the application uses TLS |
| 18637 | Samba AD TLS, guest 636; reserved for later Windows tests |

Existing rescue-key access is used over SSH with `.scratch/ucs-lab/known_hosts`. Never disable host-key or certificate verification. The test service credential and CA are private files under `.scratch/ucs-lab/`; do not copy them into Git, browser forms, screenshots or chat. The source image's CA has a legacy malformed email DN that macOS curl rejects; the pinned Node/OpenSSL runtime verifies its trust chain and hostname. This does not relax the application's TLS checks.

The installed ERPX credential profile is restricted to the lab connection IDs and explicit UDM operations inside `ou=ERPX-Lab,dc=erpx,dc=test`. `ERPX_IAM_SECRET_PROFILES` in ignored `.env.local` points to the merged private profile. Other profiles are preserved. The public People connection **UCS lab · ERPX-Lab** is tested and inactive/read-only; activating a connection and proposing a write use the existing independent-review flow.

The launcher can clone the original source on an initial setup by passing its directory. The listed 7389/7636 ports apply after Samba installation; a plain UCS image initially uses 389/636. Preserve the prepared clone for subsequent starts.

## Verification

> Recorded observations apply to their stated date/environment. Current coverage and remaining acceptance are in the [verification inventory](../../status/capability-delivery-matrix.md); old gaps do not override current owner behavior.

```sh
ERPX_VERIFY_UCS_LAB=1 bun --env-file=.env.local run test:integration tests/provider/identity_access/directory-provider-live.test.ts
```

This opt-in test makes real changes only in the lab OU: disabled account creation, repeated creation, group creation, supported field updates, stale-version rejection, group membership, enable, OU move/retry and disable read-back. Its direct adapter activation checks provider behavior; application activation additionally requires a verified identity and an effective, reviewed lifecycle case. Created test fixtures use unique names and end disabled. The test is skipped in the normal integration suite.

Run real writes after package installation has finished. Registering UCS schema extensions temporarily restarts UDM and invalidates availability measurements. The durable application's worker retries transport errors using the same operation ID, verifies results, and stops on a version/identity conflict.

Public UI verification starts through normal authentication at [Acme Holding](https://acme.jesadakorn.com/), then the authorized Company/Administration Directory and access/device surfaces. Use current Holding paths; retired Console links cannot open business pages. A health endpoint alone is insufficient.

API reference: [Univention UDM REST](https://docs.software-univention.de/nubus-customization/1.x/en/api/udm-rest.html). Deferred Windows checks: [Windows runner](../../../ops/windows/README.md).
