import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

// Deliberate development tool: execute only the hash-pinned retained pure source,
// never the new Rust implementation or the generated frontend projection.
const root=resolve(import.meta.dir,'../../..'),path=resolve(root,'fixtures/identity_access/permissions/legacy-work-admission.v1.json');
const fixture=JSON.parse(readFileSync(path,'utf8'));
const pinned='b4db260cd029707886e82ac958daab3b4fab1509';
const allowed=['packages/feature-identity_access/src/lib/business-roles.ts','packages/server-identity_access/src/domain/work-modules.ts','packages/server-identity_access/src/domain/beta-access.ts'];
if(fixture.version!==1||fixture.sources.length!==allowed.length)throw new Error('WORK_ADMISSION_ORACLE_PROVENANCE_INVALID');
const sources=fixture.sources.map((source:{revision:string;path:string;sha256:string},index:number)=>{
  if(source.revision!==pinned||source.path!==allowed[index])throw new Error('WORK_ADMISSION_ORACLE_PROVENANCE_INVALID');
  const bytes=execFileSync('git',['show',`${pinned}:${source.path}`],{cwd:root});
  if(createHash('sha256').update(bytes).digest('hex')!==source.sha256)throw new Error('WORK_ADMISSION_ORACLE_SOURCE_CHANGED');
  return bytes.toString('utf8');
});
const transpiler=new Bun.Transpiler({loader:'ts',target:'bun'});
const moduleUrl=(source:string)=>'data:text/javascript;base64,'+Buffer.from(transpiler.transformSync(source)).toString('base64');
const rolesUrl=moduleUrl(sources[0]);
const imported='import { modulesForRole } from "@erpx/feature-identity_access/lib/business-roles";';
if(!sources[1].startsWith(imported))throw new Error('WORK_ADMISSION_ORACLE_IMPORT_CHANGED');
const legacy=await import(moduleUrl(sources[1].replace(imported,`import { modulesForRole } from ${JSON.stringify(rolesUrl)};`)));
const beta=await import(moduleUrl(sources[2]));
for(const row of fixture.cases)row.expected=legacy.effectiveWorkModules(row.role,row.assigned,row.permissions,row.beta);
for(const row of fixture.destinations)row.expected=beta.betaDestinationAllowed(row.destination,row.api);
writeFileSync(path,JSON.stringify(fixture,null,2)+'\n');
console.log(JSON.stringify({modules:fixture.cases.length,destinations:fixture.destinations.length,source:pinned}));
