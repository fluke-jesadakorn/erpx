import { existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { appDirectory } from '@erpx/contracts/apps';

/** App composition roots may contain reviewed host/auth exceptions. Scaffolding
 * creates only missing entries and never rewrites an existing app's policy. */
export function ensureWorkspaceProxySource(root:string,id:string){
 const target=resolve(root,appDirectory(id),'src/proxy.ts');
 if(existsSync(target))return false;
 try{writeFileSync(target,`import { appProxy } from "../../../scripts/local/app-proxy";\nexport const proxy = appProxy(${JSON.stringify(id)});\nexport const config = { matcher: ["/:path*"] };\n`,{flag:'wx'});return true;}
 catch(error){if((error as NodeJS.ErrnoException).code==='EEXIST')return false;throw error;}
}
