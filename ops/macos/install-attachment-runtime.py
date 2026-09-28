#!/usr/bin/env python3
"""Configure the pinned local-only malware scanner used by ERPX attachments.

Homebrew packages must already be installed. This installer never downloads or
upgrades packages; it creates a least-privilege service account, private paths,
Unix-socket-only configuration and two LaunchDaemons.
"""
import grp, json, os, plistlib, pwd, re, subprocess, time
from pathlib import Path

ROOT=Path('/Library/Application Support/ERPX-Attachments')
MANIFEST=Path(__file__).parent/'clamav'/'runtime-versions.json'
ACCOUNT='_erpx_scanner'
GROUP='_erpx_scanner'
CLAMD_LABEL='com.erpx.clamd'
FRESHCLAM_LABEL='com.erpx.freshclam'

def run(arguments,**kwargs):
    return subprocess.run([str(value) for value in arguments],check=True,**kwargs)

def assert_version(executable,expected,arguments=('--version',)):
    result=subprocess.run([str(executable),*arguments],check=True,capture_output=True,text=True)
    version=result.stdout+'\n'+result.stderr
    if not re.search(r'(?<!\d)'+re.escape(expected)+r'(?!\d)',version):
        raise SystemExit(f'{executable} does not match pinned version {expected}')

def service_account():
    try:
        user=pwd.getpwnam(ACCOUNT)
        if user.pw_shell!='/usr/bin/false': raise SystemExit('Existing scanner account has an unexpected shell')
        try: group=grp.getgrnam(GROUP)
        except KeyError: raise SystemExit('Existing scanner account has no matching scanner group')
        if user.pw_gid!=group.gr_gid: raise SystemExit('Existing scanner account has an unexpected primary group')
        return user.pw_uid,user.pw_gid
    except KeyError:
        used={item.pw_uid for item in pwd.getpwall()}|{item.gr_gid for item in grp.getgrall()}
        uid=next(value for value in range(500,600) if value not in used)
        run(['/usr/bin/dscl','.', '-create',f'/Groups/{GROUP}'])
        run(['/usr/bin/dscl','.', '-create',f'/Groups/{GROUP}','PrimaryGroupID',str(uid)])
        run(['/usr/bin/dscl','.', '-create',f'/Users/{ACCOUNT}'])
        for key,value in {'UniqueID':uid,'PrimaryGroupID':uid,'UserShell':'/usr/bin/false','NFSHomeDirectory':ROOT,'RealName':'ERPX attachment scanner','IsHidden':1,'Password':'*'}.items():
            run(['/usr/bin/dscl','.', '-create',f'/Users/{ACCOUNT}',key,str(value)])
        return uid,uid

def plist(label,arguments,log,keep_alive=False,start_interval=None):
    data={'Label':label,'ProgramArguments':[str(value) for value in arguments],
          'UserName':ACCOUNT,'GroupName':GROUP,'RunAtLoad':True,'Umask':0o077,
          'StandardOutPath':str(log),'StandardErrorPath':str(log),'ProcessType':'Background'}
    if keep_alive: data.update(KeepAlive=True,ThrottleInterval=10)
    if start_interval: data['StartInterval']=start_interval
    destination=Path('/Library/LaunchDaemons')/(label+'.plist')
    destination.write_bytes(plistlib.dumps(data));destination.chmod(0o644)
    return destination

