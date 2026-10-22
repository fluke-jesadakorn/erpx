import { readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { moduleGateSummary, reviewedCapabilityManifest } from "@erpx/catalogue/capability-gate-reviews";

const root=resolve(import.meta.dir,'../..');
const capabilities=reviewedCapabilityManifest(JSON.parse(await readFile(resolve(root,'docs/status/capability-gate-reviews.json'),'utf8')));
const modules=JSON.parse(await readFile(resolve(root,'packages/catalogue/src/module-registry.json'),'utf8')) as Array<{id:string}>;
const summary=Object.fromEntries(modules.map((module)=>{
  return [module.id,moduleGateSummary(capabilities,module.id)];
}));
const text=JSON.stringify(summary,null,2)+'\n';
const path=resolve(root,'packages/catalogue/src/hospitality-evidence.generated.json');
if(process.argv.includes('--check')){
  if(await readFile(path,'utf8').catch(()=>null)!==text)throw new Error('HOSPITALITY_EVIDENCE_SNAPSHOT_STALE');
}else await writeFile(path,text);
