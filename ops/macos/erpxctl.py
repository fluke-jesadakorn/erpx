#!/usr/bin/env python3
"""Operator CLI. Backups contain credentials and stay root-private on this Mac."""
import argparse, contextlib, datetime, fcntl, hashlib, json, os, re, shutil, subprocess, sys, time
from pathlib import Path
from urllib.parse import urlparse, unquote
from offsite import export_snapshot, import_archive
from recovery_status import recovery_status
DEFAULT=Path('/Library/Application Support/ERPX')

def run(args, **kwargs):
    return subprocess.run([str(x) for x in args], check=True, **kwargs)

def services(home, action):
    for role in (['web','jobs'] if action=='stop' else ['jobs','web']):
        label='com.erpx.'+role
        if action=='stop':
            found=subprocess.run(['launchctl','print','system/'+label],capture_output=True,text=True)
            if found.returncode: continue
            match=re.search(r'\bpid = (\d+)',found.stdout)
            run(['launchctl','bootout','system/'+label])
            if match:
                pid=int(match.group(1))
                for _ in range(120):
                    try: os.kill(pid,0)
                    except ProcessLookupError: break
                    time.sleep(1)
                else: raise RuntimeError('Service did not stop; maintenance remains active')
        elif subprocess.run(['launchctl','print','system/'+label],capture_output=True).returncode:
            run(['launchctl','bootstrap','system','/Library/LaunchDaemons/'+label+'.plist'])

def postgres(home):
    admin=json.loads((home/'config/admin.json').read_text())
    return {**os.environ,'PGHOST':admin['host'],'PGPORT':str(admin['port']),'PGUSER':admin['user'],
            'PGPASSWORD':admin['password'],'PGCONNECT_TIMEOUT':'10'}, home/'runtime/postgres/bin'

def database_names(home):
    manifest=json.loads((home/'data/config/databases.json').read_text())
    names=sorted({unquote(urlparse(u).path.removeprefix('/')) for u in manifest['databases'].values()})
    if not names or any(not re.fullmatch(r'erpx(?:_[a-z0-9]+)+',n) for n in names): raise ValueError('Invalid ERPX database manifest')
    return names

@contextlib.contextmanager
def paused(home, offline=False, resume_on_error=True):
    flag=home/'data/maintenance'
    if flag.exists(): raise RuntimeError('Maintenance already active; inspect before continuing')
    flag.touch(mode=0o644)
    successful=False
    try:
        if not offline: services(home,'stop')
        env,pg=postgres(home)
        # Bootout waits for the process group; reject any surviving app connection.
        names=database_names(home)
        # Names have a strict identifier allowlist. Other ERPX development/test
        # databases on this cluster are outside this operation's maintenance scope.
        targets=','.join("'"+name+"'" for name in names)
        live=run([pg/'psql','-d','postgres','-Atc',"SELECT count(*) FROM pg_stat_activity WHERE application_name IN ('erpx-app','erpx-jobs') AND datname IN ("+targets+")"],env=env,capture_output=True,text=True).stdout.strip()
        if live!='0': raise RuntimeError('ERPX connections still active')
        yield
        successful=True
    finally:
        if successful or resume_on_error:
            flag.unlink(missing_ok=True)
            if not offline: services(home,'start')

def checksums(directory):
    result={}
    for p in sorted(directory.rglob('*')):
        if p.is_file() and p.name!='checksums.json':
            digest=hashlib.sha256()
            with p.open('rb') as f:
                for chunk in iter(lambda:f.read(1024*1024),b''): digest.update(chunk)
            result[str(p.relative_to(directory))]=digest.hexdigest()
    return result

def backup(home, offline=False, protect=None, offsite=True):
    parent=home/'backups';parent.mkdir(mode=0o700,exist_ok=True)
    stamp=datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S.%fZ')
    temporary=parent/('.'+stamp+'.partial');temporary.mkdir(mode=0o700)
    with paused(home,offline):
        env,pg=postgres(home);names=database_names(home)
        (temporary/'databases').mkdir()
        for name in names:
            run([pg/'pg_dump','--format=custom','--file',temporary/'databases'/(name+'.dump'),'--dbname',name],env=env)
        shutil.copytree(home/'data/files',temporary/'files')
        shutil.copytree(home/'data/config',temporary/'data-config')
        shutil.copytree(home/'config',temporary/'config')
        release=json.loads((home/'current/release.json').read_text())
        (temporary/'backup.json').write_text(json.dumps({'version':1,'createdAt':stamp,'release':release,'databases':names},indent=2))
        (temporary/'checksums.json').write_text(json.dumps(checksums(temporary),indent=2))
        # Flush the completed snapshot before publishing it.
        os.sync(); destination=parent/stamp;temporary.rename(destination)
    offsite_config=home/'config/offsite.json'
    if offsite and offsite_config.exists():
        if offsite_config.stat().st_mode & 0o077: raise RuntimeError('OFFSITE_CONFIG_NOT_PRIVATE')
        config=json.loads(offsite_config.read_text())
        export_snapshot(home,destination,Path(config['destination']),config['recipient'],checksums,offline)
    completed=sorted(p for p in parent.iterdir() if p.is_dir() and (p/'backup.json').exists())
    retained=set(completed[-7:])
    if protect in completed and protect not in retained:
        retained.remove(min(retained));retained.add(protect)
    for obsolete in completed:
        if obsolete not in retained: shutil.rmtree(obsolete)
    print('Backup:',destination)
    return destination

