#!/usr/bin/env python3
"""One-time, root-run ERPX database cutover to the existing localdev 5432 cluster.

Run staging first, then production. The source clusters and final frozen dumps
remain intact. This tool never changes a dev database or a Cloudflare route.
"""
import argparse
import datetime
import json
import os
import re
import shutil
import subprocess
import time
from pathlib import Path
from urllib.parse import unquote, urlsplit, urlunsplit

PG = Path('/opt/homebrew/opt/postgresql@18/bin')
HOMES = {
    'staging': (Path('/Library/Application Support/ERPX-Staging'), 'com.erpx.staging.', 'erpx_stage_', 3103),
    'production': (Path('/Library/Application Support/ERPX'), 'com.erpx.', 'erpx_', 3101),
}
TARGET_CREDENTIALS = Path('/Users/fluke/.config/dev-infra/credentials.json')
IDENTIFIER = re.compile(r'^[a-z][a-z0-9_]*$')


def identifier(value):
    if not IDENTIFIER.fullmatch(value) or len(value) > 63:
        raise RuntimeError('Unsafe PostgreSQL identifier')
    return '"' + value + '"'


def literal(value):
    return "'" + value.replace("'", "''") + "'"


def pg(admin, port, database, args, capture=False):
    env = {**os.environ, 'PGPASSWORD': admin['password'], 'PGCONNECT_TIMEOUT': '10'}
    command = [str(PG / args[0]), '-h', '127.0.0.1', '-p', str(port), '-U', admin['user']]
    if args[0] == 'psql':
        command += ['-X', '-v', 'ON_ERROR_STOP=1', '-d', database]
    else:
        command += ['-d', database]
    command += args[1:]
    result = subprocess.run(command, env=env, capture_output=capture, text=capture, check=True)
    return result.stdout.strip() if capture else None


def sql(admin, port, database, query):
    return pg(admin, port, database, ['psql', '-At', '-c', query], True)


def run(args):
    subprocess.run([str(a) for a in args], check=True)


def service(label, action):
    name = 'system/' + label
    loaded = subprocess.run(['launchctl', 'print', name], capture_output=True).returncode == 0
    if action == 'stop' and loaded:
        run(['launchctl', 'bootout', name])
    if action == 'start' and not loaded:
        run(['launchctl', 'bootstrap', 'system', '/Library/LaunchDaemons/' + label + '.plist'])


def await_http(port, path):
    for _ in range(45):
        result = subprocess.run(['curl', '-fsS', '-o', '/dev/null', '--max-time', '2',
                                 f'http://127.0.0.1:{port}{path}'], capture_output=True)
        if result.returncode == 0:
            return
        time.sleep(1)
    raise RuntimeError(f'HTTP check failed: {port}{path}')


def inventory(admin, port, database):
    query = """CREATE TEMP TABLE erpx_cutover_counts(name text, amount bigint);
DO $$ DECLARE item record; amount bigint; BEGIN
  FOR item IN SELECT schemaname,tablename FROM pg_tables
      WHERE schemaname = 'public' LOOP
    EXECUTE format('SELECT count(*) FROM %I.%I',item.schemaname,item.tablename) INTO amount;
    INSERT INTO erpx_cutover_counts VALUES (item.schemaname||'.'||item.tablename,amount);
  END LOOP;
END $$;
SELECT json_build_object(
 'tables',(SELECT count(*) FROM erpx_cutover_counts),
 'rows',(SELECT coalesce(sum(amount),0) FROM erpx_cutover_counts),
 'countsHash',(SELECT md5(coalesce(string_agg(name||':'||amount,',' ORDER BY name),'')) FROM erpx_cutover_counts),
 'schemaObjects',(SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public'),
 'migrations',(SELECT count(*) FROM public.erpx_migrations)
);"""
    return json.loads(sql(admin, port, database, query).splitlines()[-1])


