#!/usr/bin/env python3
"""Move the existing 5432 data directory from a login agent to a boot daemon."""
import os
import shutil
import subprocess
import time
from pathlib import Path

SOURCE = 'gui/501/com.localdev.postgresql'
SOURCE_PLIST = Path('/Users/fluke/Library/LaunchAgents/com.localdev.postgresql.plist')
DESTINATION = 'system/com.erpx.shared-postgres'
DESTINATION_PLIST = Path('/Library/LaunchDaemons/com.erpx.shared-postgres.plist')
TEMPLATE = Path(__file__).with_name('com.erpx.shared-postgres.plist')
PG_ISREADY = '/opt/homebrew/opt/postgresql@18/bin/pg_isready'


def call(*args):
    subprocess.run(args, check=True)


def loaded(label):
    return subprocess.run(['launchctl', 'print', label], capture_output=True).returncode == 0


def ready():
    return subprocess.run([PG_ISREADY, '-h', '127.0.0.1', '-p', '5432'],
                          capture_output=True).returncode == 0


if os.geteuid() != 0:
    raise SystemExit('Run with sudo.')
if loaded(DESTINATION) or not loaded(SOURCE) or not ready():
    raise SystemExit('Expected the existing healthy user agent and no system daemon.')
if not SOURCE_PLIST.is_file() or DESTINATION_PLIST.exists():
    raise SystemExit('Unexpected launchd files; inspect before switching.')
shutil.copy2(TEMPLATE, DESTINATION_PLIST)
os.chown(DESTINATION_PLIST, 0, 0)
DESTINATION_PLIST.chmod(0o644)
call('plutil', '-lint', str(DESTINATION_PLIST))
switched = False
try:
    call('launchctl', 'disable', SOURCE)
    # SIGINT requests PostgreSQL fast shutdown; SIGTERM waits for every client
    # to disconnect and can stall while development keeps a pool open.
    call('launchctl', 'kill', 'SIGINT', SOURCE)
    for _ in range(30):
        if not ready():
            break
        time.sleep(1)
    else:
        raise RuntimeError('Original 5432 server did not stop')
    if loaded(SOURCE):
        call('launchctl', 'bootout', SOURCE)
    call('launchctl', 'bootstrap', 'system', str(DESTINATION_PLIST))
    for _ in range(45):
        if ready() and loaded(DESTINATION):
            switched = True
            break
        time.sleep(1)
    if not switched:
        raise RuntimeError('Shared PostgreSQL did not start as a system daemon')
    print('5432 is ready under system/com.erpx.shared-postgres; original data directory retained.')
finally:
    if not switched:
        if loaded(DESTINATION):
            call('launchctl', 'bootout', DESTINATION)
        call('launchctl', 'enable', SOURCE)
        if not loaded(SOURCE):
            call('launchctl', 'bootstrap', 'gui/501', str(SOURCE_PLIST))
        DESTINATION_PLIST.unlink(missing_ok=True)
