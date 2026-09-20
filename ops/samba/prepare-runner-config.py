#!/usr/bin/env python3
"""Prepare root-private Alpine runner files from an existing ERPX profile."""
import argparse
import json
import os
import secrets
from pathlib import Path


def private_write(path: Path, value: str) -> None:
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w") as handle:
        handle.write(value)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--profiles", required=True, type=Path)
    parser.add_argument("--profile", required=True)
    parser.add_argument("--connection-id", required=True)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--new-service-password", action="store_true")
    args = parser.parse_args()
    profile = json.loads(args.profiles.read_text())[args.profile]
    args.output.mkdir(mode=0o700, parents=True, exist_ok=True)
    os.chmod(args.output, 0o700)
    password = secrets.token_urlsafe(36) if args.new_service_password else profile["bindPassword"]
    credentials = {"username": "erpx-runner", "password": password}
    config = {
        "connection_id": args.connection_id,
        "connection_ids": [args.connection_id],
        "listen_host": "0.0.0.0",
        "listen_port": 18444,
        "smb_conf": "/var/lib/erpx-samba-prod/etc/smb.conf",
        "ldap_url": "ldaps://samba-prod.erpx.test",
        "samba_host": "samba-prod.erpx.test",
        "ca_file": "/etc/erpx-gpo/ca.pem",
        "certificate_file": "/etc/erpx-gpo/server.crt",
        "private_key_file": "/etc/erpx-gpo/server.key",
        "token_file": "/etc/erpx-gpo/token",
        "credentials_file": "/etc/erpx-gpo/credentials.json",
        "state_directory": "/var/lib/erpx-gpo",
        "write_base_dns": profile["writeBaseDns"],
        "allowed_gpo_ids": [],
    }
    private_write(args.output / "config.json", json.dumps(config, indent=2) + "\n")
    private_write(args.output / "credentials.json", json.dumps(credentials) + "\n")
    private_write(args.output / "service-password", password + "\n")
    private_write(args.output / "token", profile["sambaRunnerToken"] + "\n")


if __name__ == "__main__":
    main()