def source_roles(admin, port, prefix):
    pattern = '^' + prefix + '[a-z0-9_]+$'
    query = "SELECT coalesce(json_agg(row_to_json(r)),'[]'::json) FROM (" \
        "SELECT rolname,rolsuper,rolinherit,rolcreaterole,rolcreatedb,rolcanlogin," \
        "rolreplication,rolbypassrls,rolconnlimit,rolpassword,rolvaliduntil::text " \
        "FROM pg_authid WHERE rolname=" + literal(admin['user']) + \
        " OR rolname ~ " + literal(pattern) + " ORDER BY rolname) r"
    roles = json.loads(sql(admin, port, 'postgres', query))
    if prefix == 'erpx_':
        roles = [role for role in roles if not role['rolname'].startswith(('erpx_dev_', 'erpx_stage_', 'erpx_test_'))]
    assert any(role['rolname'] == admin['user'] and role['rolsuper'] for role in roles)
    assert all(not role['rolsuper'] for role in roles if role['rolname'] != admin['user'])
    return roles


def create_roles(target, roles):
    existing = set(sql(target, 5432, 'postgres', "SELECT rolname FROM pg_roles WHERE rolname IN (" +
                       ','.join(literal(role['rolname']) for role in roles) + ')').splitlines())
    if existing:
        raise RuntimeError('Destination role collision: ' + ', '.join(sorted(existing)))
    for role in roles:
        attributes = [
            'SUPERUSER' if role['rolsuper'] else 'NOSUPERUSER',
            'INHERIT' if role['rolinherit'] else 'NOINHERIT',
            'CREATEROLE' if role['rolcreaterole'] else 'NOCREATEROLE',
            'CREATEDB' if role['rolcreatedb'] else 'NOCREATEDB',
            'LOGIN' if role['rolcanlogin'] else 'NOLOGIN',
            'REPLICATION' if role['rolreplication'] else 'NOREPLICATION',
            'BYPASSRLS' if role['rolbypassrls'] else 'NOBYPASSRLS',
            'CONNECTION LIMIT ' + str(role['rolconnlimit']),
        ]
        if role['rolpassword']:
            attributes.append('PASSWORD ' + literal(role['rolpassword']))
        if role['rolvaliduntil']:
            attributes.append('VALID UNTIL ' + literal(role['rolvaliduntil']))
        sql(target, 5432, 'postgres', 'CREATE ROLE ' + identifier(role['rolname']) + ' WITH ' + ' '.join(attributes))


def rewrite_manifest(path, names, source_port):
    source = path.read_bytes()
    document = json.loads(source)
    for key, value in document['databases'].items():
        url = urlsplit(value)
        if url.hostname != '127.0.0.1' or url.port != source_port or url.path[1:] not in names:
            raise RuntimeError(f'Unexpected manifest URL in {path.name}:{key}')
        document['databases'][key] = urlunsplit((url.scheme, url.netloc.replace(':' + str(source_port), ':5432'),
                                                  url.path, url.query, url.fragment))
    updated = json.dumps(document, indent=2).encode() + b'\n'
    replacement = path.with_name(path.name + '.cutover')
    replacement.write_bytes(updated)
    shutil.copystat(path, replacement)
    os.chown(replacement, path.stat().st_uid, path.stat().st_gid)
    replacement.replace(path)


def rollback_configs(originals):
    for path, backup in originals:
        shutil.copy2(backup, path)
        os.chown(path, backup.stat().st_uid, backup.stat().st_gid)


def restrict_target_logins(target, environment, names, roles, cutover):
    """Deny migrated logins access outside their own database set in HBA.

    Other projects can have PUBLIC CONNECT on their databases. Per-database
    REVOKE on the imported ERPX databases alone cannot isolate these logins.
    """
    hba = Path(sql(target, 5432, 'postgres', 'SHOW hba_file'))
    contents = hba.read_text()
    start = '# BEGIN ERPX ' + environment + ' isolation\n'
    if start in contents:
        raise RuntimeError('HBA isolation block already exists')
    end = '# END ERPX ' + environment + ' isolation\n'
    scoped_roles = sorted(role['rolname'] for role in roles if role['rolname'] !=
                          ('erpx_stage_admin' if environment == 'staging' else 'erpx_admin'))
    if not scoped_roles or any(',' in value for value in names + scoped_roles):
        raise RuntimeError('Invalid HBA name list')
    role_list = ','.join(scoped_roles)
    block = start + '\n'.join([
        'host ' + ','.join(names) + ' ' + role_list + ' 127.0.0.1/32 scram-sha-256',
        'host all ' + role_list + ' 127.0.0.1/32 reject',
        'host all ' + role_list + ' ::1/128 reject',
        'local all ' + role_list + ' reject',
    ]) + '\n' + end
    previous = cutover / 'target-pg_hba.conf.before'
    shutil.copy2(hba, previous)
    temporary = hba.with_name('pg_hba.conf.erpx-cutover')
    temporary.write_text(block + contents)
    shutil.copystat(hba, temporary)
    os.chown(temporary, hba.stat().st_uid, hba.stat().st_gid)
    temporary.replace(hba)
    if sql(target, 5432, 'postgres', 'SELECT pg_reload_conf()') != 't':
        raise RuntimeError('Could not reload target HBA')
    if sql(target, 5432, 'postgres', 'SELECT count(*) FROM pg_hba_file_rules WHERE error IS NOT NULL') != '0':
        raise RuntimeError('Target HBA has a parse error')
    return hba, previous


