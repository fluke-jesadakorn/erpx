#!/usr/bin/env python3
"""Legacy isolated-cluster installer (5435); not for the shared 5432 deployment."""
import argparse
import grp
import json
import os
import plistlib
import pwd
import re
import secrets
import shutil
import socket
import subprocess
import time
from pathlib import Path
from urllib.parse import quote
from urllib.request import ProxyHandler, build_opener

BASE = Path('/Library/Application Support/ERPX-Staging')
ACCOUNT = '_erpx_staging'
WEB_PORT = 3103
PG_PORT = 5435
STAGING_ENV = 'staging.env'


def run(args, **kwargs):
    return subprocess.run([str(value) for value in args], check=True, **kwargs)


def ensure_free(port):
    with socket.socket() as sock:
        try:
            sock.bind(('127.0.0.1', port))
        except OSError:
            raise SystemExit(f'Port {port} is occupied; no process was stopped.')


def service_account(require_existing=False):
    try:
        user = pwd.getpwnam(ACCOUNT)
        group = grp.getgrnam(ACCOUNT)
        if user.pw_dir != str(BASE / 'data') or user.pw_shell != '/usr/bin/false':
            raise SystemExit(f'Existing {ACCOUNT} account has unexpected settings.')
        return user.pw_uid, group.gr_gid
    except KeyError:
        if require_existing:
            raise SystemExit(f'The {ACCOUNT} account is missing from the interrupted staging install.')
        used = {entry.pw_uid for entry in pwd.getpwall()} | {entry.gr_gid for entry in grp.getgrall()}
        uid = next((candidate for candidate in range(400, 500) if candidate not in used), None)
        if uid is None:
            raise SystemExit('No unused service account ID is available in the reserved range.')
        run(['dscl', '.', '-create', f'/Groups/{ACCOUNT}'])
        run(['dscl', '.', '-create', f'/Groups/{ACCOUNT}', 'PrimaryGroupID', str(uid)])
        run(['dscl', '.', '-create', f'/Groups/{ACCOUNT}', 'RealName', 'ERPX staging service'])
        run(['dscl', '.', '-create', f'/Users/{ACCOUNT}'])
        for key, value in {
            'UniqueID': str(uid), 'PrimaryGroupID': str(uid), 'UserShell': '/usr/bin/false',
            'NFSHomeDirectory': str(BASE / 'data'), 'RealName': 'ERPX staging service',
            'IsHidden': '1', 'Password': '*',
        }.items():
            run(['dscl', '.', '-create', f'/Users/{ACCOUNT}', key, value])
        return uid, uid


def empty_install_skeleton(path):
    expected = {
        '.': {'releases', 'runtime', 'ops', 'config', 'backups', 'data', 'logs'},
        'releases': set(), 'runtime': set(), 'ops': set(), 'config': set(),
        'backups': set(), 'logs': set(), 'data': {'postgres-logs', 'config'},
        'data/postgres-logs': set(), 'data/config': set(),
    }
    if path.is_symlink() or not path.is_dir():
        return False
    for relative, names in expected.items():
        directory = path if relative == '.' else path / relative
        if directory.is_symlink() or not directory.is_dir():
            return False
        children = list(directory.iterdir())
        if {child.name for child in children} != names:
            return False
        if any(child.is_symlink() or not child.is_dir() for child in children):
            return False
    return True


def read_env_file(path):
    values = {}
    if not path.is_file() or path.is_symlink():
        return None
    for line in path.read_text().splitlines():
        if not line:
            continue
        key, separator, value = line.partition('=')
        if not separator or not re.fullmatch(r'[A-Za-z_][A-Za-z0-9_]*', key) or key in values:
            return None
        values[key] = value
    return values


