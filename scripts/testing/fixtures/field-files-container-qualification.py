#!/usr/bin/env python3
"""Owned isolated PostgreSQL, actual scanner and preview qualification; no host ports."""
import argparse, hashlib, importlib.util, json, os, re, subprocess, time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location('count_qualification', ROOT/'scripts/testing/fixtures/service-count-container-qualification.py')
base = importlib.util.module_from_spec(spec); spec.loader.exec_module(base)

class Case(base.Case):
 def cleanup(self):
  name=self.prefix+'-scanner'
  if self.owned('container',name) and self.docker('inspect','--format','{{.State.Running}}',name,check=False).stdout.strip()==b'true':
   self.docker('kill','--signal','SIGTERM',name)
   try:
    result=subprocess.run(['docker','wait',name],stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,env=self.env,timeout=20)
    self.shutdown['scanner']=result.stdout.decode().strip()
    if result.returncode or result.stdout.strip()!=b'0':raise RuntimeError('scanner graceful exit was not successful')
   finally:super().cleanup()
  else:super().cleanup()
 def resolve_images(self):
  super().resolve_images()
  worker=json.loads(self.docker('image','inspect',self.args.worker_image).stdout)[0]
  labels=worker['Config'].get('Labels') or {}
  if worker['Architecture']!=self.image_arch or labels.get('org.opencontainers.image.revision')!=self.image_revision or labels.get('org.erpx.target-architecture')!=self.image_arch:raise RuntimeError('worker source or architecture mismatch')
  self.args.worker_image=worker['Id'];self.worker_size=worker['Size']
  name=self.remember('container',self.prefix+'-inspect-worker');self.docker('create','--name',name,'--label',f'{base.LABEL}={self.ident}','--network','none',self.args.worker_image)
  self.docker('cp',f'{name}:/usr/local/bin/erpx-server',str(self.case/'worker-binary'))
  if hashlib.sha256((self.case/'worker-binary').read_bytes()).hexdigest()!=self.binary_hash:raise RuntimeError('API and worker binaries differ')
 def docker(self,*args,data=None,check=True):
  args=tuple('workspace' if x=='initial-count' else x for x in args)
  return super().docker(*args,data=data,check=check)
 def prepare_files(self,state):
  seed,pg,volume=state['seed'],state['postgres'],state['volume']
  self.docker('exec','-u','0',seed,'sh','-ec','mkdir -p /case/files /case/tmp /case/socket /case/defs; chown -R 65532:65532 /case/files /case/tmp /case/socket /case/defs; chmod 700 /case/files /case/tmp; chmod 755 /case/socket /case/defs')
  self.docker('cp',str(Path(self.args.definitions).resolve())+'/.',f'{seed}:/case/defs')
  self.docker('exec','-u','0',seed,'sh','-ec','chmod -R a+rX /case/defs')
  scanner_config=self.private('clamd.conf','Foreground yes\nDatabaseDirectory /var/lib/clamav\nLocalSocket /run/erpx-scanner/clamd.sock\nLocalSocketMode 666\nFixStaleSocket yes\nLogTime yes\n')
  self.docker('cp',str(scanner_config),f'{seed}:/case/clamd.conf');self.docker('exec','-u','0',seed,'chmod','644','/case/clamd.conf')
  scanner=self.remember('container',self.prefix+'-scanner')
  mount=self.docker('volume','inspect','--format','{{.Mountpoint}}',volume).stdout.decode().strip()
  self.docker('run','-d','--name',scanner,'--label',f'{base.LABEL}={self.ident}','--network','none','--user','65532:65532','--read-only','--cap-drop','ALL','--security-opt','no-new-privileges:true','--tmpfs','/tmp:rw,noexec,nosuid,size=16777216','--mount',f'type=bind,src={mount}/clamd.conf,dst=/run/erpx-scanner-config/clamd.conf,readonly','--mount',f'type=bind,src={mount}/socket,dst=/run/erpx-scanner','--mount',f'type=bind,src={mount}/defs,dst=/var/lib/clamav,readonly','--entrypoint','clamd',self.args.scanner_image,'--foreground=true','--config-file=/run/erpx-scanner-config/clamd.conf')
  for _ in range(180):
   if self.docker('exec',seed,'test','-S','/case/socket/clamd.sock',check=False).returncode==0:break
   if self.docker('inspect','--format','{{.State.Running}}',scanner).stdout.strip()!=b'true':raise RuntimeError('isolated scanner exited before readiness')
   time.sleep(1)
  else:raise RuntimeError('isolated scanner socket timed out')
  local=self.case/'seed-config.json';self.docker('cp',f'{seed}:/case/native-api.json',str(local));local.chmod(0o600)
  config=json.loads(local.read_text());config['listen']='127.0.0.1:44817'
  config['fieldAttachmentLinks']=[{'holdingId':'h001','mode':'enabled'}];config['fieldAttachmentBinds']=[{'holdingId':'h001','mode':'enabled'}];config['fieldFilesPolicy']={'mode':'enabled','uploadsFrozen':False}
  config['fieldFilesProviders']={'targets':[{'holdingId':'h001','companyId':'company','filesBinding':'HOLDING_H001_FILES','filesRoot':'/case/files','temporaryRoot':'/case/tmp','previewBinary':'/usr/local/bin/erpx-render-preview','heifConvert':'/usr/bin/heif-convert','pdftoppm':'/usr/bin/pdftoppm','clamavSocket':'/case/socket/clamd.sock','clamavDatabaseDirectory':'/case/defs','maximumDefinitionAgeMs':604800000,'commandTimeoutMs':5000,'scanTimeoutMs':30000,'concurrentScans':1}]}
  patched=self.private('runtime.json',json.dumps(config));self.docker('cp',str(patched),f'{seed}:/case/runtime.json');self.docker('exec','-u','0',seed,'chown','65532:65532','/case/runtime.json');self.docker('exec','-u','0',seed,'chmod','600','/case/runtime.json')
  creds=self.case/'credentials.json';self.docker('cp',f'{seed}:/case/credentials.json',str(creds));credentials=json.loads(creds.read_text());credentials={'maker':{'email':credentials['email'],'password':credentials['password'],'assignmentId':credentials['assignmentId'],'assignmentVersion':credentials['assignmentVersion']},'companyId':'company'};creds.write_text(json.dumps(credentials));creds.chmod(0o600);self.docker('cp',str(creds),f'{seed}:/case/credentials.json');self.docker('exec','-u','0',seed,'chown','65532:65532','/case/credentials.json');self.docker('exec','-u','0',seed,'chmod','600','/case/credentials.json')
  def runtime(label,image,command):
   name=self.remember('container',self.prefix+'-'+label)
   self.docker('run','-d','--name',name,'--label',f'{base.LABEL}={self.ident}','--network',f'container:{pg}','--user','65532:65532','--read-only','--cap-drop','ALL','--security-opt','no-new-privileges:true','--tmpfs','/tmp:rw,noexec,nosuid,size=16777216','--mount',f'type=volume,src={volume},dst=/case',image,command,'--development-config','/case/runtime.json')
   for _ in range(30):
    if self.docker('inspect','--format','{{.State.Running}}',name).stdout.strip()!=b'true':raise RuntimeError(label+' exited during startup')
    time.sleep(.1)
   return name
  state.update(config=config,scanner=scanner,api=runtime('api',self.args.runtime_image,'private-api'))
  state['worker']=runtime('worker',self.args.worker_image,'field-files-worker')
 def scenario(self,state):
  self.account_signin(state,'maker');seed,pg,volume=state['seed'],state['postgres'],state['volume']
  for source,destination in [(ROOT/'scripts/testing/fixtures/field-files-container-scenario.mjs','scenario.mjs'),(ROOT/'fixtures/files/preview/png-alpha.input','png-alpha.input')]:
   self.docker('cp',str(source),f'{seed}:/case/{destination}');self.docker('exec','-u','0',seed,'chown','65532:65532',f'/case/{destination}');self.docker('exec','-u','0',seed,'chmod','600',f'/case/{destination}')
  name=self.remember('container',self.prefix+'-scenario')
  result=self.docker('run','--rm','--name',name,'--label',f'{base.LABEL}={self.ident}','--network',f'container:{pg}','--user','65532:65532','--mount',f'type=volume,src={volume},dst=/case',self.args.client_image,'node','/case/scenario.mjs',check=False)
  if result.returncode:raise RuntimeError('scenario failed: '+result.stderr.decode(errors='replace')[:800])
  facts_path=self.case/'files-result.json';self.docker('cp',f'{seed}:/case/files-result.json',str(facts_path));facts=json.loads(facts_path.read_text())
  ids=[facts[k] for k in ('expirySet','expiryLink','expiryUpload')]
  if any(not re.fullmatch(r'[A-Za-z0-9_-]{1,180}',value) for value in ids):raise RuntimeError('invalid expiry fixture identity')
  database=state['config']['holdings'][0]['database']['url'].split('/')[3].split('?')[0]
  if not re.fullmatch(r'erpx_dev_rust_[0-9a-f]{32}',database):raise RuntimeError('invalid leased Holding database')
  set_id,link_id,upload_id=ids;now=int(time.time()*1000)-1000
  def sql(query):return self.docker('exec','-i',pg,'psql','-XAt','-v','ON_ERROR_STOP=1','-U','postgres','-d',database,data=query.encode()).stdout.decode().strip()
  sql(f"UPDATE attachment_links SET expires_at={now} WHERE id='{link_id}'; UPDATE upload_intents SET expires_at={now} WHERE id='{upload_id}';")
  for _ in range(120):
   states=sql(f"SELECT s.state||','||l.state||','||u.status FROM attachment_sets s JOIN attachment_links l ON l.set_id=s.id JOIN upload_intents u ON u.id='{upload_id}' WHERE s.id='{set_id}' AND l.id='{link_id}';")
   if states=='expired,expired,expired':break
   time.sleep(.5)
  else:raise RuntimeError('real worker cleanup did not expire the synthetic draft')
  hardening={}
  for label in ('api','worker','scanner'):
   cfg=json.loads(self.docker('inspect','--format','{{json .HostConfig}}',state[label]).stdout)
   if not cfg['ReadonlyRootfs'] or 'ALL' not in cfg['CapDrop'] or 'no-new-privileges:true' not in cfg['SecurityOpt'] or '/tmp' not in cfg['Tmpfs']:raise RuntimeError(label+' hardening mismatch')
   hardening[label]={'readOnlyRootfs':True,'capDropAll':True,'noNewPrivileges':True}
  api=json.loads(self.docker('image','inspect',self.args.runtime_image).stdout)[0]
  return {'uploadScanPreviewBindDownload':True,'expiredDraftCleanup':True,'normalAccountSSO':True,'apiBytes':api['Size'],'workerBytes':self.worker_size,'nativeBinarySha256':self.binary_hash,'hardening':hardening}

def main():
 p=argparse.ArgumentParser()
 for name in ('docker-host','postgres-image','fixture-image','runtime-image','worker-image','scanner-image','client-image','expected-source','definitions','evidence'):p.add_argument('--'+name,required=True)
 p.add_argument('--seed-binary',default='/usr/local/bin/native-field-service-development');args=p.parse_args();case=Case(args);result=None
 try:
  state=case.start();case.prepare_files(state);result=case.scenario(state)
 finally:case.cleanup()
 if result:
  result['gracefulExits']=case.shutdown;result['cleanupComplete']=True;result['source']=args.expected_source
  fd=os.open(args.evidence,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
  with os.fdopen(fd,'w') as out:json.dump(result,out,indent=2)
  print('native Files isolated container qualification passed')
if __name__=='__main__':main()