def restore_hba(target, backup):
    if backup:
        hba, previous = backup
        shutil.copy2(previous, hba)
        os.chown(hba, previous.stat().st_uid, previous.stat().st_gid)
        sql(target, 5432, 'postgres', 'SELECT pg_reload_conf()')


def migrate(environment):
    if os.geteuid() != 0:
        raise SystemExit('Run with sudo.')
    home, label, prefix, web_port = HOMES[environment]
    source = json.loads((home / 'config/admin.json').read_text())
    if source['host'] != '127.0.0.1' or source['port'] != (5435 if environment == 'staging' else 5433):
        raise RuntimeError('Unexpected source administrator configuration')
    target = {'user': 'localdev_admin', 'password': json.loads(TARGET_CREDENTIALS.read_text())['localdev_admin']}
    if sql(target, 5432, 'postgres', "SELECT current_setting('server_version')") .split(' ')[0] != '18.6':
        raise RuntimeError('Target is not PostgreSQL 18.6')
    manifest_paths = [home / 'data/config' / name for name in
                      ('databases.json', 'databases-jobs.json')]
    migration_manifest = home / 'data/config/databases-migrations.json'
    if migration_manifest.is_file():
        manifest_paths.append(migration_manifest)
    manifests = [json.loads(path.read_text()) for path in manifest_paths]
    names = sorted({urlsplit(url).path[1:] for url in manifests[0]['databases'].values()})
    if not names or any(not name.startswith(prefix) or not IDENTIFIER.fullmatch(name) or
                        (environment == 'production' and name.startswith(('erpx_dev_', 'erpx_stage_', 'erpx_test_')))
                        for name in names):
        raise RuntimeError('Unexpected source database names')
    for manifest in manifests[1:]:
        if {urlsplit(url).path[1:] for url in manifest['databases'].values()} != set(names):
            raise RuntimeError('Manifest database mismatch')
    roles = source_roles(source, source['port'], prefix)
    target_dbs = set(sql(target, 5432, 'postgres', 'SELECT datname FROM pg_database').splitlines())
    if target_dbs.intersection(names):
        raise RuntimeError('Destination database collision')
    # A root-private pre-cutover snapshot already exists. Take the final dumps
    # only after writers have stopped so the restore has one frozen source state.
    backups = [p for p in (home / 'backups').iterdir() if p.is_dir() and (p / 'backup.json').is_file()]
    if not backups:
        raise RuntimeError('A verified operator backup is required first')
    marker = home / 'data/maintenance'
    if marker.exists():
        raise RuntimeError('Maintenance already active')
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S.%fZ')
    cutover = home / 'backups' / ('consolidation-' + stamp)
    cutover.mkdir(mode=0o700)
    os.chmod(cutover, 0o700)
    originals = []
    hba_backup = None
    switched = False
    marker.touch(mode=0o644)
    try:
        service(label + 'web', 'stop')
        service(label + 'jobs', 'stop')
        for _ in range(60):
            live = sql(source, source['port'], 'postgres',
                       'SELECT count(*) FROM pg_stat_activity WHERE datname IN (' +
                       ','.join(literal(name) for name in names) + ") AND pid<>pg_backend_pid()")
            if live == '0':
                break
            time.sleep(1)
        else:
            raise RuntimeError('Source still has active database sessions')
        (cutover / 'databases').mkdir(mode=0o700)
        source_inventory = {}
        for name in names:
            source_inventory[name] = inventory(source, source['port'], name)
            pg(source, source['port'], name,
               ['pg_dump', '-Fc', '-f', str(cutover / 'databases' / (name + '.dump'))])
        shutil.copytree(home / 'data/files', cutover / 'files')
        shutil.copytree(home / 'data/config', cutover / 'data-config')
        shutil.copytree(home / 'config', cutover / 'config')
        (cutover / 'inventory.json').write_text(json.dumps(source_inventory, indent=2) + '\n')
        os.sync()
        print(environment, 'frozen database dumps:', len(names), flush=True)
        create_roles(target, roles)
        hba_backup = restrict_target_logins(target, environment, names, roles, cutover)
        for name in names:
            pg(target, 5432, 'postgres',
               ['pg_restore', '--create', '--exit-on-error', str(cutover / 'databases' / (name + '.dump'))])
            if inventory(target, 5432, name) != source_inventory[name]:
                raise RuntimeError('Inventory mismatch after restore: ' + name)
            # pg_restore --create restores database ACLs; reject PUBLIC CONNECT.
            public_connect = "SELECT EXISTS (SELECT 1 FROM pg_database d, " \
                "aclexplode(coalesce(d.datacl,acldefault('d',d.datdba))) acl " \
                "WHERE d.datname=" + literal(name) + " AND acl.grantee=0 AND acl.privilege_type='CONNECT')"
            if sql(target, 5432, 'postgres', public_connect) == 't':
                raise RuntimeError('PUBLIC can connect to ' + name)
        runtime_roles = [role['rolname'] for role in roles if role['rolname'].endswith(('_app', '_jobs'))]
        other_filter = "datname ~ '^erpx_(dev|stage|test)_'" if environment == 'production' else \
            "datname ~ '^erpx_(dev|test)_' OR (datname ~ '^erpx_' AND datname !~ '^erpx_(stage|dev|test)_')"
        other_names = sql(target, 5432, 'postgres',
                          'SELECT datname FROM pg_database WHERE (' + other_filter + ') ORDER BY datname').splitlines()
        if other_names:
            credentials = {}
            for manifest in manifests:
                for url in manifest['databases'].values():
                    parsed = urlsplit(url)
                    credentials[unquote(parsed.username)] = unquote(parsed.password)
            for role in runtime_roles:
                password = credentials[role]
                for other in [other_names[0], other_names[-1]]:
                    probe = subprocess.run([str(PG / 'psql'), '-X', '-At', '-h', '127.0.0.1',
                                            '-p', '5432', '-U', role, '-d', other, '-c', 'SELECT 1'],
                                           env={**os.environ, 'PGPASSWORD': password, 'PGCONNECT_TIMEOUT': '3'},
                                           capture_output=True, text=True)
                    if probe.returncode == 0:
                        raise RuntimeError('Cross-environment connection succeeded')
        for path in manifest_paths + [home / 'config/admin.json']:
            backup = cutover / 'original-config' / path.relative_to(home)
            backup.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
            shutil.copy2(path, backup)
            originals.append((path, backup))
        for path in manifest_paths:
            rewrite_manifest(path, set(names), source['port'])
        source['port'] = 5432
        admin_path = home / 'config/admin.json'
        temporary = admin_path.with_name('admin.json.cutover')
        temporary.write_text(json.dumps(source))
        temporary.chmod(0o600)
        temporary.replace(admin_path)
        switched = True
        marker.unlink()
        service(label + 'jobs', 'start')
        service(label + 'web', 'start')
        await_http(web_port, '/api/health')
        await_http(web_port, '/api/ready')
        service(label + 'postgres', 'stop')
        run(['launchctl', 'disable', 'system/' + label + 'postgres'])
        print(environment, 'migrated and source PostgreSQL unloaded', flush=True)
    except Exception:
        service(label + 'web', 'stop')
        service(label + 'jobs', 'stop')
        if switched:
            rollback_configs(originals)
        restore_hba(target, hba_backup)
        marker.unlink(missing_ok=True)
        run(['launchctl', 'enable', 'system/' + label + 'postgres'])
        service(label + 'postgres', 'start')
        service(label + 'jobs', 'start')
        service(label + 'web', 'start')
        raise


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('environment', choices=HOMES)
    migrate(parser.parse_args().environment)