def pending_postgres_install(release_id, admin_email, credentials_path, payload, base=BASE,
                             launchd_dir=Path('/Library/LaunchDaemons')):
    if base.is_symlink() or not base.is_dir():
        return None
    expected_top = {'releases', 'runtime', 'ops', 'config', 'backups', 'data', 'logs', 'current'}
    if {path.name for path in base.iterdir()} != expected_top:
        return None
    for name in ['releases', 'runtime', 'ops', 'config', 'backups', 'data', 'logs']:
        path = base / name
        if path.is_symlink() or not path.is_dir():
            return None
    current = base / 'current'
    releases = base / 'releases'
    if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]{5,80}', release_id):
        return None
    if not current.is_symlink():
        return None
    try:
        current_destination = current.resolve(strict=True)
    except OSError:
        return None
    if current_destination.parent != releases.resolve() or not current_destination.is_dir():
        return None
    current_id = current_destination.name
    release_ids = set()
    for path in releases.iterdir():
        if path.is_symlink() or not path.is_dir() or not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9._-]{5,80}', path.name):
            return None
        try:
            metadata = json.loads((path / 'release.json').read_text())
        except (OSError, ValueError):
            return None
        if metadata.get('id') != path.name:
            return None
        release_ids.add(path.name)
    if current_id not in release_ids or (release_id in release_ids and release_id != current_id):
        return None
    destination = current_destination
    service_uid = pwd.getpwnam(ACCOUNT).pw_uid
    if (not (destination / 'tools/provision.mjs').is_file()
            or not (destination / 'tools/bootstrap-staging-admin.mjs').is_file()
            or not (destination / '.next/BUILD_ID').is_file()):
        return None
    if {path.name for path in (base / 'config').iterdir()} != {'staging.env', 'admin.json'}:
        return None
    data = base / 'data'
    data_entries = {path.name for path in data.iterdir()}
    required_data = {'postgres-logs', 'config', 'postgres', 'next-cache'}
    if not required_data.issubset(data_entries) or data_entries - required_data - {'Library', '.CFUserTextEncoding', 'files'}:
        return None
    encoding_file = data / '.CFUserTextEncoding'
    if encoding_file.exists() and (encoding_file.is_symlink() or not encoding_file.is_file()
                                   or encoding_file.stat().st_uid != pwd.getpwnam(ACCOUNT).pw_uid):
        return None
    library = data / 'Library'
    if library.exists():
        service_uid = pwd.getpwnam(ACCOUNT).pw_uid
        if library.is_symlink() or not library.is_dir() or library.stat().st_uid != service_uid:
            return None
        for path in library.rglob('*'):
            if path.is_symlink() or path.stat().st_uid != service_uid or not (path.is_dir() or path.is_file()):
                return None
    postgres_logs = base / 'data/postgres-logs'
    if postgres_logs.is_symlink() or not postgres_logs.is_dir():
        return None
    if any(path.is_symlink() or not path.is_file() or path.stat().st_uid != service_uid
           or not re.fullmatch(r'postgresql-(Mon|Tue|Wed|Thu|Fri|Sat|Sun)\.log', path.name)
           for path in postgres_logs.iterdir()):
        return None
    config_data = base / 'data/config'
    if config_data.is_symlink() or not config_data.is_dir():
        return None
    config_files = {path.name for path in config_data.iterdir()}
    manifest_names = {'databases.json', 'databases-jobs.json', 'databases-migrations.json'}
    if config_files not in (set(), {'provisioning.json'}, {'provisioning.json'} | manifest_names):
        return None
    if config_files:
        credentials_file = config_data / 'provisioning.json'
        if (credentials_file.is_symlink() or not credentials_file.is_file()
                or credentials_file.stat().st_uid not in (0, service_uid)
                or credentials_file.stat().st_mode & 0o077):
            return None
        try:
            saved_credentials = json.loads(credentials_file.read_text())
        except (OSError, ValueError):
            return None
        if (not isinstance(saved_credentials, dict) or not saved_credentials
                or any(not re.fullmatch(r'erpx_stage_[a-z0-9_]+_(app|jobs|migration)', role)
                       or not isinstance(value, str) or not re.fullmatch(r'[a-f0-9]{64}', value)
                       for role, value in saved_credentials.items())):
            return None
    for name in config_files & manifest_names:
        path = config_data / name
        if path.is_symlink() or not path.is_file() or path.stat().st_mode & 0o077:
            return None
        try:
            manifest = json.loads(path.read_text())
            for connection in manifest['databases'].values():
                from urllib.parse import urlsplit
                parsed = urlsplit(connection)
                if parsed.hostname != '127.0.0.1' or parsed.port != PG_PORT or not parsed.path.startswith('/erpx_stage_'):
                    return None
        except (OSError, ValueError, KeyError, TypeError):
            return None
    files = data / 'files'
    if files.exists():
        if files.is_symlink() or not files.is_dir():
            return None
        for path in files.iterdir():
            if path.name not in {'h001', 'h002', 'h003'} or path.is_symlink() or not path.is_dir() or any(path.iterdir()):
                return None
    cache = base / 'data/next-cache'
    if cache.is_symlink() or not cache.is_dir() or any(cache.iterdir()):
        return None
    logs = base / 'logs'
    if any(path.name != 'postgres-launcher.log' or path.is_symlink() or not path.is_file() for path in logs.iterdir()):
        return None
    pg_data = base / 'data/postgres'
    if pg_data.is_symlink() or not (pg_data / 'PG_VERSION').is_file() or (pg_data / 'PG_VERSION').read_text().strip() != '18':
        return None
    pg_conf = pg_data / 'postgresql.conf'
    if pg_conf.is_symlink() or not pg_conf.is_file():
        return None
    for setting in ["listen_addresses='127.0.0.1'", 'port=5435', "unix_socket_directories=''"]:
        if setting not in pg_conf.read_text():
            return None
    if not (base / 'runtime/postgres/bin/postgres').is_file() or not (base / 'runtime/node/bin/node').is_file():
        return None
    if credentials_path.exists():
        return None
    postgres_plist = launchd_dir / 'com.erpx.staging.postgres.plist'
    if not postgres_plist.is_file():
        return None
    if any((launchd_dir / f'com.erpx.staging.{role}.plist').exists() for role in ['jobs', 'web']):
        return None
    try:
        with postgres_plist.open('rb') as plist_file:
            daemon_config = plistlib.load(plist_file)
    except (OSError, plistlib.InvalidFileException, ValueError):
        return None
    accepted_programs = [
        [str(base / 'runtime/postgres/bin/postgres'), '-D', str(pg_data)],
        [str(payload / 'runtime/postgres/bin/postgres'), '-D', str(pg_data)],
    ]
    if (daemon_config.get('Label') != 'com.erpx.staging.postgres'
            or daemon_config.get('ProgramArguments') not in accepted_programs
            or daemon_config.get('UserName') != ACCOUNT or daemon_config.get('GroupName') != ACCOUNT):
        return None

    settings = read_env_file(base / 'config/staging.env')
    if not settings:
        return None
    expected_settings = {
        'ERPX_ENVIRONMENT': 'staging',
        'ERPX_PUBLIC_ORIGIN': 'https://console-erpx.jesadakorn.com',
        'ERPX_MARKETING_ORIGIN': 'https://erpx.jesadakorn.com',
        'ERPX_FRONT_PMS_ORIGIN': 'https://stage-pms.jesadakorn.com',
        'ERPX_FRONT_POS_ORIGIN': 'https://stage-pos.jesadakorn.com',
        'BETTER_AUTH_URL': 'https://console-erpx.jesadakorn.com',
        'ERPX_DATA_DIR': str(base / 'data'),
        'ERPX_DATABASE_MANIFEST': str(base / 'data/config/databases.json'),
        'ERPX_DATABASE_PREFIX': 'erpx_stage',
        'ERPX_ADMIN_EMAIL': admin_email,
        'ERPX_PUBLIC_BETA': 'false',
        'ERPX_EMAIL_ADAPTER': 'sandbox',
        'POS_STRIPE_LIVE_ENABLED': 'false',
        'POS_STRIPE_LIVE_SECRET_KEY': '',
        'POS_STRIPE_LIVE_WEBHOOK_SECRET': '',
        'POS_STRIPE_LIVE_CLIENT_ID': '',
        'PMS_STRIPE_LIVE_ENABLED': 'false',
    }
    if any(settings.get(key) != value for key, value in expected_settings.items()):
        return None
    if len(settings.get('BETTER_AUTH_SECRET', '')) < 32:
        return None
    if settings.get('STRIPE_SECRET_KEY') and not settings['STRIPE_SECRET_KEY'].startswith('sk_test_'):
        return None
    if settings.get('POS_STRIPE_TEST_SECRET_KEY') and not settings['POS_STRIPE_TEST_SECRET_KEY'].startswith('sk_test_'):
        return None
    if settings.get('PMS_STRIPE_BOOKING_ENABLED') != ('true' if settings.get('STRIPE_SECRET_KEY') else 'false'):
        return None
    if settings.get('RESEND_API_KEY') or settings.get('RESEND_FROM'):
        return None
    try:
        admin = json.loads((base / 'config/admin.json').read_text())
    except (OSError, ValueError):
        return None
    if (admin.get('host') != '127.0.0.1' or admin.get('port') != PG_PORT
            or admin.get('user') != 'erpx_stage_admin'
            or not isinstance(admin.get('password'), str) or len(admin['password']) < 32):
        return None
    return settings, admin, current_id