def main():
    if os.geteuid()!=0: raise SystemExit('Run with sudo after installing the pinned Homebrew packages.')
    manifest=json.loads(MANIFEST.read_text())
    clamd=Path(manifest['clamav']['clamd']);clamdscan=Path(manifest['clamav']['clamdscan']);freshclam=Path(manifest['clamav']['freshclam'])
    heif=Path(manifest['libheif']['heifConvert']);pdftoppm=Path(manifest['poppler']['pdftoppm'])
    for executable in (clamd,clamdscan,freshclam,heif,pdftoppm):
        if not executable.is_file() or not os.access(executable,os.X_OK): raise SystemExit(f'Missing pinned executable: {executable}')
    assert_version(heif,manifest['libheif']['version'])
    assert_version(pdftoppm,manifest['poppler']['version'],('-v',))
    uid,gid=service_account();ROOT.mkdir(mode=0o750,parents=True,exist_ok=True);ROOT.chmod(0o750);os.chown(ROOT,0,gid)
    paths={name:ROOT/name for name in ('db','run','log')}
    for path in paths.values(): path.mkdir(mode=0o750,exist_ok=True);path.chmod(0o750);os.chown(path,uid,gid)
    clamd_conf=ROOT/'clamd.conf';freshclam_conf=ROOT/'freshclam.conf'
    clamd_conf.write_text(f'''LogFile {paths['log']/"clamd.log"}\nLogTime yes\nLogRotate yes\nPidFile {paths['run']/"clamd.pid"}\nDatabaseDirectory {paths['db']}\nOfficialDatabaseOnly yes\nFailIfCvdOlderThan 1\nLocalSocket {paths['run']/"clamd.sock"}\nLocalSocketGroup {GROUP}\nLocalSocketMode 660\nFixStaleSocket yes\nStreamMaxLength 11M\nMaxScanSize 20M\nMaxFileSize 11M\nMaxThreads 4\nReadTimeout 30\nCommandReadTimeout 5\nSendBufTimeout 30\nForeground yes\n''')
    freshclam_conf.write_text(f'''DatabaseDirectory {paths['db']}\nDatabaseMirror database.clamav.net\nMaxAttempts 3\nChecks 24\nForeground yes\nUpdateLogFile {paths['log']/"freshclam.log"}\nLogTime yes\nLogRotate yes\n''')
    for path in (clamd_conf,freshclam_conf): path.chmod(0o640);os.chown(path,0,gid)
    # Homebrew's clamd/freshclam version flags still load their default config.
    # Validate against the generated private configs instead of failing before
    # those files exist or accidentally accepting another global installation.
    assert_version(freshclam,manifest['clamav']['version'],('--config-file='+str(freshclam_conf),'--version'))
    # Application service accounts receive only Unix-socket group access.
    for account in ('_erpx','_erpx_staging'):
        try: pwd.getpwnam(account)
        except KeyError: continue
        run(['/usr/sbin/dseditgroup','-o','edit','-a',account,'-t','user',GROUP])
    # A reviewed rerun replaces both services rather than racing an existing
    # FreshClam update or leaving clamd on an older executable/configuration.
    for label in (CLAMD_LABEL,FRESHCLAM_LABEL):
        subprocess.run(['/bin/launchctl','bootout','system/'+label],capture_output=True)
    # Seed signatures before clamd starts; FreshClam then refreshes hourly.
    run(['/usr/bin/sudo','-u',ACCOUNT,freshclam,'--config-file='+str(freshclam_conf)])
    assert_version(clamd,manifest['clamav']['version'],('--config-file='+str(clamd_conf),'--version'))
    assert_version(clamdscan,manifest['clamav']['version'],('--config-file='+str(clamd_conf),'--version'))
    definitions=[
        plist(CLAMD_LABEL,[clamd,'--config-file='+str(clamd_conf)],paths['log']/'clamd-launcher.log',keep_alive=True),
        plist(FRESHCLAM_LABEL,[freshclam,'--config-file='+str(freshclam_conf)],paths['log']/'freshclam-launcher.log',start_interval=3600),
    ]
    for label,path in zip((CLAMD_LABEL,FRESHCLAM_LABEL),definitions):
        run(['/bin/launchctl','bootstrap','system',path])
    for _ in range(60):
        ping=subprocess.run([str(clamdscan),'--config-file='+str(clamd_conf),'--ping','1'],capture_output=True)
        if ping.returncode==0: break
        time.sleep(.5)
    else: raise SystemExit('clamd did not expose the configured Unix socket within 30 seconds')
    print(json.dumps({'ERPX_CLAMD_SOCKET':str(paths['run']/"clamd.sock"),'ERPX_CLAMAV_DATABASE_DIR':str(paths['db']),'ERPX_CLAMD_CONF':str(clamd_conf),'ERPX_FRESHCLAM_CONF':str(freshclam_conf),'ERPX_HEIF_CONVERT':str(heif),'ERPX_PDFTOPPM_BIN':str(pdftoppm)},indent=2))

if __name__=='__main__': main()
