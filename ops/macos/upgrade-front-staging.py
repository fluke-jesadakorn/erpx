#!/usr/bin/env python3
"""Install a code-only release into isolated staging.

Run as root only after a reviewed release build. This never touches production,
Cloudflare, PostgreSQL schema, or the staging administrator credential.
"""
import argparse
import hashlib
import http.client
import json
import os
import re
import shutil
import subprocess
import time
from pathlib import Path

from erpxctl import STAGING, activate, backup, paused

FRONT_ORIGINS = {
    "ERPX_FRONT_PMS_ORIGIN": "https://stage-pms.jesadakorn.com",
    "ERPX_FRONT_POS_ORIGIN": "https://stage-pos.jesadakorn.com",
}
SITE_ORIGINS = {
    "ERPX_PUBLIC_ORIGIN": "https://console-erpx.jesadakorn.com",
    "ERPX_MARKETING_ORIGIN": "https://erpx.jesadakorn.com",
    "BETTER_AUTH_URL": "https://console-erpx.jesadakorn.com",
}


def code_only_release(source: Path):
    source = source.resolve()
    if not (source / "server.js").is_file() or not (source / "release.json").is_file():
        raise RuntimeError("RELEASE_NOT_BUILT")
    old = json.loads((STAGING / "current/release.json").read_text())
    new = json.loads((source / "release.json").read_text())
    if new["schemaSha256"] != old["schemaSha256"]:
        raise RuntimeError("SCHEMA_CHANGE_REQUIRES_REVIEWED_MIGRATION_PLAN")
    if new.get("node") != "24.20.0":
        raise RuntimeError("WRONG_NODE_RELEASE")
    if not re.fullmatch(r"[0-9TZ-]+", new["id"]):
        raise RuntimeError("INVALID_RELEASE_ID")
    return source, STAGING / "releases" / new["id"], old["id"]


def configured_env(original: bytes):
    text = original.decode("utf-8")
    if not re.search(r"^ERPX_PUBLIC_ORIGIN=https://(?:console-)?erpx\.jesadakorn\.com$", text, re.MULTILINE):
        raise RuntimeError("UNEXPECTED_STAGING_ORIGIN")
    for key, origin in {**FRONT_ORIGINS, **SITE_ORIGINS}.items():
        lines = [line for line in text.splitlines() if line.startswith(key + "=")]
        if len(lines) > 1:
            raise RuntimeError("DUPLICATE_" + key)
        if lines:
            text = re.sub(r"^" + key + r"=.*$", key + "=" + origin, text, flags=re.MULTILINE)
        else:
            text += ("" if text.endswith("\n") else "\n") + key + "=" + origin + "\n"
    return text.encode("utf-8")


def write_env(path: Path, content: bytes):
    stat = path.stat()
    temporary = path.with_name(path.name + ".front-next")
    if temporary.exists():
        raise RuntimeError("INSPECT_EXISTING_FRONT_ENV_TEMP_FILE")
    try:
        with temporary.open("xb") as stream:
            stream.write(content)
            stream.flush()
            os.fsync(stream.fileno())
        os.chown(temporary, stat.st_uid, stat.st_gid)
        os.chmod(temporary, stat.st_mode & 0o777)
        temporary.replace(path)
    finally:
        temporary.unlink(missing_ok=True)


def status(host: str, route: str):
    connection = http.client.HTTPConnection("127.0.0.1", 3103, timeout=5)
    try:
        connection.request("GET", route, headers={"Host": host, "X-Forwarded-Proto": "https"})
        response = connection.getresponse()
        response.read()
        return response.status
    finally:
        connection.close()


def same_file(source: Path, target: Path):
    def digest(path: Path):
        with path.open("rb") as stream:
            value = hashlib.sha256()
            for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                value.update(chunk)
            return value.digest()
    return source.is_file() and target.is_file() and digest(source) == digest(target)


def reusable_release(source: Path, target: Path):
    return (
        target.is_dir() and not target.is_symlink()
        and all(same_file(source / name, target / name) for name in ("release.json", "server.js", ".next/BUILD_ID"))
        and (target / ".next/cache").is_symlink()
        and (target / ".next/cache").resolve() == (STAGING / "data/next-cache").resolve()
    )


