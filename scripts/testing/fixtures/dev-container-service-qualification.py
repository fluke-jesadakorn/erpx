#!/usr/bin/env python3
"""Fresh, keyed contributor qualification. Execute only against the owned case.

Inputs describe a frozen tracked base and explicit, hashed candidate overlay. No
operator environment or checkout is mounted into the workspace container.
"""
from __future__ import annotations
import argparse, base64, hashlib, io, json, os, re, selectors, socket, stat, subprocess, tarfile, tempfile, time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
SOCKET=Path.home()/'.colima/erpx-opensource-dev/docker.sock'

def digest(data:bytes)->str:return hashlib.sha256(data).hexdigest()
def source(path:Path)->bytes:
 info=path.lstat()
 if not stat.S_ISREG(info.st_mode) or stat.S_ISLNK(info.st_mode):raise RuntimeError('REGULAR_SOURCE_REQUIRED')
 return path.read_bytes()
def allowed(value:str)->bool:
 parts=Path(value).parts
 return bool(parts) and not Path(value).is_absolute() and all(x not in ('..','.git','.scratch','.runtime','node_modules','target','.next') and not x.startswith('.env') for x in parts)
def private(path:Path,data:bytes):
 fd=os.open(path,os.O_CREAT|os.O_EXCL|os.O_WRONLY,0o600)
 with os.fdopen(fd,'wb') as out:out.write(data)

