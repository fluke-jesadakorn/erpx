#!/usr/bin/env python3
"""Prove failure cleanup removes the owned project and preserves unrelated peers."""
import argparse, importlib.util, json, os
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
spec=importlib.util.spec_from_file_location('compose_qualification',ROOT/'scripts/testing/fixtures/field-files-compose-qualification.py')
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)

def main():
 p=argparse.ArgumentParser()
 for name in ('docker-host','postgres-image','fixture-image','runtime-image','worker-image','scanner-image','client-image','expected-source','definitions','evidence'):p.add_argument('--'+name,required=True)
 p.add_argument('--seed-binary',default='/usr/local/bin/native-field-service-development');args=p.parse_args();case=module.Case(args)
 canary=case.prefix+'-canary';label='com.erpx.compose-cleanup-canary';cleaned=False
 try:
  state=case.start();case.prepare_files(state)
  ids=[state[name] for name in ('api','worker','scanner')]
  case.docker('run','-d','--name',canary,'--label',f'{label}={case.ident}','--network','none','--entrypoint','node',args.client_image,'-e','setInterval(()=>{},1000)')
  case.docker('kill','--signal','SIGKILL',state['worker'])
  try:case.cleanup()
  except RuntimeError as error:
   if str(error)!='Compose worker did not exit cleanly':raise
   failure=str(error)
  else:raise RuntimeError('nonzero worker exit was not reported')
  cleaned=True
  if any(case.docker('inspect',name,check=False).returncode==0 for name in ids):raise RuntimeError('owned Compose containers remain after failure')
  remaining=case.docker('ps','-a','--filter',f'label=com.docker.compose.project={case.prefix}','--format','{{.ID}}').stdout.strip()
  if remaining:raise RuntimeError('owned Compose project still has containers')
  identity=case.docker('inspect','--format',f'{{{{index .Config.Labels "{label}"}}}}',canary).stdout.decode().strip()
  if identity!=case.ident or case.docker('inspect','--format','{{.State.Running}}',canary).stdout.strip()!=b'true':raise RuntimeError('unrelated canary changed')
  fd=os.open(args.evidence,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
  with os.fdopen(fd,'w') as out:json.dump({'reportedFailure':failure,'ownedProjectRemoved':True,'unrelatedCanaryPreserved':True,'workerSignal':'SIGKILL'},out,indent=2)
  print('Compose failure cleanup proof passed')
 finally:
  if not cleaned:
   try:case.cleanup()
   except Exception:pass
  check=case.docker('inspect','--format',f'{{{{index .Config.Labels "{label}"}}}}',canary,check=False)
  if check.returncode==0 and check.stdout.decode().strip()==case.ident:case.docker('rm','-f',canary)
if __name__=='__main__':main()
