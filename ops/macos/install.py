#!/usr/bin/env python3
"""Legacy isolated-cluster installer (5433); not for the shared 5432 deployment."""
import argparse, grp, json, os, plistlib, pwd, secrets, shutil, socket, subprocess, time
from pathlib import Path
from urllib.parse import quote
BASE=Path('/Library/Application Support/ERPX')

def run(args,**kwargs):
    return subprocess.run([str(x) for x in args],check=True,**kwargs)
def free(port):
    with socket.socket() as sock:
        try: sock.bind(('127.0.0.1',port))
        except OSError: raise SystemExit(f'Port {port} is occupied; no process was stopped.')
def public_link(path):
    # macOS enforces permissions on symlinks; the secret-safe umask is 077.
    os.chmod(path,0o755,follow_symlinks=False)
def loaded(role):
    return subprocess.run(['launchctl','print','system/com.erpx.'+role],capture_output=True).returncode==0
def daemon(role,args,calendar=None):
    label='com.erpx.'+role
    data={'Label':label,'ProgramArguments':[str(x) for x in args], 'WorkingDirectory':str(BASE/'current'),
          'EnvironmentVariables':{'ERPX_HOME':str(BASE),'PATH':'/usr/bin:/bin:/usr/sbin:/sbin','LC_ALL':'C','LANG':'C'},'Umask':0o077,
          'StandardOutPath':str(BASE/'logs'/f'{role}-launcher.log'),'StandardErrorPath':str(BASE/'logs'/f'{role}-launcher.log')}
    if calendar:
        data['StartCalendarInterval']=calendar
    else:
        data.update(UserName='_erpx',GroupName='_erpx',RunAtLoad=True,KeepAlive=True,ThrottleInterval=10,ExitTimeOut=120,ProcessType='Background')
    path=Path('/Library/LaunchDaemons')/(label+'.plist')
    path.write_bytes(plistlib.dumps(data));path.chmod(0o644)
    return path

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--release',required=True,type=Path);parser.add_argument('--payload',required=True,type=Path)
    parser.add_argument('--resume',action='store_true',help='Resume an inspected initial install before web/jobs/Tunnel were registered')
    args=parser.parse_args();release=args.release.resolve();payload=args.payload.resolve()
    if os.geteuid()!=0: raise SystemExit('Run this installer with sudo; it creates four dedicated ERPX services and a daily backup service.')
    os.umask(0o077)
    metadata=json.loads((release/'release.json').read_text())
    resuming=args.resume and (BASE/'current').is_symlink()
    runtime_root=BASE if resuming else payload
    config_root=BASE/'config' if resuming else payload
    if not (runtime_root/'runtime/postgres/bin/postgres').exists() or not (config_root/'production.env').exists(): raise SystemExit('Incomplete payload')
    if not (config_root/'tunnel.token').is_file(): raise SystemExit('Tunnel token missing')
    if resuming:
        if (BASE/'current').resolve()!=BASE/'releases'/metadata['id']: raise SystemExit('Resume release differs from installed release')
        if any(loaded(role) for role in ['web','jobs','tunnel','backup']): raise SystemExit('Initial install already registered application services; inspect before using the upgrade procedure')
        if json.loads((BASE/'current/release.json').read_text())!=metadata: raise SystemExit('Installed release metadata differs')
        free(3101)
        if not loaded('postgres'): free(5433)
    else:
        if args.resume: raise SystemExit('No initial installation to resume')
        if (BASE/'current').exists(): raise SystemExit('ERPX is already installed. Use the documented upgrade procedure; installer did not modify it.')
        free(3101);free(5433)
    try:
        account=pwd.getpwnam('_erpx')
        if account.pw_dir!=str(BASE/'data') or account.pw_shell!='/usr/bin/false': raise SystemExit('Existing _erpx account has unexpected settings')
        uid=account.pw_uid;gid=account.pw_gid
    except KeyError:
        used={u.pw_uid for u in pwd.getpwall()}|{g.gr_gid for g in grp.getgrall()}
        uid=next(i for i in range(400,500) if i not in used);gid=uid
        for record,attributes in [('/Groups/_erpx',{'PrimaryGroupID':str(gid),'RealName':'ERPX service'}),('/Users/_erpx',{'UniqueID':str(uid),'PrimaryGroupID':str(gid),'UserShell':'/usr/bin/false','NFSHomeDirectory':str(BASE/'data'),'RealName':'ERPX service','IsHidden':'1','Password':'*'})]:
            run(['dscl','.', '-create',record])
            for key,value in attributes.items(): run(['dscl','.', '-create',record,key,value])
    BASE.mkdir(mode=0o755,parents=True,exist_ok=True);BASE.chmod(0o755)
    for name in ['releases','runtime','ops']:
        (BASE/name).mkdir(mode=0o755,exist_ok=True)
        # The private installer umask must not hide immutable code from _erpx.
        (BASE/name).chmod(0o755)
    for name in ['config','backups','data','logs']:
        (BASE/name).mkdir(mode=0o700,exist_ok=True)
    os.chown(BASE/'data',uid,gid);os.chown(BASE/'logs',uid,gid)
    (BASE/'config').chmod(0o750);os.chown(BASE/'config',0,gid)
    destination=BASE/'releases'/metadata['id']
    if not resuming:
        shutil.copytree(payload/'runtime',BASE/'runtime',dirs_exist_ok=True)
        shutil.copytree(release,destination)
    shutil.copytree(Path(__file__).parent,BASE/'ops',dirs_exist_ok=True)
    for tree in ['runtime','releases','ops']:
        for p in (BASE/tree).rglob('*'):
            if p.is_symlink(): continue
            os.chown(p,0,0);p.chmod(0o755 if p.is_dir() or p.stat().st_mode & 0o111 else 0o644)
    if not resuming: (BASE/'current').symlink_to(destination)
    public_link(BASE/'current')
    if not resuming:
        for name in ['production.env','tunnel.token']:
            shutil.copy2(payload/name,BASE/'config'/name);os.chown(BASE/'config'/name,0,gid);(BASE/'config'/name).chmod(0o640)
    node=BASE/'runtime/node/bin/node';pg=BASE/'runtime/postgres/bin';data=BASE/'data/postgres'
    adminfile=BASE/'config/admin.json'
    if adminfile.exists(): admin=json.loads(adminfile.read_text())
    else:
        admin={'host':'127.0.0.1','port':5433,'user':'erpx_admin','password':secrets.token_hex(32)}
        adminfile.write_text(json.dumps(admin));adminfile.chmod(0o600)
    if not (data/'PG_VERSION').exists():
        password=BASE/'data/init-password';password.write_text(admin['password']);os.chown(password,uid,gid);password.chmod(0o600)
        try: run(['sudo','-u','_erpx',pg/'initdb','-D',data,'-U','erpx_admin','--pwfile',password,'--auth-local=scram-sha-256','--auth-host=scram-sha-256','--encoding=UTF8','--locale=C'])
        finally: password.unlink(missing_ok=True)
        with (data/'postgresql.conf').open('a') as f:
            f.write("\nlisten_addresses='127.0.0.1'\nport=5433\nunix_socket_directories=''\nmax_connections=160\nshared_buffers='128MB'\npassword_encryption='scram-sha-256'\nlogging_collector=on\nlog_directory='../postgres-logs'\nlog_filename='postgresql-%a.log'\nlog_rotation_age='1d'\nlog_rotation_size=0\nlog_truncate_on_rotation=on\nlog_min_messages=warning\nlog_min_error_statement=panic\nlog_statement='none'\nlog_parameter_max_length_on_error=0\n")
    pgplist=daemon('postgres',[pg/'postgres','-D',data])
    # A resumed initial installation must reload the corrected daemon environment.
    if loaded('postgres'): run(['launchctl','bootout','system/com.erpx.postgres'])
    run(['launchctl','bootstrap','system',pgplist])
    for _ in range(60):
        if subprocess.run([str(pg/'pg_isready'),'-h','127.0.0.1','-p','5433'],capture_output=True).returncode==0: break
        time.sleep(1)
    else:
        for log in sorted((BASE/'data/postgres-logs').glob('*.log'),key=lambda p:p.stat().st_mtime)[-1:]:
            print('\n'.join(log.read_text().splitlines()[-12:]))
        raise RuntimeError('PostgreSQL startup failed; inspect ERPX logs')
    env={**os.environ,'ERPX_ADMIN_DATABASE_URL':f"postgresql://{admin['user']}:{quote(admin['password'])}@127.0.0.1:5433/postgres",'ERPX_DATA_DIR':str(BASE/'data'),'ERPX_DATABASE_PREFIX':'erpx'}
    run([node,destination/'tools/provision.mjs'],cwd=destination,env=env)
    run(['chown','-R','_erpx:_erpx',BASE/'data'])
    # The code remains immutable to the service user; Next's image cache is data.
    cache=BASE/'data/next-cache';cache.mkdir(exist_ok=True);os.chown(cache,uid,gid)
    release_cache=destination/'.next/cache'
    if release_cache.is_symlink(): release_cache.unlink()
    elif release_cache.exists(): shutil.rmtree(release_cache)
    release_cache.symlink_to(cache)
    public_link(release_cache)
    definitions=[daemon('jobs',[node,BASE/'current/tools/launch.mjs','jobs']),daemon('web',[node,BASE/'current/tools/launch.mjs','web']),
      daemon('tunnel',[node,BASE/'current/tools/launch.mjs','tunnel']),
      daemon('backup',['/usr/bin/python3',BASE/'ops/erpxctl.py','backup'],{'Hour':3,'Minute':15})]
    for path in definitions: run(['launchctl','bootstrap','system',path])
    # Rotate launcher/tunnel logs; PostgreSQL and Node have their own bounded logs.
    newsyslog=Path('/etc/newsyslog.d/com.erpx.conf')
    newsyslog.write_text('"'+str(BASE/'logs/*-launcher.log')+'" _erpx:_erpx 600 7 5120 * GJN\n"'+str(BASE/'logs/tunnel.log')+'" _erpx:_erpx 600 7 5120 * GJN\n');newsyslog.chmod(0o644)
    print('Installed ERPX. Public registration remains controlled by config/production.env.')
if __name__=='__main__': main()
