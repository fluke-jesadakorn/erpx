import {existsSync} from 'node:fs';
import {mkdir,readFile,readdir,writeFile} from 'node:fs/promises';
import {join,resolve} from 'node:path';
import {buildIsolatedApp} from '../local/isolated-app-build';

const root=resolve(import.meta.dirname,'../..');
const mode=process.env.ERPX_BUILD_UI_MODULES===undefined?'off':process.env.ERPX_BUILD_UI_MODULES==='erpx-example'?'on':null;
if(!mode)throw Error('UI_MODULE_PROOF_SELECTION_REQUIRED');
await buildIsolatedApp('apps');
const output=join(root,'apps/launcher/.build'),server=join(output,'server/app/applications/example-module'),standalone=join(output,'standalone/apps/launcher/.build/server/app/applications/example-module');
const required=[join(server,'page.js'),join(server,'page.js.nft.json'),join(standalone,'page.js')];
if(mode==='on'){
 for(const path of required)if(!existsSync(path))throw Error('UI_MODULE_ARTIFACT_MISSING');
 const [compiled,nft,copy]=await Promise.all(required.map(path=>readFile(path,'utf8')));
 if(!compiled.includes('erpx.example.http')||!copy.includes('erpx.example.http')||!Array.isArray(JSON.parse(nft).files))throw Error('UI_MODULE_CONTRACT_TRACE_MISSING');
}else{
 if(existsSync(server)||existsSync(standalone))throw Error('UI_MODULE_DISABLED_ROUTE_PRESENT');
 async function scan(path:string):Promise<boolean>{for(const entry of await readdir(path,{withFileTypes:true})){const child=join(path,entry.name);if(entry.isDirectory()){if(await scan(child))return true;}else if(entry.isFile()&&/\.(?:[cm]?js|json|tsx?)$/.test(entry.name)&&(await readFile(child,'utf8')).includes('erpx.example.http'))return true;}return false;}
 if(await scan(join(output,'server'))||await scan(join(output,'standalone')))throw Error('UI_MODULE_DISABLED_IMPLEMENTATION_PRESENT');
}
const artifact=JSON.parse(await readFile(join(output,'artifact.json'),'utf8'));
const directory=join(root,'.scratch/dev/ui-module-proof');await mkdir(directory,{recursive:true,mode:0o700});
await writeFile(join(directory,`${mode}.json`),JSON.stringify({mode,app:'apps',route:'/applications/example-module',buildFingerprint:artifact.buildFingerprint,runtimeFingerprint:artifact.runtimeFingerprint,routePresent:mode==='on',nftPresent:mode==='on',standalonePresent:mode==='on',acceptance:false},null,2)+'\n',{mode:0o600});
console.log(`UI_MODULE_BUILD_PROOF:${mode}:PASS`);
