import { createHash } from 'node:crypto';
import { cp, mkdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';

const manifestPath='contracts/modules/ui-modules.json';
const sourcePath='packages/feature-runtime/src/modules/example/page.tsx';
/** Build-time registration is deliberately finite; this does not discover runtime plugins. */
export async function uiModuleSelection(root:string,app:string,selection:string|undefined) {
  if (selection !== undefined && selection !== '' && selection !== 'erpx-example') throw Error('UNKNOWN_UI_MODULE_SELECTION');
  if (app !== 'apps' && selection) throw Error('UI_MODULE_APP_MISMATCH');
  if (app !== 'apps') return {enabled:false,fingerprint:''};
  const manifestBytes=await readFile(join(root,manifestPath)),manifest:unknown=JSON.parse(manifestBytes.toString());
  const expected={version:1,modules:[{id:'erpx-example',app:'apps',route:'/applications/example-module',source:sourcePath,apiContract:'erpx.example.http',apiVersion:'1.0.0'}]};
  if(JSON.stringify(manifest)!==JSON.stringify(expected))throw Error('INVALID_UI_MODULE_MANIFEST');
  const enabled=selection==='erpx-example',hash=createHash('sha256').update(manifestBytes).update(enabled?'enabled':'disabled');
  if(enabled)hash.update(await readFile(join(root,sourcePath))).update(await readFile(join(root,'scripts/local/ui-module-registration.ts')));
  return {enabled,fingerprint:hash.digest('hex')};
}
export async function composeUiModules(root:string,snapshot:string,enabled:boolean) {
  if(!enabled)return;
  await writeFile(join(snapshot,'apps/launcher/src/proxy.ts'),"import { appProxy } from '../../../scripts/local/app-proxy';\nexport const proxy = appProxy('apps', ['/applications/example-module']);\nexport const config = { matcher: ['/:path*'] };\n");
  const destination=join(snapshot,'apps/launcher/src/app/applications/example-module');
  await mkdir(destination,{recursive:false});
  await cp(join(root,sourcePath),join(destination,'implementation.tsx'),{errorOnExist:true,force:false});
  await writeFile(join(destination,'page.tsx'),"import {currentSession} from '@erpx/server-identity_access/public/authorization';\nimport ExampleModulePage from './implementation';\nexport const dynamic = 'force-dynamic';\nexport default async function Page() { await currentSession(); return ExampleModulePage(); }\n",{flag:'wx'});
}
