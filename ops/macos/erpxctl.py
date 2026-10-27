#!/usr/bin/env python3
"""Operator CLI. Backups contain credentials and stay root-private on this Mac."""
import argparse, contextlib, datetime, fcntl, hashlib, json, os, re, shutil, subprocess, sys, time
from pathlib import Path
from urllib.parse import urlparse, unquote
from offsite import export_snapshot, import_archive
from recovery_status import recovery_status
DEFAULT=Path('/Library/Application Support/ERPX')
STAGING=Path('/Library/Application Support/ERPX-Staging')

def instance(home):
    if home==DEFAULT: return 'com.erpx.', '_erpx', 'production.env'
    if home==STAGING: return 'com.erpx.staging.', '_erpx_staging', 'staging.env'
    raise RuntimeError('Unrecognized installed ERPX home')

def run(args, **kwargs):
    return subprocess.run([str(x) for x in args], check=True, **kwargs)

def services(home, action):
    prefix,_,_=instance(home)
    for role in (['web','jobs'] if action=='stop' else ['jobs','web']):
        label=prefix+role
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

def database_names(home, offline=False):
    manifest=json.loads((home/'data/config/databases.json').read_text())
    names=sorted({unquote(urlparse(u).path.removeprefix('/')) for u in manifest['databases'].values()})
    prefix='erpx_test_' if offline else 'erpx_stage_' if home==STAGING else 'erpx_'
    if not names or any(not re.fullmatch(r'erpx(?:_[a-z0-9]+)+',n) or not n.startswith(prefix) or (prefix=='erpx_' and n.startswith(('erpx_dev_','erpx_stage_','erpx_test_'))) for n in names): raise ValueError('Invalid ERPX database manifest')
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
        names=database_names(home,offline)
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

def minio_configured(home,offline=False):
    _,_,env_file=instance(home) if not offline else ('','','production.env')
    config=home/'config'/env_file
    # Read only whether an object-store setting is present. Credentials are
    # loaded by the pinned Node runtime, never passed on the command line.
    for line in config.read_text().splitlines():
        match=re.match(r'^\s*(?:export\s+)?(ERPX_MINIO_[A-Z_]+)\s*=\s*(.*)$',line)
        if match and match.group(1) in ('ERPX_MINIO_ENDPOINT','ERPX_MINIO_BUCKET','ERPX_MINIO_ACCESS_KEY','ERPX_MINIO_SECRET_KEY') and match.group(2).strip() not in ('','""',"''"):
            return True
    return False

def minio_snapshot(home,action,snapshot,offline=False):
    _,_,env_file=instance(home) if not offline else ('','','production.env')
    tool=home/'current/tools/minio-snapshot.mjs'
    if not tool.is_file(): raise RuntimeError('MINIO_SNAPSHOT_TOOL_REQUIRED')
    environment='local' if offline else 'staging' if home==STAGING else 'production'
    env={key:value for key,value in os.environ.items() if key!='ERPX_ENVIRONMENT' and not key.startswith('ERPX_MINIO_')}
    env['NODE_ENV']='development' if offline else 'production'
    run([home/'runtime/node/bin/node','--env-file='+str(home/'config'/env_file),tool,action,snapshot,environment],env=env,cwd=home/'current')

def minio_descriptor(snapshot):
    manifest=json.loads((snapshot/'minio/manifest.json').read_text())
    return {'type':'minio','bucket':manifest['bucket'],'prefix':manifest['prefix'],'objects':len(manifest['objects'])}

