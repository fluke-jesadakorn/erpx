#!/usr/bin/env python3
"""Prepare and run isolated Alpine ARM64 Samba hosts on Apple Silicon.

The VMs expose only SSH, LDAPS and the reviewed ERPX runner on loopback.
They are not LAN domain controllers until an operator deliberately replaces
user-mode networking with an approved bridged network.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
BASE = ROOT / ".scratch" / "samba"
IMAGE_URL = "https://dl-cdn.alpinelinux.org/alpine/latest-stable/releases/cloud/generic_alpine-3.24.1-aarch64-uefi-cloudinit-r0.qcow2"
QEMU = "/opt/homebrew/bin/qemu-system-aarch64"
QEMU_IMG = "/opt/homebrew/bin/qemu-img"
FIRMWARE = "/opt/homebrew/share/qemu/edk2-aarch64-code.fd"
VARS = "/opt/homebrew/share/qemu/edk2-arm-vars.fd"
ENVIRONMENTS = {
    "prod": {"memory": 4096, "ssh": 19322, "ldaps": 19636, "runner": 19444},
    "dev": {"memory": 2048, "ssh": 20322, "ldaps": 20636, "runner": 20444},
}

def run(args, **kwargs):
    return subprocess.run([str(v) for v in args], check=True, **kwargs)

def verified_download(url, target):
    checksum = urllib.request.urlopen(url + ".sha512", timeout=30).read().decode().split()[0]
    partial = target.with_suffix(".partial")
    urllib.request.urlretrieve(url, partial)
    actual = hashlib.sha512(partial.read_bytes()).hexdigest()
    if actual != checksum:
        partial.unlink(missing_ok=True)
        raise RuntimeError("Downloaded Alpine image failed SHA-512 verification")
    os.replace(partial, target)

def paths(name):
    root = BASE / name
    return root, root / "disk.qcow2", root / "seed.iso", root / "qemu.pid"

def active(name):
    _, disk, _, pidfile = paths(name)
    if not pidfile.exists(): return False
    pid = pidfile.read_text().strip()
    result = subprocess.run(["ps", "-p", pid, "-o", "command="], capture_output=True, text=True)
    return result.returncode == 0 and str(disk) in result.stdout

def prepare(name):
    root, disk, seed, _ = paths(name)
    root.mkdir(parents=True, exist_ok=True, mode=0o700)
    key = BASE / "operator_ed25519"
    if not key.exists():
        run(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", key])
        key.chmod(0o600)
    base = BASE / "alpine-3.24.1-aarch64-cloudinit.qcow2"
    if not base.exists():
        verified_download(IMAGE_URL, base)
    if not disk.exists():
        run([QEMU_IMG, "create", "-f", "qcow2", "-F", "qcow2", "-b", base, disk, "24G"])
    nvram = root / "nvram.fd"
    if not nvram.exists(): shutil.copy2(VARS, nvram)
    seed_dir = root / "seed"
    seed_dir.mkdir(exist_ok=True, mode=0o700)
    public_key = key.with_suffix(".pub").read_text().strip()
    (seed_dir / "meta-data").write_text("instance-id: erpx-samba-%s\nlocal-hostname: samba-%s\n" % (name, name))
    (seed_dir / "user-data").write_text("""#cloud-config
users:
  - default
ssh_authorized_keys:
  - %s
package_update: true
packages: [chrony, openssl, ca-certificates, python3, py3-cryptography, doas]
runcmd:
  - [touch, /var/lib/erpx-samba-ready]
final_message: ERPX Samba base host ready
""" % public_key)
    if seed.exists(): seed.unlink()
    run(["hdiutil", "makehybrid", "-quiet", "-iso", "-joliet", "-default-volume-name", "cidata", "-o", seed, seed_dir])
    os.chmod(seed, 0o600)
    print(json.dumps({"environment": name, "disk": str(disk), "key": str(key), "prepared": True}))

def start(name):
    if active(name):
        print(json.dumps({"environment": name, "running": True, "reused": True})); return
    root, disk, seed, pid = paths(name)
    if not disk.exists(): prepare(name)
    cfg = ENVIRONMENTS[name]
    run([QEMU, "-name", "erpx-samba-" + name, "-machine", "virt,accel=hvf", "-cpu", "host", "-smp", "2", "-m", str(cfg["memory"]),
         "-drive", "if=pflash,format=raw,readonly=on,file=" + FIRMWARE,
         "-drive", "if=pflash,format=raw,file=" + str(root / "nvram.fd"),
         "-drive", "file=" + str(disk) + ",format=qcow2,if=virtio",
         "-drive", "file=" + str(seed) + ",format=raw,media=cdrom,readonly=on",
         "-device", "virtio-net-pci,netdev=net0",
         "-netdev", "user,id=net0,hostfwd=tcp:127.0.0.1:%d-:22,hostfwd=tcp:127.0.0.1:%d-:636,hostfwd=tcp:127.0.0.1:%d-:18444" % (cfg["ssh"], cfg["ldaps"], cfg["runner"]),
         "-display", "none", "-serial", "file:" + str(root / "serial.log"),
         "-qmp", "unix:" + str(root / "qmp.sock") + ",server=on,wait=off", "-pidfile", str(pid), "-daemonize"])
    print(json.dumps({"environment": name, "running": active(name), **cfg}))

def status(name):
    root, _, _, pid = paths(name)
    cfg = ENVIRONMENTS[name]
    ready = False
    if active(name):
        known_hosts = BASE / "known_hosts"
        result = subprocess.run(["ssh", "-i", BASE / "operator_ed25519", "-p", str(cfg["ssh"]), "-o", "BatchMode=yes", "-o", "ConnectTimeout=3", "-o", "StrictHostKeyChecking=yes", "-o", "UserKnownHostsFile=" + str(known_hosts), "alpine@127.0.0.1", "test -f /var/lib/erpx-samba-ready"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        ready = result.returncode == 0
    print(json.dumps({"environment": name, "running": active(name), "cloudInitReady": ready, "pidFile": str(pid), "serialLog": str(root / "serial.log"), **cfg}))

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("command", choices=("prepare", "start", "status"))
    parser.add_argument("environment", choices=tuple(ENVIRONMENTS))
    args = parser.parse_args()
    {"prepare": prepare, "start": start, "status": status}[args.command](args.environment)

if __name__ == "__main__": main()