def restore(home,snapshot,offline=False):
    snapshot=snapshot.resolve()
    metadata=json.loads((snapshot/'backup.json').read_text())
    if checksums(snapshot)!=json.loads((snapshot/'checksums.json').read_text()): raise RuntimeError('BACKUP_CHECKSUM_MISMATCH')
    if metadata['databases']!=database_names(home): raise RuntimeError('BACKUP_DATABASE_SET_MISMATCH')
    target=home/'releases'/metadata['release']['id']
    if not (target/'release.json').is_file(): raise RuntimeError('Install the matching release before restoring')
    backup(home,offline,protect=snapshot,offsite=False)
    with paused(home,offline,resume_on_error=False):
        env,pg=postgres(home)
        for name in metadata['databases']:
            run([pg/'pg_restore','--clean','--if-exists','--exit-on-error','--single-transaction','--no-owner','--dbname',name,snapshot/'databases'/(name+'.dump')],env=env)
        incoming=home/'data/files.restore'
        if incoming.exists(): raise RuntimeError('Inspect existing files.restore first')
        shutil.copytree(snapshot/'files',incoming)
        old=home/'data/files.previous'
        if old.exists(): shutil.rmtree(old)
        (home/'data/files').rename(old);incoming.rename(home/'data/files')
        # Connection passwords belong to the existing cluster. Restore application
        # secrets (auth/AI) with data, while retaining its database login manifests.
        shutil.copy2(snapshot/'config/production.env',home/'config/production.env')
        if not offline:
            run(['/usr/sbin/chown','-R','_erpx:_erpx',home/'data/files'])
            run(['/usr/sbin/chown','root:_erpx',home/'config/production.env'])
            (home/'config/production.env').chmod(0o640)
        activate(home,target)
    print('Restored:',snapshot)

def activate(home,target):
    if target.parent!=home/'releases' or not (target/'server.js').exists(): raise ValueError('Invalid release')
    link=home/'current.new';link.unlink(missing_ok=True);link.symlink_to(target)
    os.chmod(link,0o755,follow_symlinks=False)
    link.replace(home/'current')

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--home',type=Path,default=DEFAULT);parser.add_argument('--offline-test',action='store_true')
    parser.add_argument('--destination',type=Path);parser.add_argument('--recipient');parser.add_argument('--identity',type=Path)
    parser.add_argument('command',choices=['backup','export','import','restore','rollback','doctor','bootstrap-admin','recovery-status']);parser.add_argument('target',nargs='?')
    args=parser.parse_args();home=args.home.resolve()
    if args.offline_test:
        if home==DEFAULT or any(not n.startswith('erpx_test_') for n in database_names(home)): raise RuntimeError('Offline mode requires isolated test databases')
    elif os.geteuid()!=0: raise SystemExit('Run this operator command with sudo.')
    os.umask(0o077)
    with (home/'operator.lock').open('w') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        if args.command=='backup': backup(home,args.offline_test)
        elif args.command=='export':
            if not args.target or not args.destination or not args.recipient: raise RuntimeError('OFFSITE_EXPORT_ARGUMENTS_REQUIRED')
            export_snapshot(home,Path(args.target),args.destination,args.recipient,checksums,args.offline_test)
        elif args.command=='import':
            if not args.target or not args.identity: raise RuntimeError('OFFSITE_IMPORT_ARGUMENTS_REQUIRED')
            import_archive(home,Path(args.target),args.identity,checksums)
        elif args.command=='restore': restore(home,Path(args.target),args.offline_test)
        elif args.command=='recovery-status': print(json.dumps(recovery_status(home),indent=2))
        elif args.command=='rollback':
            target=home/'releases'/str(args.target)
            if not re.fullmatch(r'[0-9TZ.-]+',str(args.target)): raise ValueError('Invalid release ID')
            # A release rollback never reverses schema. Use a backup restore for
            # incompatible schema changes; releases must declare equal schema IDs.
            if json.loads((target/'release.json').read_text())['schemaSha256']!=json.loads((home/'current/release.json').read_text())['schemaSha256']: raise RuntimeError('Schema differs; restore a matching backup instead')
            with paused(home,args.offline_test): activate(home,target)
        else:
            env={**os.environ,'ERPX_DATA_DIR':str(home/'data'),'ERPX_DATABASE_MANIFEST':str(home/'data/config/databases.json'),'NODE_ENV':'production'}
            run([home/'runtime/node/bin/node','--env-file='+str(home/'config/production.env'),home/'current/tools'/(args.command+'.mjs')],env=env,cwd=home/'current')
if __name__=='__main__': main()
