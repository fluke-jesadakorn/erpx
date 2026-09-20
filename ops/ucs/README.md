# UCS Samba GPO runner

ERPX uses UDM for accounts, groups and OUs. This runner uses Samba Python bindings,
Samba LDAPS and authenticated SMB/SYSVOL for Group Policy. It does not execute shell
commands or accept arbitrary registry paths, scripts, XML, LDAP modifications or
filesystem paths from callers.

The finite operations are `gpo.publish`, `gpo.link`, `gpo.order`,
`gpo.inheritance` and `gpo.filter`. Supported settings are removable storage access,
inactivity lock, user wallpaper, mapped drives and shared printers. Computer policy
requires an explicit computer OU/group. GPMC link order 1 is represented by the
rightmost LDAP `gPLink` entry. Unknown registry entries, preference attributes,
preference nodes, extension names and other GPT files survive patches.
Patching an existing GPO changes settings only; links and security filtering use
their separate operations. Shared OU/group/member versions invalidate an old
settings preview when its affected targets change.

## Install on UCS

1. Install `gpo_runner.py` from this repository on the UCS domain controller with
   its system Python/Samba packages. Use a dedicated operating-system identity
   and dedicated delegated Samba/UDM credentials. Do not use a domain administrator
   account as the routine service identity.
2. Delegate creation of owned `groupPolicyContainer` objects in `CN=Policies`,
   write access to the approved existing GPC/GPT objects, and `gPLink`/`gPOptions`
   on approved OUs. Existing GPO delegation is separate from permission to create
   new GPOs. The UCS lab required an explicit class-specific Create Child ACE;
   membership in Group Policy Creator Owners alone was insufficient there.
3. Give the service identity access to its own GPT directories and read access
   to the configured TLS key/CA. Preserve authenticated-computer read access;
   security filtering changes the Apply Group Policy ACEs, not general delegation.
4. Create the private state directory with mode `0700`. Configuration, credentials,
   token and identity mapping files must be `0600`. Generate a random token of
   at least 32 characters and install the corresponding ERPX secret profile.
5. Run `python3 /opt/erpx/gpo_runner.py --config /etc/erpx-gpo/config.json` under the
   site's service supervisor. Bind to loopback behind a private tunnel, or a
   private management interface restricted to ERPX. The runner always serves HTTPS.

Example configuration (all identifiers/paths are placeholders):

```json
{
  "connection_id": "approved-erpx-connection-id",
  "connection_ids": ["approved-erpx-connection-id"],
  "listen_host": "127.0.0.1",
  "listen_port": 18444,
  "smb_conf": "/etc/samba/smb.conf",
  "ldap_url": "ldaps://ucs.example.test",
  "samba_host": "ucs.example.test",
  "udm_url": "https://ucs.example.test/univention/udm/",
  "ca_file": "/etc/erpx-gpo/ca.pem",
  "certificate_file": "/etc/erpx-gpo/server.crt",
  "private_key_file": "/etc/erpx-gpo/server.key",
  "token_file": "/etc/erpx-gpo/token",
  "credentials_file": "/etc/erpx-gpo/credentials.json",
  "identity_mapping_file": "/etc/erpx-gpo/identities.json",
  "state_directory": "/var/lib/erpx-gpo",
  "write_base_dns": ["OU=ERPX-Lab,DC=example,DC=test"],
  "allowed_gpo_ids": ["{EXISTING-GPO-CN-GUID}"]
}
```

Credentials contain `username`, `password`, `udmUsername`, `udmPassword`.
The trusted identity mapping is an array of
`{sambaGuid, udmUuid, udmDn, udmModule}`. Build it from verified UCS S4 connector
mapping plus the current permanent object identities; corroborate account/group
SIDs. Do not infer permanent pairs from names or assume UCS and Samba DNs match.
The runner rechecks UDM UUIDs and Samba GUIDs on every inventory read. Unmapped
objects inside an approved OU stop the read. Reconcile mappings after UDM moves,
creation or S4 replication before applying policies.

In the ERPX server secret profile, set `sambaRunnerUrl`, `sambaRunnerToken`, trusted
`caPem`, `tlsServerName`, `allowedHosts`, approved `connectionIds`, `writeBaseDns`
and `writeOperations`. Keep credentials out of application/database records.
Read/test the connector in Technology, then enable write-back with fresh MFA.
The Company IT operator previews and applies Directory changes directly. ERPX
role requests still need an independent reviewer.

UCS UDM recreates a populated container during subtree movement. ERPX rejects
that operation to preserve permanent identities: move children out, move the
empty OU, then return the children through explicit previews. LDAP checks all
children before execution; an identity change still stops read-back.

## Concurrency, retry and recovery

The API requires a connection ID, operation ID, typed action and the previewed
inventory. Versions fingerprint LDAP attributes and every GPT file. A process
lock serializes runner requests; the runner checks versions before and again
immediately before mutation, writes private backups, verifies changed values and
preserved files, and records an operation journal. Completed identical operations
return their original result. Reusing an ID with a different request is rejected.

LDAP and SYSVOL are separate stores: an LDAP transaction cannot roll back SMB
writes. A started/blocked operation is never blindly replayed. On an uncertain
partial result, inspect its `operation-*.json` and `backup-*.json` locally, compare
LDAP and SYSVOL, reconcile the fixture or restore with the site's recovery process,
read a fresh inventory and preview a new operation. Do not delete journal files to
force a retry. Ordinary transport retries retain the same ERPX operation ID and
skip already verified steps.

Backups contain directory information and policy files. Restrict and retain them
under the site's private backup policy. Restore is intentionally an operator
recovery action, not an arbitrary-file-write API.

`verified: true` means the server-side GPC/GPT write was read back. It always returns
`endpointConfirmed: false`; it does not claim that Windows clients ran `gpupdate`,
reached their printer/share or applied the policy. Windows endpoint validation is
outside this iteration. Share creation and filesystem resource ACL editing are
also outside scope (GPT permissions are part of GPO publication).

## Verification

```sh
python3 -m unittest discover -s ops/ucs -p 'test_*.py'
# On an isolated UCS lab only; creates and unlinks its own verification GPOs:
python3 verify_lab.py --config /etc/erpx-gpo/config.json \
  --target-ou 'OU=ERPX-Lab,DC=example,DC=test' --filter-group ERPX-TestGroup
# From ERPX with an approved private .scratch/ucs-lab/profiles.json:
ERPX_VERIFY_GPO_LAB=1 bun --env-file=.env.local run test:integration tests/directory-provider-live.test.ts
```

Primary references: [Univention's separate UCS LDAP/Samba LDAP/SYSVOL architecture](https://docs.software-univention.de/manual/5.2/en/windows/samba-domain.html),
[Samba GPO commands](https://www.samba.org/samba/docs/current/man-html/samba-tool.8.html),
[Microsoft Group Policy scope and filtering](https://learn.microsoft.com/en-us/windows-server/identity/ad-ds/manage/group-policy/group-policy-scope).
