# Windows compatibility boundary

The current delivery prioritizes UCS. Windows host, RSAT, GPMC and endpoint acceptance testing are explicitly deferred by the user (2026-09-17). Do not enable Windows write operations in a credential profile until that lab has passed the checks below.

`PeopleDirectoryRunner.ps1` is the Windows adapter for the same reviewed directory-operation contract. It uses the GroupPolicy module and certificate-verified LDAP, with an operation journal under a private state directory. The editor compiles USB, screen lock and desktop settings separately from Drive/Printer Group Policy Preferences. No shell commands or arbitrary registry paths come from the browser.

## Installation contract

Run Windows PowerShell 5.1 or newer under a dedicated domain identity with the necessary OU and GPO delegation and RSAT Group Policy Management installed. Restrict the HTTPS listener to the ERPX server on a private network. Install a trusted server certificate and HTTP.sys HTTPS binding; do not disable certificate checks. Restrict the configuration, token file and state directory ACLs to this service identity and recovery administrators.

Configuration keys:

```json
{
  "ConnectionId": "reviewed-connection-id",
  "Prefix": "https://windows-runner.example.test:18444/",
  "TokenFile": "C:\\ProgramData\\ERPX\\runner.token",
  "Domain": "example.test",
  "Server": "dc.example.test",
  "LdapPort": 636,
  "BaseDn": "DC=example,DC=test",
  "WriteBaseDns": ["OU=ERPX-Lab,DC=example,DC=test"],
  "StateDirectory": "C:\\ProgramData\\ERPX\\state"
}
```

Keep the token in the private file, not in a command line. Start with `powershell.exe -NoProfile -File PeopleDirectoryRunner.ps1 -ConfigPath C:\ProgramData\ERPX\runner.json` through the machine's approved service/task mechanism. The ERPX server profile must explicitly allow the runner host, connection ID, write DNs and operation kinds. For UCS plus Samba, UDM remains the account/group writer and the runner handles GPO operations.

## Acceptance required before Windows write-back

- Prove observed immutable IDs and versions across UCS and Samba after synchronization, including rename, move and deletion/recreation.
- Exercise independent review, concurrent GPMC edits, a stopped runner, failed LDAP assertion, interrupted SYSVOL writes and repeated operation IDs. An uncertain partial GPO update must stop for a fresh review.
- Check GPO links, security filtering and XML in GPMC. Use explicit computer targets; a person's position alone does not select computers.
- Verify USB, screen lock and desktop behavior on both an included and excluded Windows machine/user, then Drive and Printer preferences separately.
- Capture Group Policy Results / `gpresult` and the actual endpoint behavior. A successful runner read-back means configuration was published; `endpointConfirmed` remains false and does not imply that a client applied the policy.

References: [Microsoft GPMC](https://learn.microsoft.com/en-us/windows-server/identity/ad-ds/manage/group-policy/group-policy-management-console), [Group Policy Preferences protocol](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-gppref/1b0df77e-9f78-4be0-b55a-988ac916b425).
