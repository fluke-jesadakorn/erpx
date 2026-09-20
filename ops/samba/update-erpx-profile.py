#!/usr/bin/env python3
"""Atomically point an ERPX directory profile at the cut-over Samba DC."""
import argparse
import json
import os
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--profiles", required=True, type=Path)
    parser.add_argument("--profile", required=True)
    parser.add_argument("--credentials", required=True, type=Path)
    parser.add_argument("--ca", required=True, type=Path)
    parser.add_argument("--aliases", required=True, type=Path)
    args = parser.parse_args()
    profiles = json.loads(args.profiles.read_text())
    current = profiles[args.profile]
    credentials = json.loads(args.credentials.read_text())
    profiles[args.profile] = {
        "allowedHosts": ["127.0.0.1", "samba-prod.erpx.test"],
        "connectionIds": current["connectionIds"],
        "bindDn": "CN=erpx-runner,CN=Users,DC=erpx,DC=test",
        "bindPassword": credentials["password"],
        "caPem": args.ca.read_text(),
        "tlsServerName": "samba-prod.erpx.test",
        "sambaRunnerUrl": "https://127.0.0.1:19444",
        "sambaRunnerToken": current["sambaRunnerToken"],
        "identityAliases": json.loads(args.aliases.read_text()),
        "writeOperations": [],
        "writeBaseDns": current["writeBaseDns"],
    }
    temporary = args.profiles.with_suffix(args.profiles.suffix + ".new")
    fd = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w") as handle:
        json.dump(profiles, handle, indent=2)
        handle.write("\n")
        handle.flush()
        os.fsync(handle.fileno())
    os.replace(temporary, args.profiles)


if __name__ == "__main__":
    main()
