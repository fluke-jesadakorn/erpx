#!/usr/bin/env python3
"""Copy pinned runtimes and relocate Homebrew dylibs into a self-contained payload."""
import os, shutil, subprocess, sys
from pathlib import Path
root = Path(__file__).resolve().parents[2]
target = root / '.runtime/deployment/runtime'
node = root / '.runtime/node/node-v24.20.0-darwin-arm64'
pg = Path('/opt/homebrew/opt/postgresql@18').resolve()
if subprocess.check_output([str(node/'bin/node'), '--version'], text=True).strip() != 'v24.20.0':
    raise SystemExit('Node version mismatch')
if subprocess.check_output([str(pg/'bin/postgres'), '--version'], text=True).strip() not in ['postgres (PostgreSQL) 18.6','postgres (PostgreSQL) 18.6 (Homebrew)']:
    raise SystemExit('PostgreSQL version mismatch')
target.mkdir(parents=True, exist_ok=True)
shutil.copytree(node, target/'node', dirs_exist_ok=True)
pgout=target/'postgres'
if pgout.exists(): shutil.rmtree(pgout)
for name in ['bin','lib','share']:
    shutil.copytree(pg/name, pgout/name, dirs_exist_ok=True, symlinks=False)
(target/'cloudflared').mkdir(exist_ok=True)
(target/'cloudflared/cloudflared').unlink(missing_ok=True)
shutil.copy2('/opt/homebrew/bin/cloudflared', target/'cloudflared/cloudflared')
# Relocate every Mach-O binary, including dynamically loaded PostgreSQL extensions.
queue=[p for p in pgout.rglob('*') if p.is_file()]
sources={p:pg/p.relative_to(pgout) for p in queue}
seen=set()
while queue:
    path=queue.pop()
    if path in seen: continue
    seen.add(path)
    result=subprocess.run(['/usr/bin/otool','-L',str(path)],capture_output=True,text=True)
    if result.returncode: continue
    changes=[]
    for line in result.stdout.splitlines()[1:]:
        dep=line.strip().split(' (compatibility')[0]
        if dep.startswith('/opt/homebrew/'):
            source=Path(dep).resolve()
        elif dep.startswith('@loader_path/'):
            source=(sources[path].parent/dep.removeprefix('@loader_path/')).resolve()
        elif dep.startswith('@rpath/'):
            source=(sources[path].parent/Path(dep).name).resolve()
        else: continue
        if not source.exists(): raise RuntimeError('Missing runtime dependency: '+str(source))
        dest=pgout/'lib'/source.name
        if not dest.exists():
            shutil.copy2(source,dest)
            sources[dest]=source
            queue.append(dest)
        relocated='@loader_path/'+os.path.relpath(dest,path.parent)
        changes.extend(['-change',dep,relocated])
    if changes:
        path.chmod(path.stat().st_mode | 0o200)
        subprocess.run(['/usr/bin/install_name_tool',*changes,str(path)],check=True,capture_output=True)
        subprocess.run(['/usr/bin/codesign','--force','--sign','-',str(path)],check=True,capture_output=True)
for executable in ['postgres','initdb','pg_dump','pg_restore','psql']:
    subprocess.run([str(pgout/'bin'/executable),'--version'],check=True)
print('Runtime payload:',target)