def parse_env(path):
    values = {}
    if not path.is_file():
        return values
    for line in path.read_text().splitlines():
        match = re.match(r'^\s*(?:export\s+)?([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.*?)\s*$', line)
        if not match or match.group(1) not in {
            'ERPX_ADMIN_EMAIL',
            'STRIPE_SECRET_KEY', 'STRIPE_WEBHOOK_SECRET',
            'POS_STRIPE_TEST_SECRET_KEY', 'POS_STRIPE_TEST_WEBHOOK_SECRET', 'POS_STRIPE_TEST_CLIENT_ID',
        }:
            continue
        key, value = match.groups()
        if len(value) >= 2 and value[0] == value[-1] and value[0] in "\"'":
            value = value[1:-1]
        values[key] = value
    return values


def staging_settings(test_env_path, admin_email):
    source = parse_env(test_env_path)
    stripe = source.get('STRIPE_SECRET_KEY', '')
    pos_stripe = source.get('POS_STRIPE_TEST_SECRET_KEY', '')
    if stripe and not stripe.startswith('sk_test_'):
        raise SystemExit('Staging accepts only a Stripe test secret key; check the configured source environment.')
    if pos_stripe and not pos_stripe.startswith('sk_test_'):
        raise SystemExit('Staging accepts only a POS Stripe test secret key; check the configured source environment.')
    stripe_webhook = source.get('STRIPE_WEBHOOK_SECRET', '')
    pos_webhook = source.get('POS_STRIPE_TEST_WEBHOOK_SECRET', '')
    if stripe_webhook and (not stripe or not stripe_webhook.startswith('whsec_')):
        raise SystemExit('A Stripe test webhook secret requires a matching test API key.')
    if pos_webhook and (not pos_stripe or not pos_webhook.startswith('whsec_')):
        raise SystemExit('A POS Stripe test webhook secret requires a matching test API key.')
    client_id = source.get('POS_STRIPE_TEST_CLIENT_ID', '')
    if client_id and (not pos_stripe or not client_id.startswith('ca_')):
        raise SystemExit('A POS Stripe test client ID requires a matching test API key.')

    return {
        'ERPX_ENVIRONMENT': 'staging',
        'ERPX_PUBLIC_ORIGIN': 'https://console-erpx.jesadakorn.com',
        'ERPX_MARKETING_ORIGIN': 'https://erpx.jesadakorn.com',
        'ERPX_FRONT_PMS_ORIGIN': 'https://stage-pms.jesadakorn.com',
        'ERPX_FRONT_POS_ORIGIN': 'https://stage-pos.jesadakorn.com',
        'BETTER_AUTH_URL': 'https://console-erpx.jesadakorn.com',
        'BETTER_AUTH_SECRET': secrets.token_urlsafe(48),
        'ERPX_DATA_DIR': str(BASE / 'data'),
        'ERPX_DATABASE_MANIFEST': str(BASE / 'data/config/databases.json'),
        'ERPX_DATABASE_PREFIX': 'erpx_stage',
        'ERPX_ADMIN_EMAIL': admin_email,
        'ERPX_PUBLIC_BETA': 'false',
        'ERPX_EMAIL_ADAPTER': 'sandbox',
        'ERPX_POS_ROOM_CHARGE_PILOT_ENABLED': 'false',
        'POS_STRIPE_LIVE_ENABLED': 'false',
        'POS_STRIPE_LIVE_SECRET_KEY': '',
        'POS_STRIPE_LIVE_WEBHOOK_SECRET': '',
        'POS_STRIPE_LIVE_CLIENT_ID': '',
        'PMS_STRIPE_LIVE_ENABLED': 'false',
        'PMS_STRIPE_BOOKING_ENABLED': 'true' if stripe else 'false',
        'STRIPE_SECRET_KEY': stripe,
        'STRIPE_WEBHOOK_SECRET': stripe_webhook,
        'STRIPE_PRICE_ID': '',
        'POS_STRIPE_TEST_SECRET_KEY': pos_stripe,
        'POS_STRIPE_TEST_WEBHOOK_SECRET': pos_webhook,
        'POS_STRIPE_TEST_CLIENT_ID': client_id,
    }