def migrate_files(home,mode,expected_count=None,offline=False):
    if mode not in ('plan','apply','verify'): raise RuntimeError('MINIO_MIGRATION_MODE_REQUIRED')
    if mode!='plan' and (expected_count is None or expected_count<0): raise RuntimeError('MINIO_MIGRATION_EXPECT_COUNT_REQUIRED')
    if not minio_configured(home,offline): raise RuntimeError('ERPX_MINIO_CONFIG_INCOMPLETE')
    _,_,env_file=instance(home) if not offline else ('','','production.env')
    environment='local' if offline else 'staging' if home==STAGING else 'production'
    tool=home/'current/tools/migrate-files-to-minio.mjs'
    if not tool.is_file(): raise RuntimeError('MINIO_MIGRATION_TOOL_REQUIRED')
    env={key:value for key,value in os.environ.items() if key!='ERPX_ENVIRONMENT' and not key.startswith('ERPX_MINIO_')}
    env.update({'NODE_ENV':'development' if offline else 'production','ERPX_ENVIRONMENT':environment,
                'ERPX_DATA_DIR':str(home/'data'),'ERPX_DATABASE_MANIFEST':str(home/'data/config/databases.json')})
    args=[home/'runtime/node/bin/node','--env-file='+str(home/'config'/env_file),tool,'--'+mode,
          '--data-dir',home/'data','--environment',environment]
    if expected_count is not None: args.extend(['--expect-count',str(expected_count)])
    with paused(home,offline): run(args,env=env,cwd=home/'current')

def backup(home, offline=False, protect=None, offsite=True):
    parent=home/'backups';parent.mkdir(mode=0o700,exist_ok=True)
    stamp=datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S.%fZ')
    temporary=parent/('.'+stamp+'.partial');temporary.mkdir(mode=0o700)
    with paused(home,offline):
        env,pg=postgres(home);names=database_names(home,offline)
        (temporary/'databases').mkdir()
        for name in names:
            run([pg/'pg_dump','--format=custom','--file',temporary/'databases'/(name+'.dump'),'--dbname',name],env=env)
        shutil.copytree(home/'data/files',temporary/'files')
        shutil.copytree(home/'data/config',temporary/'data-config')
        shutil.copytree(home/'config',temporary/'config')
        object_store=None
        if minio_configured(home,offline):
            minio_snapshot(home,'backup',temporary,offline)
            object_store=minio_descriptor(temporary)
        release=json.loads((home/'current/release.json').read_text())
        metadata={'version':2 if object_store else 1,'createdAt':stamp,'release':release,'databases':names}
        if object_store: metadata['objectStore']=object_store
        (temporary/'backup.json').write_text(json.dumps(metadata,indent=2))
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
    if metadata['databases']!=database_names(home,offline): raise RuntimeError('BACKUP_DATABASE_SET_MISMATCH')
    has_minio='objectStore' in metadata
    if has_minio!=minio_configured(home,offline): raise RuntimeError('BACKUP_OBJECT_STORE_CONFIG_MISMATCH')
    if has_minio:
        if metadata['objectStore']!=minio_descriptor(snapshot): raise RuntimeError('BACKUP_OBJECT_STORE_SCOPE_MISMATCH')
        minio_snapshot(home,'verify',snapshot,offline)
    target=home/'releases'/metadata['release']['id']
    if not (target/'release.json').is_file(): raise RuntimeError('Install the matching release before restoring')
    backup(home,offline,protect=snapshot,offsite=False)
    with paused(home,offline,resume_on_error=False):
        if has_minio: minio_snapshot(home,'restore',snapshot,offline)
        env,pg=postgres(home)
        for name in metadata['databases']:
            # The complete catalogue has thousands of tables, indexes and
            # triggers. Holding every DROP+CREATE lock until one final commit
            # exhausts PostgreSQL's lock table on the shared local cluster.
            # PG18 bounds locks per batch; maintenance remains active if any
            # batch fails, before files or the matching release are activated.
            # A full, checksum-verified snapshot is restored while services are
            # paused, after a protected backup of the current state. pg_restore
            # --clean cannot remove an inherited child PK before its parent.
            # Remove partition roots (and their FK/index dependencies) first;
            # the complete archive recreates and validates those dependencies.
            # Never ignore restore errors or omit constraints from the archive.
            run([pg/'psql','-X','--set','ON_ERROR_STOP=1','--dbname',name,'--command',
                 """DO $$ DECLARE parent record; BEGIN
                 FOR parent IN SELECT c.relname,n.nspname FROM pg_class c
                   JOIN pg_namespace n ON n.oid=c.relnamespace
                   WHERE c.relkind='p' AND NOT c.relispartition AND n.nspname='public'
                   ORDER BY c.relname LOOP
                   EXECUTE format('DROP TABLE %I.%I CASCADE',parent.nspname,parent.relname);
                 END LOOP; END $$;"""],env=env)
            options=['--clean','--if-exists','--exit-on-error','--transaction-size=64']
            if offline: options.append('--no-owner')
            run([pg/'pg_restore',*options,'--dbname',name,snapshot/'databases'/(name+'.dump')],env=env)
        incoming=home/'data/files.restore'
        if incoming.exists(): raise RuntimeError('Inspect existing files.restore first')
        shutil.copytree(snapshot/'files',incoming)
        old=home/'data/files.previous'
        if old.exists(): shutil.rmtree(old)
        (home/'data/files').rename(old);incoming.rename(home/'data/files')
        # Connection passwords belong to the existing cluster. Restore application
        # secrets (auth/AI) with data, while retaining its database login manifests.
        _,account,env_file=instance(home) if not offline else ('','_erpx','production.env')
        shutil.copy2(snapshot/'config'/env_file,home/'config'/env_file)
        if not offline:
            run(['/usr/sbin/chown','-R',account+':'+account,home/'data/files'])
            run(['/usr/sbin/chown','root:'+account,home/'config'/env_file])
            (home/'config'/env_file).chmod(0o640)
        activate(home,target)
    print('Restored:',snapshot)

