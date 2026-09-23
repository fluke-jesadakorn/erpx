import { readFile, writeFile } from 'node:fs/promises';
import { resolve } from 'node:path';

const root=resolve(import.meta.dir,'..');
const manifest=JSON.parse(await readFile(resolve(root,'docs/status/business-acceptance-manifest.json'),'utf8')) as {
  capabilities:Array<{id:string;gates:Record<'automated'|'browser'|'provider'|'production',{state:string}>}>;
};
const modules=JSON.parse(await readFile(resolve(root,'src/data/module-registry.json'),'utf8')) as Array<{id:string}>;
const summary=Object.fromEntries(modules.map((module)=>{
  const rows=manifest.capabilities.filter((entry)=>entry.id.startsWith(`${module.id}/`));
  return [module.id,{
    capabilities:rows.length,
    automated:rows.length>0&&rows.every((entry)=>entry.gates.automated.state==='accepted'),
    browser:rows.length>0&&rows.every((entry)=>entry.gates.browser.state==='accepted'),
    provider:rows.length>0&&rows.every((entry)=>['accepted','not_required'].includes(entry.gates.provider.state)),
    production:rows.length>0&&rows.every((entry)=>entry.gates.production.state==='accepted'),
  }];
}));
const text=JSON.stringify(summary,null,2)+'\n';
const path=resolve(root,'src/data/hospitality-evidence.generated.json');
if(process.argv.includes('--check')){
  if(await readFile(path,'utf8').catch(()=>null)!==text)throw new Error('HOSPITALITY_EVIDENCE_SNAPSHOT_STALE');
}else await writeFile(path,text);