def write_env(path, settings):
    path.write_text(''.join(f'{key}={value}\n' for key, value in settings.items()))
    path.chmod(0o640)


def daemon(role, args, uid=None, gid=None, env=None):
    label = f'com.erpx.staging.{role}'
    data = {
        'Label': label,
        'ProgramArguments': [str(value) for value in args],
        'WorkingDirectory': str(BASE / 'current'),
        'EnvironmentVariables': {'LC_ALL': 'C', 'LANG': 'C', **(env or {})},
        'Umask': 0o077,
        'StandardOutPath': str(BASE / 'logs' / f'{role}-launcher.log'),
        'StandardErrorPath': str(BASE / 'logs' / f'{role}-launcher.log'),
        'RunAtLoad': True,
        'KeepAlive': True,
        'ThrottleInterval': 10,
        'ExitTimeOut': 120,
        'ProcessType': 'Background',
    }
    if uid is not None and gid is not None:
        data.update(UserName=ACCOUNT, GroupName=ACCOUNT)
    destination = Path('/Library/LaunchDaemons') / f'{label}.plist'
    destination.write_bytes(plistlib.dumps(data))
    destination.chmod(0o644)
    return destination


def await_postgres(pg_bin):
    for _ in range(60):
        result = subprocess.run([str(pg_bin / 'pg_isready'), '-h', '127.0.0.1', '-p', str(PG_PORT)], capture_output=True)
        if result.returncode == 0:
            return
        time.sleep(1)
    raise RuntimeError(f'Staging PostgreSQL did not start; inspect {BASE / "logs/postgres-*.log"}.')


