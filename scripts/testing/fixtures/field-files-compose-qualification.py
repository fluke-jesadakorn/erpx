#!/usr/bin/env python3
"""Exercise the checked-in Compose file against a fresh, owned synthetic lease."""
import argparse, hashlib, importlib.util, json, os, time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
spec=importlib.util.spec_from_file_location('field_files_qualification',ROOT/'scripts/testing/fixtures/field-files-container-qualification.py')
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)

class Case(module.Case):
 def compose(self,*args,check=True):
  return __import__('subprocess').run(['docker-compose','-p',self.prefix,'-f',str(ROOT/'ops/containers/field-files-native.compose.yaml'),*args],stdout=__import__('subprocess').PIPE,stderr=__import__('subprocess').PIPE,env=self.env,check=check)
 def prepare_files(self,state):
  seed,pg,volume=state['seed'],state['postgres'],state['volume']
  self.docker('exec','-u','0',seed,'sh','-ec','mkdir -p /case/files /case/tmp /case/socket /case/defs; chown -R 65532:65532 /case/files /case/tmp /case/socket /case/defs; chmod 700 /case/files /case/tmp; chmod 755 /case/socket /case/defs')
  self.docker('cp',str(Path(self.args.definitions).resolve())+'/.',f'{seed}:/case/defs');self.docker('exec','-u','0',seed,'chmod','-R','a+rX','/case/defs')
  scanner_config=self.private('clamd.conf','Foreground yes\nDatabaseDirectory /var/lib/clamav\nLocalSocket /run/erpx-scanner/clamd.sock\nLocalSocketMode 666\nFixStaleSocket yes\nLogTime yes\n')
  self.docker('cp',str(scanner_config),f'{seed}:/case/clamd.conf');self.docker('exec','-u','0',seed,'chmod','644','/case/clamd.conf')
  local=self.case/'seed-config.json';self.docker('cp',f'{seed}:/case/native-api.json',str(local));local.chmod(0o600)
  config=json.loads(local.read_text());config['listen']='127.0.0.1:44817'
  config['fieldAttachmentLinks']=[{'holdingId':'h001','mode':'enabled'}];config['fieldAttachmentBinds']=[{'holdingId':'h001','mode':'enabled'}];config['fieldFilesPolicy']={'mode':'enabled','uploadsFrozen':False}
  config['fieldFilesProviders']={'targets':[{'holdingId':'h001','companyId':'company','filesBinding':'HOLDING_H001_FILES','filesRoot':'/var/lib/erpx-files','temporaryRoot':'/var/tmp/erpx-files','previewBinary':'/usr/local/bin/erpx-render-preview','heifConvert':'/usr/bin/heif-convert','pdftoppm':'/usr/bin/pdftoppm','clamavSocket':'/run/erpx-scanner/clamd.sock','clamavDatabaseDirectory':'/var/lib/clamav','maximumDefinitionAgeMs':604800000,'commandTimeoutMs':5000,'scanTimeoutMs':30000,'concurrentScans':1}]}
  patched=self.private('private-api.json',json.dumps(config));self.docker('cp',str(patched),f'{seed}:/case/private-api.json');self.docker('exec','-u','0',seed,'chown','65532:65532','/case/private-api.json');self.docker('exec','-u','0',seed,'chmod','600','/case/private-api.json')
  creds=self.case/'credentials.json';self.docker('cp',f'{seed}:/case/credentials.json',str(creds));credentials=json.loads(creds.read_text());credentials={'maker':{'email':credentials['email'],'password':credentials['password'],'assignmentId':credentials['assignmentId'],'assignmentVersion':credentials['assignmentVersion']},'companyId':'company'};creds.write_text(json.dumps(credentials));creds.chmod(0o600);self.docker('cp',str(creds),f'{seed}:/case/credentials.json');self.docker('exec','-u','0',seed,'chown','65532:65532','/case/credentials.json');self.docker('exec','-u','0',seed,'chmod','600','/case/credentials.json')
  mount=self.docker('volume','inspect','--format','{{.Mountpoint}}',volume).stdout.decode().strip()
  self.env.update(COMPOSE_DISABLE_ENV_FILE='1',ERPX_FILES_NETWORK_CONTAINER=pg,ERPX_FILES_CONFIG_DIR=mount,ERPX_FILES_STORE_DIR=mount+'/files',ERPX_FILES_TEMP_DIR=mount+'/tmp',ERPX_CLAMD_SOCKET_DIR=mount+'/socket',ERPX_CLAMAV_DEFINITIONS_DIR=mount+'/defs',ERPX_FILES_API_IMAGE=self.args.runtime_image,ERPX_FILES_WORKER_IMAGE=self.args.worker_image)
  self.compose('up','-d')
  state['config']=config
  for label in ('api','worker','scanner'):
   name=self.compose('ps','-q',label).stdout.decode().strip()
   if not name:raise RuntimeError('missing Compose '+label)
   state[label]=name
   for _ in range(30):
    if self.docker('inspect','--format','{{.State.Running}}',name).stdout.strip()!=b'true':raise RuntimeError('Compose '+label+' exited')
    time.sleep(.1)
 def cleanup(self):
  error=None; safe_to_down='ERPX_FILES_NETWORK_CONTAINER' in self.env
  try:
   ids=self.compose('ps','-a','-q').stdout.decode().split()
   for name in ids:
    data=json.loads(self.docker('inspect',name).stdout)[0]
    if data['Config']['Labels'].get('com.docker.compose.project')!=self.prefix:
     safe_to_down=False;raise RuntimeError('Compose project identity changed')
   if ids:
    self.compose('stop',check=True)
    for name in ids:
     data=json.loads(self.docker('inspect',name).stdout)[0]
     labels=data['Config']['Labels'];label=labels.get('com.docker.compose.service','unknown')
     if labels.get('com.docker.compose.project')!=self.prefix or data['State']['ExitCode']!=0:raise RuntimeError('Compose '+label+' did not exit cleanly')
     self.shutdown['compose-'+label]='0'
  except Exception as caught:error=caught
  finally:
   if safe_to_down:
    try:self.compose('down','--remove-orphans',check=True)
    except Exception as caught:
     if error is None:error=caught
     else:error.add_note('Compose down also failed: '+type(caught).__name__)
   try:module.base.Case.cleanup(self)
   except Exception as caught:
    if error is None:error=caught
    else:error.add_note('Owned resource cleanup also failed: '+str(caught))
  if error is not None:raise error
 def scenario(self,state):
  result=super().scenario(state);seed,pg,volume=state['seed'],state['postgres'],state['volume']
  scanner_image=json.loads(self.docker('image','inspect',self.args.scanner_image).stdout)[0]
  scanner_container=json.loads(self.docker('inspect',state['scanner']).stdout)[0]
  if scanner_container['Image']!=scanner_image['Id']:raise RuntimeError('Compose scanner image differs from pinned scanner input')
  targets={m['Destination'] for m in scanner_container['Mounts']}
  if targets!={'/run/erpx-scanner-config/clamd.conf','/run/erpx-scanner','/var/lib/clamav'}:raise RuntimeError('scanner mounts exceed the isolated provider scope')
  if self.docker('exec',state['scanner'],'test','!','-e','/run/erpx-files/private-api.json',check=False).returncode:raise RuntimeError('scanner can see API configuration')
  sources=['ops/containers/field-files-native.Dockerfile','ops/containers/field-files-native.Dockerfile.dockerignore','ops/containers/field-files-native.compose.yaml','scripts/testing/fixtures/field-files-container-fixture.Dockerfile','scripts/testing/fixtures/field-files-container-fixture-overlay.Dockerfile','scripts/testing/fixtures/field-files-origin-runtime-overlay.Dockerfile','scripts/testing/fixtures/field-files-container-qualification.py','scripts/testing/fixtures/field-files-container-scenario.mjs','scripts/testing/fixtures/field-files-compose-qualification.py','scripts/testing/fixtures/field-files-compose-corpus.mjs','scripts/testing/fixtures/field-files-preview-corpus.Dockerfile','tests/provider/files/rust-preview-provider.test.rs','fixtures/files/native-toolchain/sources.json','scripts/testing/provision-native-worker.ts','packages/server-files/src/adapters/preview_engine.rs']
  result['sourceHashes']={name:hashlib.sha256((ROOT/name).read_bytes()).hexdigest() for name in sources}
  result['images']={**self.images,'worker_image':self.args.worker_image,'scanner_image':scanner_image['Id']};result['architecture']=self.image_arch;result['scannerReference']=self.args.scanner_image;result['scannerConfigurationIsolation']=True
  self.docker('cp',str(ROOT/'fixtures/files/preview'),f'{seed}:/case/preview')
  self.docker('cp',str(ROOT/'scripts/testing/fixtures/field-files-compose-corpus.mjs'),f'{seed}:/case/corpus.mjs')
  self.docker('exec','-u','0',seed,'chown','-R','65532:65532','/case/preview','/case/corpus.mjs')
  self.docker('exec','-u','0',seed,'chmod','-R','a+rX','/case/preview');self.docker('exec','-u','0',seed,'chmod','600','/case/corpus.mjs')
  name=self.remember('container',self.prefix+'-corpus')
  completed=self.docker('run','--rm','--name',name,'--label',f'{module.base.LABEL}={self.ident}','--network',f'container:{pg}','--user','65532:65532','--mount',f'type=volume,src={volume},dst=/case',self.args.client_image,'node','/case/corpus.mjs',check=False)
  corpus=self.case/'compose-corpus-result.json';copied=self.docker('cp',f'{seed}:/case/compose-corpus-result.json',str(corpus),check=False)
  if copied.returncode==0:
   rows=json.loads(corpus.read_text());result['previewCorpus']=rows
   target=Path(self.args.evidence+'.corpus.json');fd=os.open(target,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
   with os.fdopen(fd,'w') as out:json.dump(rows,out,indent=2)
  if completed.returncode:raise RuntimeError('positive preview corpus did not match: '+completed.stderr.decode(errors='replace')[:500])
  return result

def main():
 p=argparse.ArgumentParser()
 for name in ('docker-host','postgres-image','fixture-image','runtime-image','worker-image','scanner-image','client-image','expected-source','definitions','evidence'):p.add_argument('--'+name,required=True)
 p.add_argument('--seed-binary',default='/usr/local/bin/native-field-service-development');args=p.parse_args();case=Case(args);result=None
 try:state=case.start();case.prepare_files(state);result=case.scenario(state);result['composeUpScenarioDown']=True
 finally:case.cleanup()
 if result:
  result['gracefulExits']=case.shutdown;result['cleanupComplete']=True;result['source']=args.expected_source
  fd=os.open(args.evidence,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
  with os.fdopen(fd,'w') as out:json.dump(result,out,indent=2)
  print('native Files Compose qualification passed')
if __name__=='__main__':main()
