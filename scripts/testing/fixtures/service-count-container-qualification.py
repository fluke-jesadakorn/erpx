#!/usr/bin/env python3
"""Disposable native Service Count container qualification; never targets host PostgreSQL.

Requires prebuilt local runtime and test-fixture images. All Docker resources have a
fresh label and are removed only after inspecting that exact label. No host port is
published; the seed, runtime and client share the isolated PostgreSQL namespace.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import re
import os
import secrets
import subprocess
import tempfile
import time
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
LABEL = 'com.erpx.service-count-qualification'

class Case:
 def __init__(self, args: argparse.Namespace):
  self.args=args; self.ident=uuid.uuid4().hex; self.prefix='erpx-count-qual-'+self.ident[:12]
  self.env=os.environ.copy(); self.env['DOCKER_HOST']=args.docker_host
  self.resources=[]; self.shutdown={}; self.started=time.monotonic(); self.temp=tempfile.TemporaryDirectory(prefix='erpx-count-qual-'); self.case=Path(self.temp.name); self.case.chmod(0o700)
  self.sources={name:ROOT/'scripts/testing/fixtures'/name for name in ['service-count-container-qualification.py','service-count-container-account.mjs','service-count-container-scenario.mjs']}
  self.sources.update({name:ROOT/'ops/containers'/name for name in ['service-count-native.Dockerfile','service-count-native.Dockerfile.dockerignore','service-count-fixture.Dockerfile','service-count-native.compose.yaml']})
  self.source_hashes={}
  for name,path in self.sources.items():
   data=path.read_bytes();self.source_hashes[name]=hashlib.sha256(data).hexdigest();target=self.case/name;target.write_bytes(data);target.chmod(0o600)
 def docker(self,*args: str, data: bytes|None=None, check: bool=True)->subprocess.CompletedProcess:
  return subprocess.run(['docker',*args],input=data,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=self.env,check=check)
 def owned(self,kind: str,name: str)->bool:
  r=self.docker(kind,'inspect','--format',f'{{{{index .Config.Labels "{LABEL}"}}}}' if kind=='container' else f'{{{{index .Labels "{LABEL}"}}}}',name,check=False)
  return r.returncode==0 and r.stdout.decode().strip()==self.ident
 def remember(self,kind: str,name: str)->str:
  self.resources.append((kind,name));return name
 def private(self,name: str,text: str)->Path:
  path=self.case/name;fd=os.open(path,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
  with os.fdopen(fd,'w') as out:out.write(text)
  return path
 def cleanup(self):
  errors=[]
  for kind,name in reversed(self.resources):
   if self.docker(kind,'inspect',name,check=False).returncode!=0:continue
   if not self.owned(kind,name):errors.append(f'{kind} identity changed: {name}');continue
   if kind=='container' and name.endswith(('-seed','-api','-worker')) and self.docker('inspect','--format','{{.State.Running}}',name).stdout.strip()==b'true':
    self.docker('kill','--signal','SIGINT' if name.endswith('-seed') else 'SIGTERM',name,check=False)
    try:
     waited=subprocess.run(['docker','wait',name],stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,env=self.env,timeout=20);self.shutdown[name.rsplit('-',1)[-1]]=waited.stdout.decode().strip()
     if waited.returncode or waited.stdout.strip()!=b'0':errors.append(f'{name} graceful exit was not successful')
    except subprocess.TimeoutExpired:errors.append('seed graceful shutdown timed out')
   result=self.docker(kind,'rm',*( ['-f'] if kind=='container' else []),name,check=False)
   if result.returncode: errors.append(f'{kind} removal failed: {name}')
  self.temp.cleanup()
  if errors:raise RuntimeError('; '.join(errors))
 def verify_sources(self):
  if any(hashlib.sha256(path.read_bytes()).hexdigest()!=self.source_hashes[name] for name,path in self.sources.items()):raise RuntimeError('qualification source changed during case')
 def resolve_images(self):
  self.images={};metadata={}
  for key in ('postgres_image','fixture_image','runtime_image','client_image'):
   reference=getattr(self.args,key);r=self.docker('image','inspect',reference,check=False)
   if r.returncode:raise RuntimeError(f'local image required: {reference}')
   image=json.loads(r.stdout)[0];identity=image['Id'];self.images[key]=identity;setattr(self.args,key,identity);metadata[key]=image
  fixture,runtime=metadata['fixture_image'],metadata['runtime_image'];fl=fixture['Config'].get('Labels') or {};rl=runtime['Config'].get('Labels') or {};revision=rl.get('org.opencontainers.image.revision');arch=runtime['Architecture']
  if not re.fullmatch(r'[0-9a-f]{40}',self.args.expected_source) or revision!=self.args.expected_source or fl.get('org.opencontainers.image.revision')!=revision or fixture['Architecture']!=arch or fl.get('org.erpx.target-architecture')!=arch or rl.get('org.erpx.target-architecture')!=arch:raise RuntimeError('fixture/runtime image revision or architecture mismatch')
  self.image_revision=revision;self.image_arch=arch;self.image_sizes={key:value['Size'] for key,value in metadata.items()}
  if runtime['Config'].get('User')!='65532:65532':raise RuntimeError('runtime image user mismatch')
  for kind in ('fixture','runtime'):
   if metadata[kind+'_image']['Config'].get('Volumes'):raise RuntimeError('inspection image must not declare anonymous volumes')
   name=self.remember('container',self.prefix+'-inspect-'+kind);image=self.images[kind+'_image'];self.docker('create','--name',name,'--label',f'{LABEL}={self.ident}','--network','none',image)
   self.docker('cp',f'{name}:/usr/local/bin/erpx-server',str(self.case/(kind+'-binary')))
   if kind=='fixture':self.docker('cp',f'{name}:/workspace/erpx/migrations/postgres/holding-read',str(self.case/'holding-read'))
  self.binary_hash=hashlib.sha256((self.case/'fixture-binary').read_bytes()).hexdigest()
  if hashlib.sha256((self.case/'runtime-binary').read_bytes()).hexdigest()!=self.binary_hash:raise RuntimeError('fixture/runtime native binaries differ')
  self.migrations={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted((self.case/'holding-read').glob('*.sql'))}
  if not self.migrations:raise RuntimeError('fixture HoldingRead migrations missing')
 def start(self):
  self.resolve_images()
  pg=self.prefix+'-postgres';volume=self.remember('volume',self.prefix+'-case')
  self.docker('volume','create','--label',f'{LABEL}={self.ident}',volume)
  pg_data=self.remember('volume',self.prefix+'-postgres-data');self.docker('volume','create','--label',f'{LABEL}={self.ident}',pg_data)
  secret=secrets.token_urlsafe(32);admin=secrets.token_urlsafe(32)
  password=self.private('postgres-password',secret+'\n')
  secret_volume=self.remember('volume',self.prefix+'-secret');self.docker('volume','create','--label',f'{LABEL}={self.ident}',secret_volume)
  secret_setup=self.remember('container',self.prefix+'-secret-setup');self.docker('run','-d','--name',secret_setup,'--label',f'{LABEL}={self.ident}','--network','none','--user','0:0','--mount',f'type=volume,src={secret_volume},dst=/run/case','--entrypoint','sleep',self.args.fixture_image,'infinity')
  self.docker('cp',str(password),f'{secret_setup}:/run/case/postgres-password');self.docker('exec',secret_setup,'chmod','600','/run/case/postgres-password')
  pg_env=self.private('postgres.env','POSTGRES_USER=postgres\nPOSTGRES_PASSWORD_FILE=/run/case/postgres-password\nPOSTGRES_DB=postgres\n')
  self.remember('container',pg);self.docker('run','-d','--name',pg,'--label',f'{LABEL}={self.ident}','--network','none','--env-file',str(pg_env),'--mount',f'type=volume,src={secret_volume},dst=/run/case,readonly','--mount',f'type=volume,src={pg_data},dst=/var/lib/postgresql',self.args.postgres_image)
  for _ in range(90):
   r=self.docker('exec',pg,'pg_isready','-q','-h','127.0.0.1','-p','5432','-U','postgres',check=False)
   if r.returncode==0:break
   time.sleep(1)
  else:raise RuntimeError('isolated PostgreSQL did not become ready')
  sql=f"CREATE ROLE erpx_test_admin LOGIN CREATEDB PASSWORD '{admin}';\n".encode()
  self.docker('exec','-i',pg,'psql','-X','-v','ON_ERROR_STOP=1','-U','postgres','-d','postgres',data=sql)
  setup=self.remember('container',self.prefix+'-volume-setup')
  self.docker('run','--rm','--name',setup,'--label',f'{LABEL}={self.ident}','--network','none','--user','0:0','--mount',f'type=volume,src={volume},dst=/case','--entrypoint','/bin/sh',self.args.fixture_image,'-ec','chown 65532:65532 /case && chmod 700 /case')
  seed_env=self.private('seed.env',f'ERPX_TEST_ADMIN_URL=postgres://erpx_test_admin:{admin}@127.0.0.1:5432/postgres\n')
  seed=self.remember('container',self.prefix+'-seed')
  self.docker('run','-d','-i','--name',seed,'--label',f'{LABEL}={self.ident}','--network',f'container:{pg}','--user','65532:65532','--mount',f'type=volume,src={volume},dst=/case','--env-file',str(seed_env),'--entrypoint',self.args.seed_binary,self.args.fixture_image,'/case','127.0.0.1:44816','initial-count')
  for _ in range(180):
   r=self.docker('logs',seed,check=False)
   if b'READY' in r.stdout:break
   if self.docker('inspect','--format','{{.State.Running}}',seed).stdout.strip()!=b'true':
    log=(r.stdout+r.stderr).decode(errors='replace');safe='\n'.join(line for line in log.splitlines() if any(word in line for word in ('panicked','unavailable','required','called `Result::unwrap()`','MIGRATION_','permission denied','No such file')));raise RuntimeError('synthetic seed exited before readiness: '+safe[:500])
   time.sleep(1)
  else:raise RuntimeError('synthetic seed readiness timed out')
  return {'postgres':pg,'seed':seed,'volume':volume,'admin':admin}
 def prepare_runtime(self, state):
  pg,seed,volume=state['postgres'],state['seed'],state['volume']
  local=self.case/'seed-config.json';self.docker('cp',f'{seed}:/case/native-api.json',str(local));local.chmod(0o600)
  config=json.loads(local.read_text());token=uuid.uuid4().hex;name='erpx_dev_rust_'+token
  self.docker('exec','-i',pg,'psql','-X','-v','ON_ERROR_STOP=1','-U','postgres','-d','postgres',data=f"CREATE DATABASE \"{name}\" OWNER erpx_test_admin TEMPLATE template0;\nREVOKE CONNECT ON DATABASE \"{name}\" FROM PUBLIC;\nCOMMENT ON DATABASE \"{name}\" IS 'erpx-rust-test:{token}';\n".encode())
  self.docker('exec','-i',pg,'psql','-X','-v','ON_ERROR_STOP=1','-U','postgres','-d',name,data=b'SET ROLE erpx_test_admin; CREATE TABLE erpx_migrations(name TEXT PRIMARY KEY,checksum TEXT NOT NULL,applied_at TIMESTAMPTZ NOT NULL DEFAULT now());')
  import hashlib
  for path in sorted((self.case/'holding-read').glob('*.sql')):
   raw=path.read_bytes();digest=hashlib.sha256(raw).hexdigest();sql=b'SET ROLE erpx_test_admin; BEGIN;\n'+raw+f"\nINSERT INTO erpx_migrations(name,checksum) VALUES('{path.name}','{digest}'); COMMIT;\n".encode()
   self.docker('exec','-i',pg,'psql','-X','-v','ON_ERROR_STOP=1','-U','postgres','-d',name,data=sql)
  from urllib.parse import urlsplit,urlunsplit
  parsed=urlsplit(config['holdings'][0]['database']['url']);read_url=urlunsplit((parsed.scheme,parsed.netloc,'/'+name,'',''))
  config['holdings'][0]['readDatabase']={'url':read_url,'leaseToken':token};config['listen']='127.0.0.1:44817'
  patched=self.private('runtime.json',json.dumps(config));self.docker('cp',str(patched),f'{seed}:/case/runtime.json');self.docker('exec','-u','0',seed,'chown','65532:65532','/case/runtime.json');self.docker('exec','-u','0',seed,'chmod','600','/case/runtime.json')
  api=self.remember('container',self.prefix+'-api');self.docker('run','-d','--name',api,'--label',f'{LABEL}={self.ident}','--network',f'container:{pg}','--user','65532:65532','--read-only','--cap-drop','ALL','--security-opt','no-new-privileges:true','--tmpfs','/tmp:rw,noexec,nosuid,size=16777216','--mount',f'type=volume,src={volume},dst=/case,readonly',self.args.runtime_image,'private-api','--development-config','/case/runtime.json')
  for _ in range(30):
   if self.docker('inspect','--format','{{.State.Running}}',api).stdout.strip()!=b'true':raise RuntimeError('native API exited during startup')
   time.sleep(.1)
  state.update(api=api,readDatabase=name,readLease=token,config=config,apiStartSeconds=round(time.monotonic()-self.started,3))
 def account_signin(self,state,persona):
  seed,pg,volume=state['seed'],state['postgres'],state['volume']
  user='fixture-a' if persona=='maker' else 'fixture-b'
  control=state['config']['control']['url'].split('/')[3].split('?')[0];holding=state['config']['holdings'][0]['database']['url'].split('/')[3].split('?')[0]
  assert re.fullmatch(r'erpx_dev_rust_[0-9a-f]{32}',control) and re.fullmatch(r'erpx_dev_rust_[0-9a-f]{32}',holding)
  query=f"SELECT s.id FROM session s JOIN sso_session_bindings b ON b.session_id=s.id WHERE s.\"userId\"='{user}' AND b.parent_session_id IS NOT NULL ORDER BY s.id;"
  def sessions():return set(self.docker('exec','-i',pg,'psql','-XAt','-v','ON_ERROR_STOP=1','-U','postgres','-d',control,data=query.encode()).stdout.decode().splitlines())
  before=sessions()
  helper=self.case/'service-count-container-account.mjs';self.docker('cp',str(helper),f'{seed}:/case/account.mjs');self.docker('exec','-u','0',seed,'chown','65532:65532','/case/account.mjs');self.docker('exec','-u','0',seed,'chmod','600','/case/account.mjs')
  client=self.remember('container',self.prefix+'-account-client-'+persona)
  self.docker('run','--rm','--name',client,'--label',f'{LABEL}={self.ident}','--network',f'container:{pg}','--user','65532:65532','--mount',f'type=volume,src={volume},dst=/case',self.args.client_image,'node','/case/account.mjs',persona)
  proof=self.case/f'account-proof-{persona}.json';self.docker('cp',f'{seed}:/case/account-proof-{persona}.json',str(proof));proof.chmod(0o600)
  new=sessions()-before
  if len(new)!=1:raise RuntimeError('actual Holding SSO session was not unique')
  session=new.pop();info=json.loads(proof.read_text());assignment=info['assignmentId']
  if not re.fullmatch(r'[A-Za-z0-9_-]{1,128}',session) or not re.fullmatch(r'[A-Za-z0-9_-]{1,128}',assignment):raise RuntimeError('invalid synthetic selection identity')
  now=int(time.time()*1000);sql=f"INSERT INTO active_work_contexts(session_id,user_id,company_id,assignment_id,assignment_version,revision,expires_at,updated_at) VALUES('{session}','{user}','company','{assignment}',{int(info['assignmentVersion'])},1,{now+3600000},{now});"
  self.docker('exec','-i',pg,'psql','-X','-v','ON_ERROR_STOP=1','-U','postgres','-d',holding,data=sql.encode());info['revision']=1;proof.write_text(json.dumps(info));proof.chmod(0o600);self.docker('cp',str(proof),f'{seed}:/case/account-proof-{persona}.json');self.docker('exec','-u','0',seed,'chown','65532:65532',f'/case/account-proof-{persona}.json');self.docker('exec','-u','0',seed,'chmod','600',f'/case/account-proof-{persona}.json')

 def run_scenario(self,state,persona):
  seed,pg,volume=state['seed'],state['postgres'],state['volume'];helper=self.case/'service-count-container-scenario.mjs'
  self.docker('cp',str(helper),f'{seed}:/case/scenario.mjs');self.docker('exec','-u','0',seed,'chown','65532:65532','/case/scenario.mjs');self.docker('exec','-u','0',seed,'chmod','600','/case/scenario.mjs')
  scenario=self.remember('container',self.prefix+'-scenario-'+persona)
  result=self.docker('run','--rm','--name',scenario,'--label',f'{LABEL}={self.ident}','--network',f'container:{pg}','--user','65532:65532','--mount',f'type=volume,src={volume},dst=/case',self.args.client_image,'node','/case/scenario.mjs',persona,check=False)
  if result.returncode:
   codes=re.findall(r'(?:NATIVE_[A-Za-z0-9_/?-]+|PARTS_[A-Z_]+|COUNT_SELECTION_NOT_PREPARABLE|COUNT_CREATE_READBACK_INVALID|NATIVE_SELECTED_CONTEXT_MISMATCH)',result.stderr.decode(errors='replace'));raise RuntimeError('native '+persona+' scenario failed: '+(codes[-1] if codes else 'CLIENT_FAILURE'))
 def run_worker(self,state):
  pg,seed,volume=state['postgres'],state['seed'],state['volume'];result=self.case/'count-result.json';self.docker('cp',f'{seed}:/case/count-result.json',str(result));result.chmod(0o600);count_data=json.loads(result.read_text());count=count_data['id']
  finance=self.case/'finance-setup.json';self.docker('cp',f'{seed}:/case/finance-setup.json',str(finance));finance.chmod(0o600);setup=json.loads(finance.read_text())
  if not re.fullmatch(r'count-[A-Za-z0-9_-]{1,128}',count) or count_data['companyId']!='company' or count_data['version']!=1 or setup.get('initialReferencesEmpty') is not True or setup.get('mappingConfigured') is not True:raise RuntimeError('invalid synthetic Parts or Count receipt')
  for identifier in (count_data['warehouseId'],count_data['productId'],setup['chartId']):
   if not re.fullmatch(r'[A-Za-z0-9_-]{1,128}',identifier):raise RuntimeError('invalid synthetic Parts receipt identity')
  worker=self.remember('container',self.prefix+'-worker');self.docker('run','-d','--name',worker,'--label',f'{LABEL}={self.ident}','--network',f'container:{pg}','--user','65532:65532','--read-only','--cap-drop','ALL','--security-opt','no-new-privileges:true','--tmpfs','/tmp:rw,noexec,nosuid,size=16777216','--mount',f'type=volume,src={volume},dst=/case,readonly',self.args.runtime_image,'service-count-worker','--development-config','/case/runtime.json')
  write=state['config']['holdings'][0]['database']['url'].split('/')[3].split('?')[0];read=state['readDatabase']
  def query(db,sql):return self.docker('exec','-i',pg,'psql','-XAt','-v','ON_ERROR_STOP=1','-U','postgres','-d',db,data=sql.encode()).stdout.decode().strip()
  sources=query(write,f"SELECT (EXISTS(SELECT 1 FROM finance_charts WHERE id='{setup['chartId']}' AND company_id='company') AND EXISTS(SELECT 1 FROM commerce_service_parts_posting_sources WHERE company_id='company') AND EXISTS(SELECT 1 FROM commerce_warehouses WHERE id='{count_data['warehouseId']}' AND company_id='company') AND EXISTS(SELECT 1 FROM commerce_products WHERE id='{count_data['productId']}' AND company_id='company') AND EXISTS(SELECT 1 FROM inventory_service_initial_physical_sources WHERE count_id='{count}' AND company_id='company' AND warehouse_id='{count_data['warehouseId']}' AND product_id='{count_data['productId']}' AND chart_id='{setup['chartId']}'))::text;")
  if sources!='true':raise RuntimeError('persisted Parts and Count sources missing')
  for _ in range(100):
   result=query(write,f"SELECT count(*) FROM outbox_events WHERE aggregate_type='inventory_count' AND aggregate_id='{count}' AND event_type='inventory.count.create' AND aggregate_version=1 AND processed_at IS NOT NULL AND last_error IS NULL;")
   if result=='1':break
   if self.docker('inspect','--format','{{.State.Running}}',worker).stdout.strip()!=b'true':raise RuntimeError('native worker exited before projection')
   time.sleep(.2)
  else:raise RuntimeError('native Count outbox not acknowledged')
  event=query(write,f"SELECT id FROM outbox_events WHERE aggregate_type='inventory_count' AND aggregate_id='{count}' AND event_type='inventory.count.create' AND aggregate_version=1 AND processed_at IS NOT NULL AND last_error IS NULL;")
  if not re.fullmatch(r'[A-Za-z0-9_-]{1,180}',event):raise RuntimeError('invalid projected event identity')
  receipts=query(read,f"SELECT count(*) FROM projection_receipts WHERE event_id='{event}' AND aggregate_id='{count}' AND aggregate_version=1;");checkpoints=query(read,f"SELECT count(*) FROM projection_checkpoints WHERE last_event_id='{event}';")
  if receipts!='1' or checkpoints!='1':raise RuntimeError('native Count projection receipt or checkpoint missing')
  stats=self.docker('stats','--no-stream','--format','{{json .}}',state['api'],worker).stdout.decode().splitlines()
  hardening={}
  for label,name in [('api',state['api']),('worker',worker)]:
   raw=self.docker('inspect','--format','{{json .HostConfig}}',name).stdout;cfg=json.loads(raw);hardening[label]={'readOnlyRootfs':cfg['ReadonlyRootfs'],'capDrop':cfg['CapDrop'],'securityOpt':cfg['SecurityOpt'],'tmpfs':cfg['Tmpfs']}
   if not cfg['ReadonlyRootfs'] or 'ALL' not in cfg['CapDrop'] or 'no-new-privileges:true' not in cfg['SecurityOpt'] or '/tmp' not in cfg['Tmpfs']:raise RuntimeError('runtime hardening mismatch')
  return {'initialFinanceReferencesEmpty':True,'nativePartsMappingReadback':True,'nativePartsMastersAndCountSourcesPersisted':True,'countCreated':True,'outboxAcknowledged':True,'matchingReceiptCount':1,'checkpointCount':1,'stats':[json.loads(line) for line in stats],'hardening':hardening}

def main()->None:
 parser=argparse.ArgumentParser();parser.add_argument('--docker-host',required=True);parser.add_argument('--postgres-image',required=True);parser.add_argument('--fixture-image',required=True);parser.add_argument('--runtime-image',required=True);parser.add_argument('--client-image',required=True);parser.add_argument('--seed-binary',default='/usr/local/bin/native-field-service-development');parser.add_argument('--evidence',required=True);parser.add_argument('--expected-source',required=True);args=parser.parse_args()
 case=Case(args);evidence=None
 try:
  state=case.start()
  case.prepare_runtime(state)
  case.account_signin(state,'finance');case.run_scenario(state,'finance')
  case.account_signin(state,'maker');case.run_scenario(state,'maker')
  evidence=case.run_worker(state);case.verify_sources();evidence.update(apiStartSeconds=state['apiStartSeconds'],readyStatus=503,healthStatus=200,normalAccountSSO=True,selectedContext='synthetic fixture selection after real Holding SSO')
 finally:case.cleanup()
 if evidence is not None:
  evidence['gracefulExits']=case.shutdown;evidence['caseLabel']=case.ident;evidence['cleanupComplete']=True
  case.verify_sources();evidence['images']=case.images;evidence['imageSizesBytes']=case.image_sizes;evidence['expectedSource']=args.expected_source;evidence['imageRevision']=case.image_revision;evidence['imageArchitecture']=case.image_arch;evidence['nativeBinarySha256']=case.binary_hash;evidence['fixtureHoldingReadMigrations']=case.migrations;evidence['qualificationSourceSha256Before']=case.source_hashes;evidence['qualificationSourceSha256After']={name:hashlib.sha256(path.read_bytes()).hexdigest() for name,path in case.sources.items()}
  path=Path(args.evidence);path.parent.mkdir(parents=True,exist_ok=True);fd=os.open(path,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
  with os.fdopen(fd,'w') as out:json.dump(evidence,out,indent=2)
  print('native Service Count container qualification passed; evidence saved')
if __name__=='__main__':main()