def allow_expected_postgres_listener(pg_bin, loaded):
    with socket.socket() as sock:
        try:
            sock.bind(('127.0.0.1', PG_PORT))
            return
        except OSError:
            pass
    if not loaded:
        raise SystemExit(f'Port {PG_PORT} is occupied by a process outside the staging LaunchDaemon; no process was stopped.')
    state = subprocess.run(['launchctl', 'print', 'system/com.erpx.staging.postgres'], capture_output=True, text=True)
    pid_match = re.search(r'^\s*pid = (\d+)\s*$', state.stdout, re.MULTILINE)
    if state.returncode or not pid_match:
        raise SystemExit(f'Port {PG_PORT} is occupied, but the staging PostgreSQL process could not be verified.')
    pid = pid_match.group(1)
    listeners = subprocess.run(['/usr/sbin/lsof', '-nP', f'-iTCP:{PG_PORT}', '-sTCP:LISTEN', '-t'], capture_output=True, text=True)
    listener_pids = {line.strip() for line in listeners.stdout.splitlines() if line.strip()}
    if listeners.returncode not in (0, 1) or listener_pids != {pid}:
        raise SystemExit(f'Port {PG_PORT} is occupied by a process that does not match the staging PostgreSQL LaunchDaemon.')
    ready = subprocess.run([str(pg_bin / 'pg_isready'), '-h', '127.0.0.1', '-p', str(PG_PORT)], capture_output=True)
    if ready.returncode != 0:
        raise SystemExit(f'The staging PostgreSQL LaunchDaemon is not accepting connections on port {PG_PORT}.')


def wait_for_postgres_unload():
    service = 'system/com.erpx.staging.postgres'
    for _ in range(120):
        state = subprocess.run(['launchctl', 'print', service], capture_output=True)
        with socket.socket() as sock:
            try:
                sock.bind(('127.0.0.1', PG_PORT))
                port_is_free = True
            except OSError:
                port_is_free = False
        if state.returncode != 0 and port_is_free:
            return
        time.sleep(0.25)
    raise RuntimeError('Staging PostgreSQL did not fully unload; its launchd job or port 5435 is still active.')