def verify_local():
    checks = [
        ("erpx.jesadakorn.com", "/api/health", 200),
        ("erpx.jesadakorn.com", "/", 200),
        ("erpx.jesadakorn.com", "/login", 308),
        ("erpx.jesadakorn.com", "/api/v1/holdings/acme/home", 404),
        ("console-erpx.jesadakorn.com", "/api/health", 200),
        ("console-erpx.jesadakorn.com", "/login", 200),
        ("stage-pms.jesadakorn.com", "/api/health", 200),
        ("stage-pos.jesadakorn.com", "/api/health", 200),
        ("stage-pms.jesadakorn.com", "/login", 200),
        ("stage-pos.jesadakorn.com", "/login", 200),
        ("stage-pms.jesadakorn.com", "/manifest.webmanifest", 200),
        ("stage-pos.jesadakorn.com", "/manifest.webmanifest", 200),
        ("stage-pms.jesadakorn.com", "/holdings/acme", 404),
        ("stage-pos.jesadakorn.com", "/api/v1/front/acme/acme-co/pms", 404),
        ("invalid.jesadakorn.com", "/api/health", 421),
    ]
    results = []
    deadline = time.monotonic() + 90
    while time.monotonic() < deadline:
        results = []
        for host, route, expected in checks:
            try:
                actual = status(host, route)
            except (OSError, TimeoutError) as error:
                actual = type(error).__name__ + ": " + str(error)
            results.append((host, route, actual, expected))
            if actual != expected:
                break
        if len(results) == len(checks) and all(actual == expected for _, _, actual, expected in results):
            print("Staging Front local host matrix: passed")
            return
        time.sleep(1)
    service = subprocess.run(
        ["launchctl", "print", "system/com.erpx.staging.web"],
        capture_output=True, text=True, check=False,
    )
    state = re.search(r"(?m)^\s*state = ([^\n]+)", service.stdout)
    pid = re.search(r"(?m)^\s*pid = (\d+)", service.stdout)
    launcher_log = STAGING / "logs/web-launcher.log"
    error_codes = []
    if launcher_log.is_file():
        # Show only fixed-shape error codes, never raw logs or environment data.
        tail = launcher_log.read_bytes()[-20000:].decode("utf-8", "replace")
        error_codes = re.findall(r"\b(?:Error|TypeError|SyntaxError): ([A-Z][A-Z0-9_]{2,79})\b", tail)[-5:]
    raise RuntimeError(
        "STAGING_FRONT_LOCAL_VERIFICATION_FAILED: " + str(results)
        + " web_state=" + (state.group(1) if state else "unknown")
        + " web_pid=" + (pid.group(1) if pid else "none")
        + " launcher_error_codes=" + str(error_codes)
    )


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--release", required=True, type=Path)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    source, target, old_id = code_only_release(args.release)
    if target.exists() and not reusable_release(source, target):
        raise RuntimeError("RELEASE_ALREADY_INSTALLED_WITH_DIFFERENT_CONTENT")
    if args.check:
        print("Code-only staging release is compatible:", target.name, "(reusing installed files)" if target.exists() else "(new copy)")
        return
    if os.geteuid() != 0:
        raise SystemExit("Run staging activation with sudo.")
    env_path = STAGING / "config/staging.env"
    original = env_path.read_bytes()
    updated = configured_env(original)
    print("Creating staging backup before activation.")
    backup(STAGING)
    if not target.exists():
        shutil.copytree(source, target, symlinks=True)
        cache_link = target / ".next/cache"
        cache_link.symlink_to(STAGING / "data/next-cache")
    try:
        with paused(STAGING):
            try:
                write_env(env_path, updated)
                activate(STAGING, target)
            except BaseException:
                write_env(env_path, original)
                activate(STAGING, STAGING / "releases" / old_id)
                raise
        verify_local()
    except BaseException:
        with paused(STAGING):
            write_env(env_path, original)
            activate(STAGING, STAGING / "releases" / old_id)
        raise
    print("Staging Front release active:", target.name)


if __name__ == "__main__":
    main()
