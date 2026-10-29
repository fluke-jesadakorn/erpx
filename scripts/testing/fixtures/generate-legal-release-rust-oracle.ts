/** Capture the unchanged Legal release SQL predicate from B0 on pinned Node. */
import {createHash} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {mkdir,readFile,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
const sourceCommit='a5ed8737a2f05eff1ee9841fda4a094b3926d732';
const paths=['packages/server-platform_administration/src/adapters/release-predicate.ts','packages/catalogue/src/production-scope.ts','packages/server-organization/src/adapters/platform-owner-projection-sources.ts','packages/server-identity_access/src/adapters/platform-owner-projection-sources.ts'];
const sources:{path:string;sha256:string}[]=[];const bytes:string[]=[];
for(const path of paths){const content=await readFile(path,'utf8');if(content!==execFileSync('git',['show',`${sourceCommit}:${path}`],{encoding:'utf8'}))throw new Error('LEGAL_RELEASE_ORACLE_SOURCE_CHANGED');bytes.push(content);sources.push({path,sha256:createHash('sha256').update(content).digest('hex')});}
const node=process.env.ERPX_NODE_BIN??'node';if(execFileSync(node,['--version'],{encoding:'utf8'}).trim()!=='v26.11.1')throw new Error('PINNED_RELEASE_ORACLE_NODE_REQUIRED');
const directory=resolve('.scratch/dev/legal-release-rust-oracle');await mkdir(directory,{recursive:true,mode:0o700});
const projection=(index:number,name:string)=>bytes[index]!.slice(bytes[index]!.indexOf('export const platformOwnerProjectionSources')).replace('export const platformOwnerProjectionSources',`const ${name}`);
const predicate=bytes[0]!.slice(bytes[0]!.indexOf('export function releaseRevisionTable('));
const entry=resolve(directory,'entry.ts');
await writeFile(entry,`import {productionScope,legalProductionScope,criteriaForScope} from ${JSON.stringify(resolve(paths[1]!))};
const artifact={softwareVersion:'synthetic-native-release'};
${projection(2,'organizationPlatformSources')}
${projection(3,'identity_accessPlatformSources')}
${predicate}
process.stdout.write(JSON.stringify({node:process.version,scope:legalProductionScope.version,criteria:criteriaForScope(legalProductionScope.version),predicate:acceptedReleasePredicate('$1',legalProductionScope.version)}));\n`,{mode:0o600});
const built=await Bun.build({entrypoints:[entry],outdir:directory,target:'node',format:'esm',naming:'oracle.mjs'});if(!built.success)throw new Error('LEGAL_RELEASE_ORACLE_BUILD_FAILED');
const oracle=JSON.parse(execFileSync(node,[resolve(directory,'oracle.mjs')],{encoding:'utf8',maxBuffer:1024*1024}));
const output='fixtures/platform_administration/rust-legal-release';await mkdir(output,{recursive:true});
await writeFile(resolve(output,'predicate.v1.sql'),`SELECT ${oracle.predicate} AS legacy_allowed;\n`);
await writeFile(resolve(output,'source.v1.json'),JSON.stringify({version:1,sourceCommit,sources,node:oracle.node,scope:oracle.scope,softwareVersion:'synthetic-native-release',criteria:oracle.criteria,predicateSha256:createHash('sha256').update(`SELECT ${oracle.predicate} AS legacy_allowed;\n`).digest('hex')},null,2)+'\n');
console.log(`Retained Legal release oracle: ${oracle.criteria.length} independent gates, exact B0 SQL.`);

await writeFile("packages/server-platform_administration/resources/legal-release.v1.json",JSON.stringify({schema:"erpx-legal-release/v1",source:sources[1],scopeVersion:oracle.scope,criteria:oracle.criteria.map(({id,permission}:{id:string;permission:string})=>({id,permission}))},null,2)+"\n");