def await_http(path):
    opener = build_opener(ProxyHandler({}))
    url = f'http://127.0.0.1:{WEB_PORT}{path}'
    for _ in range(90):
        try:
            with opener.open(url, timeout=3) as response:
                if response.status == 200:
                    return
        except Exception:
            time.sleep(1)
    raise RuntimeError(f'Staging {path} did not return HTTP 200 on loopback.')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--release', required=True, type=Path)
    parser.add_argument('--payload', required=True, type=Path)
    parser.add_argument('--test-env', type=Path, default=Path('.env.local'))
    parser.add_argument('--admin-email')
    args = parser.parse_args()
    if os.geteuid() != 0:
        raise SystemExit('Run install-staging.py with sudo; it creates an isolated service account and LaunchDaemons.')
    source_settings = parse_env(args.test_env.resolve())
    admin_email = (args.admin_email or source_settings.get('ERPX_ADMIN_EMAIL', '')).strip().lower()
    if not re.fullmatch(r'[^\s@]+@[^\s@]+\.[^\s@]+', admin_email):
        raise SystemExit('A valid staging administrator email is required.')
    release = args.release.resolve()
    payload = args.payload.resolve()
    metadata = json.loads((release / 'release.json').read_text())
    if not (payload / 'runtime/postgres/bin/initdb').is_file() or not (payload / 'runtime/node/bin/node').is_file():
        raise SystemExit('The pinned Node and PostgreSQL runtime payload is incomplete.')
    payload_node = payload / 'runtime/node/bin/node'
    payload_pg_bin = payload / 'runtime/postgres/bin'
    if subprocess.check_output([str(payload_node), '--version'], text=True).strip() != 'v24.20.0':
        raise SystemExit('Staging requires pinned Node 24.20.0.')
    if 'postgres (PostgreSQL) 18.6' not in subprocess.check_output([str(payload_pg_bin / 'postgres'), '--version'], text=True):
        raise SystemExit('Staging requires pinned PostgreSQL 18.6.')

    credentials_owner = int(os.environ.get('SUDO_UID', str(os.getuid())))
    credentials_user = pwd.getpwuid(credentials_owner)
    scratch_root = Path(__file__).resolve().parents[2] / '.scratch'
    credentials_path = scratch_root / 'erpx-staging/admin-credentials.txt'
    if not scratch_root.exists():
        scratch_root.mkdir(mode=0o700)
        os.chown(scratch_root, credentials_owner, credentials_user.pw_gid)
    elif scratch_root.stat().st_uid != credentials_owner:
        raise RuntimeError(f'{scratch_root} is not owned by the invoking user; staging credentials were not written.')
    if credentials_path.parent.exists():
        owner = credentials_path.parent.stat().st_uid
        if owner == 0 and not any(credentials_path.parent.iterdir()):
            # Recover the empty directory left by an interrupted first attempt.
            os.chown(credentials_path.parent, credentials_owner, credentials_user.pw_gid)
        elif owner != credentials_owner:
            raise RuntimeError(f'{credentials_path.parent} is not owned by the invoking user.')
        if credentials_path.exists():
            raise RuntimeError(f'{credentials_path} already exists; staging credentials were not overwritten.')
    else:
        credentials_path.parent.mkdir(mode=0o700)
        os.chown(credentials_path.parent, credentials_owner, credentials_user.pw_gid)

    partial_install = BASE.exists() or BASE.is_symlink()
    daemon_paths = [Path('/Library/LaunchDaemons') / f'com.erpx.staging.{role}.plist'
                    for role in ['postgres', 'jobs', 'web']]
    resume_state = 'new'
    if partial_install:
        if empty_install_skeleton(BASE):
            resume_state = 'skeleton'
        else:
            pending = pending_postgres_install(metadata['id'], admin_email, credentials_path, payload)
            if not pending:
                raise SystemExit(f'{BASE} contains staging files or data outside a recognized safe resume point.')
            resume_state = 'postgres-pending'
            settings, admin, previous_release_id = pending
    present_daemons = {role for role, path in zip(['postgres', 'jobs', 'web'], daemon_paths) if path.exists()}
    if resume_state == 'postgres-pending':
        if present_daemons != {'postgres'}:
            raise SystemExit('The interrupted staging install has unexpected LaunchDaemon files.')
    elif present_daemons:
        raise SystemExit('A staging LaunchDaemon file already exists; installer did not modify it.')
    if resume_state in {'skeleton', 'postgres-pending'}:
        service_account(require_existing=True)

    ensure_free(WEB_PORT)
    loaded_services = {}
    for role in ['postgres', 'jobs', 'web']:
        loaded = subprocess.run(['launchctl', 'print', f'system/com.erpx.staging.{role}'], capture_output=True).returncode == 0
        loaded_services[role] = loaded
        if loaded and not (resume_state == 'postgres-pending' and role == 'postgres'):
            raise SystemExit(f'Staging service com.erpx.staging.{role} is already loaded.')

    if resume_state == 'postgres-pending':
        allow_expected_postgres_listener(BASE / 'runtime/postgres/bin', loaded_services['postgres'])
    else:
        ensure_free(PG_PORT)

    uid, gid = service_account()
    if resume_state == 'postgres-pending':
        destination = BASE / 'releases' / metadata['id']
        node = BASE / 'runtime/node/bin/node'
        pg_bin = BASE / 'runtime/postgres/bin'
        if metadata['id'] != previous_release_id:
            shutil.copytree(release, destination)
            for path in destination.rglob('*'):
                if not path.is_symlink():
                    os.chown(path, 0, 0)
                    path.chmod(0o755 if path.is_dir() or path.stat().st_mode & 0o111 else 0o644)
            release_cache = destination / '.next/cache'
            if release_cache.is_symlink():
                release_cache.unlink()
            elif release_cache.exists():
                shutil.rmtree(release_cache)
            release_cache.symlink_to(BASE / 'data/next-cache')
            os.chmod(release_cache, 0o755, follow_symlinks=False)
            next_current = BASE / '.current-staging'
            if next_current.exists() or next_current.is_symlink():
                raise SystemExit(f'{next_current} exists; staging current release was not changed.')
            next_current.symlink_to(destination)
            os.chmod(next_current, 0o755, follow_symlinks=False)
            os.replace(next_current, BASE / 'current')
        shutil.copytree(Path(__file__).parent, BASE / 'ops', dirs_exist_ok=True)
        for path in (BASE / 'ops').rglob('*'):
            if not path.is_symlink():
                os.chown(path, 0, 0)
                path.chmod(0o755 if path.is_dir() or path.stat().st_mode & 0o111 else 0o644)
    else:
        settings = staging_settings(args.test_env.resolve(), admin_email)
        BASE.mkdir(mode=0o755, parents=True, exist_ok=True)
        BASE.chmod(0o755)
        for name in ['releases', 'runtime', 'ops']:
            (BASE / name).mkdir(mode=0o755, exist_ok=True)
            (BASE / name).chmod(0o755)
        for name in ['config', 'backups']:
            (BASE / name).mkdir(mode=0o750, exist_ok=True)
        for name in ['data', 'logs']:
            (BASE / name).mkdir(mode=0o700, exist_ok=True)
            os.chown(BASE / name, uid, gid)
        os.chown(BASE / 'config', 0, gid)
        (BASE / 'config').chmod(0o750)
        os.chown(BASE / 'backups', 0, gid)
        (BASE / 'backups').chmod(0o750)
        (BASE / 'data/postgres-logs').mkdir(mode=0o700, exist_ok=True)
        os.chown(BASE / 'data/postgres-logs', uid, gid)
        (BASE / 'data/config').mkdir(mode=0o700, exist_ok=True)
        os.chown(BASE / 'data/config', uid, gid)

        shutil.copytree(payload / 'runtime', BASE / 'runtime', dirs_exist_ok=True)
        destination = BASE / 'releases' / metadata['id']
        shutil.copytree(release, destination)
        shutil.copytree(Path(__file__).parent, BASE / 'ops', dirs_exist_ok=True)
        for tree in ['runtime', 'releases', 'ops']:
            for path in (BASE / tree).rglob('*'):
                if path.is_symlink():
                    continue
                os.chown(path, 0, 0)
                path.chmod(0o755 if path.is_dir() or path.stat().st_mode & 0o111 else 0o644)
        (BASE / 'current').symlink_to(destination)
        os.chmod(BASE / 'current', 0o755, follow_symlinks=False)
        write_env(BASE / 'config/staging.env', settings)
        os.chown(BASE / 'config/staging.env', 0, gid)
        admin = {'host': '127.0.0.1', 'port': PG_PORT, 'user': 'erpx_stage_admin', 'password': secrets.token_hex(32)}
        admin_path = BASE / 'config/admin.json'
        admin_path.write_text(json.dumps(admin))
        admin_path.chmod(0o600)

        node = BASE / 'runtime/node/bin/node'
        pg_bin = BASE / 'runtime/postgres/bin'
        pg_data = BASE / 'data/postgres'
        password_file = BASE / 'data/init-password'
        password_file.write_text(admin['password'])
        os.chown(password_file, uid, gid)
        password_file.chmod(0o600)
        try:
            run(['/usr/bin/sudo', '-u', ACCOUNT, str(pg_bin / 'initdb'), '-D', pg_data, '-U', admin['user'],
                 '--pwfile', password_file, '--auth-local=scram-sha-256', '--auth-host=scram-sha-256',
                 '--encoding=UTF8', '--locale=C'], cwd=BASE / 'data')
        finally:
            password_file.unlink(missing_ok=True)
        with (pg_data / 'postgresql.conf').open('a') as config:
            config.write(
                "\nlisten_addresses='127.0.0.1'\nport=5435\nunix_socket_directories=''\n"
                "max_connections=120\nshared_buffers='128MB'\npassword_encryption='scram-sha-256'\n"
                "logging_collector=on\nlog_directory='../postgres-logs'\nlog_filename='postgresql-%a.log'\n"
                "log_rotation_age='1d'\nlog_rotation_size=0\nlog_truncate_on_rotation=on\n"
                "log_min_messages=warning\nlog_min_error_statement=panic\nlog_statement='none'\n"
                "log_parameter_max_length_on_error=0\n"
            )
        os.chown(pg_data / 'postgresql.conf', uid, gid)
        cache = BASE / 'data/next-cache'
        cache.mkdir(mode=0o700)
        os.chown(cache, uid, gid)
        release_cache = destination / '.next/cache'
        if release_cache.is_symlink():
            release_cache.unlink()
        elif release_cache.exists():
            shutil.rmtree(release_cache)
        release_cache.symlink_to(cache)
        os.chmod(release_cache, 0o755, follow_symlinks=False)

    if subprocess.check_output([str(node), '--version'], text=True).strip() != 'v24.20.0':
        raise SystemExit('Staging requires pinned Node 24.20.0 in the installed runtime.')
    pg_data = BASE / 'data/postgres'
    if 'postgres (PostgreSQL) 18.6' not in subprocess.check_output([str(pg_bin / 'postgres'), '--version'], text=True):
        raise SystemExit('Staging requires pinned PostgreSQL 18.6 in the installed runtime.')

    if loaded_services.get('postgres'):
        run(['launchctl', 'bootout', 'system/com.erpx.staging.postgres'])
        wait_for_postgres_unload()
    postgres_plist = daemon('postgres', [BASE / 'runtime/postgres/bin/postgres', '-D', pg_data], uid, gid)
    run(['launchctl', 'bootstrap', 'system', postgres_plist])
    await_postgres(BASE / 'runtime/postgres/bin')
    database_url = f"postgresql://{admin['user']}:{quote(admin['password'], safe='')}@127.0.0.1:{PG_PORT}/postgres"
    provision_env = {
        **os.environ,
        'NODE_ENV': 'production',
        **settings,
        'ERPX_ADMIN_DATABASE_URL': database_url,
        'ERPX_DATABASE_PREFIX': 'erpx_stage',
        'ERPX_DATA_DIR': str(BASE / 'data'),
    }
    run([node, destination / 'tools/provision.mjs', '--development'], cwd=destination, env=provision_env)
    os.chown(BASE / 'data', uid, gid)
    for path in (BASE / 'data').rglob('*'):
        if not path.is_symlink():
            os.chown(path, uid, gid)
    migration_manifest = BASE / 'data/config/databases-migrations.json'
    settings['ERPX_MIGRATION_DATABASE_MANIFEST'] = str(migration_manifest)
    settings['ERPX_DATABASE_MANIFEST'] = str(BASE / 'data/config/databases.json')
    bootstrap_env = {
        **os.environ,
        'NODE_ENV': 'production',
        **settings,
        'ERPX_PROCESS_ROLE': 'jobs',
        'ERPX_DATABASE_MANIFEST': str(BASE / 'data/config/databases-jobs.json'),
        'ERPX_STAGING_CREDENTIALS_PATH': str(credentials_path),
    }
    run([node, destination / 'tools/bootstrap-staging-admin.mjs', admin_email], cwd=destination, env=bootstrap_env)
    os.chown(credentials_path, credentials_owner, pwd.getpwuid(credentials_owner).pw_gid)
    credentials_path.chmod(0o600)

    definitions = [
        daemon('jobs', [node, BASE / 'current/tools/launch.mjs', 'jobs'], uid, gid,
               {'ERPX_HOME': str(BASE), 'ERPX_ENV_FILE': STAGING_ENV, 'ERPX_PORT': str(WEB_PORT), 'PATH': '/usr/bin:/bin:/usr/sbin:/sbin'}),
        daemon('web', [node, BASE / 'current/tools/launch.mjs', 'web'], uid, gid,
               {'ERPX_HOME': str(BASE), 'ERPX_ENV_FILE': STAGING_ENV, 'ERPX_PORT': str(WEB_PORT), 'PATH': '/usr/bin:/bin:/usr/sbin:/sbin'}),
    ]
    for path in definitions:
        run(['launchctl', 'bootstrap', 'system', path])
    await_http('/api/health')
    await_http('/api/ready')
    print(f'Staging is ready at http://127.0.0.1:{WEB_PORT} with PostgreSQL on 127.0.0.1:{PG_PORT}.')
    print(f'Staging admin credentials are stored at {credentials_path}; no production auth data was copied.')
    print('Public DNS and Tunnel routes are managed separately in the Cloudflare dashboard.')


if __name__ == '__main__':
    main()