def activate(home,target):
    if target.parent!=home/'releases' or not (target/'server.js').exists(): raise ValueError('Invalid release')
    link=home/'current.new';link.unlink(missing_ok=True);link.symlink_to(target)
    # Linux symlink modes are kernel-managed; never follow the link to chmod
    # its release target. macOS supports the original no-follow operation.
    if os.chmod in os.supports_follow_symlinks:
        os.chmod(link,0o755,follow_symlinks=False)
    link.replace(home/'current')

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--home',type=Path,default=DEFAULT);parser.add_argument('--offline-test',action='store_true')
    parser.add_argument('--destination',type=Path);parser.add_argument('--recipient');parser.add_argument('--identity',type=Path)
    parser.add_argument('--expect-count',type=int)
    parser.add_argument('command',choices=['backup','export','import','restore','rollback','doctor','bootstrap-admin','recovery-status','migrate-files']);parser.add_argument('target',nargs='?')
    args=parser.parse_args();home=args.home.resolve()
    if args.offline_test:
        if home in (DEFAULT,STAGING) or any(not n.startswith('erpx_test_') for n in database_names(home,True)): raise RuntimeError('Offline mode requires isolated test databases')
    elif os.geteuid()!=0: raise SystemExit('Run this operator command with sudo.')
    elif home not in (DEFAULT,STAGING): raise SystemExit('Unrecognized installed ERPX home')
    os.umask(0o077)
    with (home/'operator.lock').open('w') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        if args.command=='backup': backup(home,args.offline_test)
        elif args.command=='migrate-files': migrate_files(home,args.target,args.expect_count,args.offline_test)
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
            _,_,env_file=instance(home) if not args.offline_test else ('','','production.env')
            env={**os.environ,'ERPX_DATA_DIR':str(home/'data'),'ERPX_DATABASE_MANIFEST':str(home/'data/config/databases.json'),'NODE_ENV':'production'}
            run([home/'runtime/node/bin/node','--env-file='+str(home/'config'/env_file),home/'current/tools'/(args.command+'.mjs')],env=env,cwd=home/'current')
if __name__=='__main__': main()