class Case:
 def __init__(self,plan,resume=False):
  self.plan=plan;self.key=plan['workspaceKey'];self.project='erpx-dev-'+self.key
  if not re.fullmatch(r'[a-z0-9]+(?:-[a-z0-9]+)*',self.key):raise RuntimeError('EXACT_KEY_REQUIRED')
  self.evidence=Path(plan['evidenceDirectory']).resolve();self.evidence.mkdir(mode=0o700,parents=True,exist_ok=resume);self.evidence.chmod(0o700)
  self.env={key:value for key,value in os.environ.items() if not key.startswith('ERPX_') and key not in ('DATABASE_URL','BETTER_AUTH_SECRET','DOCKER_CONTEXT','DOCKER_TLS_VERIFY','DOCKER_CERT_PATH')};self.env.update(DOCKER_HOST='unix://'+str(SOCKET),ERPX_DEV_WORKSPACE_KEY=self.key,ERPX_DEV_SOURCE_REF=plan['baseRef'],COMPOSE_DISABLE_ENV_FILE='1')
  self.workspace=None;self.overlay={};self.baseline={};self.report=json.loads((self.evidence/'result.json').read_text()) if resume else {'key':self.key,'baseRef':plan['baseRef'],'steps':[]}
  if self.report['key']!=self.key or self.report['baseRef']!=plan['baseRef']:raise RuntimeError('RESUME_IDENTITY_CHANGED')
  for entry in plan['overlay']:
   name=entry['path'];expected=entry['sha256']
   if not allowed(name) or name in self.overlay:raise RuntimeError('INVALID_OVERLAY_PATH')
   data=source(ROOT/name)
   if digest(data)!=expected:raise RuntimeError('OVERLAY_SOURCE_CHANGED')
   self.overlay[name]=data;self.baseline[name]=expected
  required={'scripts/testing/fixtures/dev-container-service-qualification.py','scripts/testing/fixtures/dev-container-service-qualification.mjs','scripts/testing/fixtures/dev-container-service-qualification-db.mjs'}
  if not required.issubset(self.overlay) or not re.fullmatch(r'[0-9a-f]{40}',plan['baseRef']):raise RuntimeError('FROZEN_BASE_AND_OVERLAY_REQUIRED')
 def run(self,args,*,data=None,env=None,timeout=300,log=None):
  start=time.monotonic();r=subprocess.run(args,input=data,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,env=env or self.env,cwd=ROOT,timeout=timeout)
  if log:private(self.evidence/log,r.stdout)
  if r.returncode:raise RuntimeError('COMMAND_FAILED: '+args[0]+' '+(args[1] if len(args)>1 else ''))
  return r.stdout,round(time.monotonic()-start,3)
 def docker(self,*args,data=None,timeout=300):return self.run(['docker',*args],data=data,timeout=timeout)[0]
 def guest(self,*args,data=None,timeout=300):
  if not self.workspace:raise RuntimeError('WORKSPACE_NOT_RESOLVED')
  return self.docker('exec','-i',self.workspace,*args,data=data,timeout=timeout)
 def cli(self,*args,log=None,timeout=600):return self.run([str(ROOT/'scripts/local/dev-container'),*args],log=log,timeout=timeout)
 def resolve_workspace(self):
  found=self.docker('ps','--filter','label=com.docker.compose.project='+self.project,'--filter','label=com.docker.compose.service=workspace','--format','{{.ID}}').decode().splitlines()
  if len(found)!=1:raise RuntimeError('ONE_OWNED_WORKSPACE_REQUIRED')
  self.workspace=found[0];info=json.loads(self.docker('inspect',self.workspace).decode())[0];mount=next((x for x in info['Mounts'] if x['Destination']=='/workspace/erpx'),None)
  if not mount or mount['Type']!='volume' or mount['Name']!=self.project+'-source':raise RuntimeError('KEYED_SOURCE_VOLUME_REQUIRED')
  target=next((x for x in info['Mounts'] if x['Destination']=='/workspace/erpx/target'),None)
  if not target or target['Type']!='volume' or target['Name']!=self.project+'-rust1.99.1-target':raise RuntimeError('KEYED_RUST_CACHE_REQUIRED')
  identities={}
  for key,entry in [('source',mount),('cargo',target)]:
   volume=json.loads(self.docker('volume','inspect',entry['Name']).decode())[0]
   if volume.get('Labels',{}).get('com.docker.compose.project')!=self.project:raise RuntimeError('VOLUME_OWNER_MISMATCH')
   identities[key]={'name':entry['Name'],'createdAt':volume['CreatedAt']}
  if 'volumeIdentities' in self.report and self.report['volumeIdentities']!=identities:raise RuntimeError('WORKSPACE_VOLUME_CHANGED')
  self.report['volumeIdentities']=identities;self.report['workspaceImage']=info['Image'];self.report['sourceVolume']=mount['Name'];self.report['cargoTargetVolume']=target['Name'];self.report['rustc']=self.guest('rustc','--version').decode().strip()
 def check_frozen(self):
  if any(digest(source(ROOT/name))!=value for name,value in self.baseline.items()):raise RuntimeError('HOST_OVERLAY_CHANGED')
  head=self.guest('git','-C','/workspace/erpx','rev-parse','HEAD').decode().strip()
  if head!=self.plan['baseRef']:raise RuntimeError('GUEST_BASE_MISMATCH')
  for name,value in self.baseline.items():
   actual=self.guest('sha256sum','/workspace/erpx/'+name).decode().split()[0]
   if actual!=value:raise RuntimeError('GUEST_OVERLAY_MISMATCH')
 def populate(self):
  probe=socket.socket();
  try:probe.bind(('127.0.0.1',3443))
  except OSError as error:raise RuntimeError('LOCAL_HTTPS_PORT_OCCUPIED') from error
  finally:probe.close()
  existing=self.docker('ps','-a','--filter','label=com.docker.compose.project='+self.project,'--format','{{.ID}}').decode().splitlines()
  if existing:raise RuntimeError('FRESH_KEY_REQUIRED')
  suffixes=['source','postgres','scanner-socket','scanner-definitions','local-ca','node26.11.1-bun1.4.3-modules','rust1.99.1-target','rust1.99.1-registry','rust1.99.1-git','bun1.4.3-cache']
  for name in [self.project+'-'+x for x in suffixes]+[self.project+'_secrets']:
   r=subprocess.run(['docker','volume','inspect',name],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,env=self.env)
   if r.returncode==0:raise RuntimeError('FRESH_VOLUME_KEY_REQUIRED')
  self.cli('populate',log='populate.log');self.resolve_workspace()
  if self.guest('git','-C','/workspace/erpx','rev-parse','HEAD').decode().strip()!=self.plan['baseRef']:raise RuntimeError('GUEST_BASE_MISMATCH')
  for name in self.overlay:
   parts=Path(name).parts
   for i in range(1,len(parts)+1):
    target='/workspace/erpx/'+'/'.join(parts[:i]);self.guest('sh','-ec','test ! -L "$1"','sh',target)
  stream=io.BytesIO()
  with tarfile.open(fileobj=stream,mode='w') as archive:
   for name,data in self.overlay.items():
    entry=tarfile.TarInfo(name);entry.size=len(data);entry.mode=0o755 if os.access(ROOT/name,os.X_OK) else 0o644;archive.addfile(entry,io.BytesIO(data))
  self.guest('tar','-xf','-','-C','/workspace/erpx',data=stream.getvalue());self.check_frozen();self.report['overlaySha256']=self.baseline
  editor=json.loads(self.guest('cat','/workspace/erpx/.devcontainer/devcontainer.json'))
  if editor.get('workspaceFolder')!='/workspace/erpx' or editor.get('postCreateCommand')!='scripts/local/dev-container bootstrap' or editor.get('postStartCommand')!='scripts/local/dev-container start':raise RuntimeError('EDITOR_LIFECYCLE_MISMATCH')
  self.report['editorHooks']={'workspaceFolder':editor['workspaceFolder'],'postCreateCommand':editor['postCreateCommand'],'postStartCommand':editor['postStartCommand']}
 def bootstrap(self):
  self.cli('bootstrap',log='bootstrap.log',timeout=1800);self.cli('start','--app=service',log='start.log',timeout=600);self.check_frozen()
 def certificate_pin(self):
  ca,_=self.cli('certificate');ca_path=self.evidence/'root-ca.pem'
  if ca_path.exists():
   if ca_path.read_bytes()!=ca:raise RuntimeError('CONTRIBUTOR_CA_CHANGED')
  else:private(ca_path,ca)
  pins=[]
  for host in ['account.erpx.localhost','acme.erpx.localhost','northstar.erpx.localhost','erpx-internal.erpx.localhost']:
   result=subprocess.run(['openssl','s_client','-connect','127.0.0.1:3443','-servername',host,'-verify_hostname',host,'-verify_return_error','-CAfile',str(ca_path),'-showcerts'],input=b'',stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=15)
   if result.returncode:raise RuntimeError('CONTRIBUTOR_TLS_VALIDATION_FAILED')
   match=re.search(rb'-----BEGIN CERTIFICATE-----.*?-----END CERTIFICATE-----',result.stdout,re.S)
   if not match:raise RuntimeError('CONTRIBUTOR_TLS_CERTIFICATE_REQUIRED')
   public=subprocess.run(['openssl','x509','-pubkey','-noout'],input=match.group(),stdout=subprocess.PIPE,check=True).stdout
   der=subprocess.run(['openssl','pkey','-pubin','-outform','DER'],input=public,stdout=subprocess.PIPE,check=True).stdout
   pins.append(base64.b64encode(hashlib.sha256(der).digest()).decode())
  return ','.join(pins)
 def browser(self,mode='create',count_id=None):
  b=dict(self.plan['browser']);b.update(mode=mode,spki=self.certificate_pin(),progressPath=str(self.evidence/'browser-progress.json'),resultPath=str(self.evidence/('browser-'+mode+'.json')),screenshotPath=str(self.evidence/('browser-'+mode+'.png')))
  progress=json.loads((self.evidence/'browser-progress.json').read_text()) if (self.evidence/'browser-progress.json').exists() else {};hmr_needed=mode=='create' and not progress.get('hmr',{}).get('receipt')
  if count_id:b['countId']=count_id
  with tempfile.TemporaryDirectory(prefix='erpx-qual-browser-') as tmp:
   os.chmod(tmp,0o700);source_path=Path(tmp)/'synthetic-credentials.json';private(source_path,self.guest('cat','/workspace/erpx/.runtime/local/config/dev-container-credentials.json'));b['credentialsPath']=str(source_path)
   cfg=Path(tmp)/'browser.json';private(cfg,json.dumps(b).encode());cmd=[self.plan['browserNode'],str(ROOT/'scripts/testing/fixtures/dev-container-service-qualification.mjs'),str(cfg)]
   proc=subprocess.Popen(cmd,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,cwd=ROOT,env=self.env,bufsize=0);chunks=[];pending=b''
   original=None;path=None
   try:
    if hmr_needed:
     selector=selectors.DefaultSelector();selector.register(proc.stdout,selectors.EVENT_READ);deadline=time.monotonic()+180;ready=False
     while time.monotonic()<deadline:
      if selector.select(1):
       part=os.read(proc.stdout.fileno(),4096);chunks.append(part);pending+=part
       if b'READY_FOR_HMR' in pending:pending=pending.split(b'READY_FOR_HMR',1)[1];ready=True;break
      if proc.poll() is not None:break
     if not ready:raise RuntimeError('BROWSER_HMR_READY_REQUIRED')
     edit=self.plan['hmr'];path=edit['path']
     if not allowed(path):raise RuntimeError('HMR_PATH_INVALID')
     self.guest('sh','-ec','test -f "$1" && test ! -L "$1"','sh','/workspace/erpx/'+path);original=self.guest('cat','/workspace/erpx/'+path);needle=edit['needle'].encode();marker=edit['replacement'].encode()
     if original.count(needle)!=1 or b['hmrMarker'].encode() not in marker:raise RuntimeError('HMR_EXACT_EDIT_REQUIRED')
     self.guest('sh','-ec','cat > "$1"','sh','/workspace/erpx/'+path,data=original.replace(needle,marker,1))
     deadline=time.monotonic()+90;visible=False
     while time.monotonic()<deadline:
      if b'HMR_EDIT_VISIBLE' in pending:visible=True;break
      if selector.select(1):
       part=os.read(proc.stdout.fileno(),4096);chunks.append(part);pending+=part
       if b'HMR_EDIT_VISIBLE' in pending:visible=True;break
      if proc.poll() is not None:break
     if not visible:raise RuntimeError('BROWSER_HMR_EDIT_NOT_VISIBLE')
     self.guest('sh','-ec','cat > "$1"','sh','/workspace/erpx/'+path,data=original)
     if digest(self.guest('cat','/workspace/erpx/'+path))!=digest(original):raise RuntimeError('HMR_SOURCE_RESTORE_FAILED')
     original=None
    tail,_=proc.communicate(timeout=240);chunks.append(tail)
    if proc.returncode:raise RuntimeError('BROWSER_QUALIFICATION_FAILED')
   finally:
    if proc.poll() is None:proc.terminate();proc.wait(timeout=10)
    if original is not None:
     self.guest('sh','-ec','cat > "$1"','sh','/workspace/erpx/'+path,data=original)
     if digest(self.guest('cat','/workspace/erpx/'+path))!=digest(original):raise RuntimeError('HMR_SOURCE_RESTORE_FAILED')
    private(self.evidence/('browser-'+mode+'.log'),b''.join(chunks))
  result=json.loads((self.evidence/('browser-'+mode+'.json')).read_text())
  if not result.get('success'):raise RuntimeError('BROWSER_SUCCESS_REQUIRED')
  return result
 def worker_proof(self,count_id):
  out=self.guest('sh','-ec','cd /workspace/erpx && node scripts/testing/fixtures/dev-container-service-qualification-db.mjs "$1"','sh',count_id,timeout=60)
  result=json.loads(out);private(self.evidence/'worker-proof.json',out);return result
 def restart(self,count_id):
  self.cli('bootstrap',log='rebootstrap.log',timeout=1800);self.cli('down',log='down.log');self.cli('up','--app=service',log='restart.log',timeout=1800);self.resolve_workspace();self.check_frozen();self.report['restartReadback']=self.browser('readback',count_id)
 def guest_cache_build(self):
  cold=self.guest('cat','/workspace/erpx/.scratch/dev/native-build.log');metadata=self.guest('cat','/workspace/erpx/.scratch/dev/native-build.json');private(self.evidence/'rust-cold-start.log',cold);private(self.evidence/'rust-cold-start.json',metadata)
  if b'Compiling erpx-service ' not in cold:raise RuntimeError('COLD_RUST_BUILD_EVIDENCE_REQUIRED')
  target='src/composition/src/service_initial_physical_stock_contracts.rs';original=self.guest('cat','/workspace/erpx/'+target);baseline=digest(original)
  cmd=['sh','-ec','cd /workspace/erpx && export CARGO_TARGET_DIR="/workspace/erpx/target/dev-native-$(node -p process.arch)-rust1991" CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=1 && cargo build --locked -v -p erpx-service --no-default-features --features private-field-api --bin erpx-server']
  def build(name,require_compilation):
   begin=time.monotonic();out=self.guest(*cmd,timeout=1800);private(self.evidence/name,out)
   compiling=b'Compiling erpx-service ' in out
   if require_compilation is not None and compiling!=require_compilation:raise RuntimeError('RUST_CACHE_REBUILD_EVIDENCE_REQUIRED')
   return round(time.monotonic()-begin,3)
  timings={'first':build('rust-first.log',None),'warm':build('rust-warm.log',False)}
  try:
   self.guest('sh','-ec','cat >> /workspace/erpx/'+target,data=b'\n// Disposable contributor cache qualification.\n');timings['edited']=build('rust-edited.log',True)
  finally:self.guest('sh','-ec','cat > /workspace/erpx/'+target,data=original)
  if digest(self.guest('cat','/workspace/erpx/'+target))!=baseline:raise RuntimeError('RUST_SOURCE_RESTORE_FAILED')
  timings['restored']=build('rust-restored.log',True);self.report['rustCache']={'timingsSeconds':timings,'restoredSha256':baseline}
 def save(self):
  path=self.evidence/'result.json';temporary=self.evidence/'result.pending';private(temporary,json.dumps(self.report,indent=2).encode());os.replace(temporary,path)

def main():
 p=argparse.ArgumentParser();p.add_argument('--plan',required=True);p.add_argument('--execute',action='store_true');p.add_argument('--resume',action='store_true');p.add_argument('--bootstrap-only',action='store_true');args=p.parse_args();plan=json.loads(Path(args.plan).read_text())
 if not args.execute:raise SystemExit('Explicit --execute required for owned lifecycle qualification')
 case=Case(plan,args.resume)
 try:
  if args.resume:case.resolve_workspace();case.check_frozen()
  else:case.populate();case.bootstrap()
  if args.bootstrap_only:case.report['bootstrapComplete']=True;return
  case.guest_cache_build();case.cli('restart-native',log='restart-native.log',timeout=600);browser=case.browser();case.report['browser']=browser;count_id=browser['count']['id'];case.report['worker']=case.worker_proof(count_id);case.restart(count_id);case.check_frozen();case.report['success']=True
 finally:case.save()
if __name__=='__main__':main()
