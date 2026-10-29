import {createRequire} from 'node:module';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {readFileSync,writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {DatabaseSync} from 'node:sqlite';
if(process.version!=='v26.11.1')throw Error('PINNED_NODE_REQUIRED');
const revision='a5ed8737a2f05eff1ee9841fda4a094b3926d732';
const path='packages/server-assets/src/adapters/maintenance-transaction.ts';
const original=execFileSync('git',['show',`${revision}:${path}`],{encoding:'utf8'});
const ts=createRequire(resolve('package.json'))('typescript');
const loaded={exports:{}};
new Function('module','exports','require',ts.transpileModule(original,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText)(loaded,loaded.exports,name=>{
 if(name==='@erpx/server-workflow/public/command-assertions')return {assertion:(_db,_key,label,predicate,bindings)=>({guard:{label,predicate,bindings}})};
 throw Error('UNREVIEWED_ORACLE_IMPORT:'+name);
});
const db=new DatabaseSync(':memory:');
db.exec('CREATE TABLE operational_assets(id TEXT,version INTEGER,condition TEXT,lifecycle_status TEXT,placement TEXT,custodian_id TEXT,updated_at INTEGER);CREATE TABLE operational_asset_events(id TEXT,asset_id TEXT,version INTEGER,action TEXT,actor_id TEXT,reason TEXT,condition TEXT,lifecycle_status TEXT,placement TEXT,custodian_id TEXT,created_at INTEGER)');
const port={prepare:sql=>({bind:(...bindings)=>({sql,bindings})})};
const cases=[];
for(const lifecycle of ['registered','in_service','maintenance','retirement_pending','retired'])
for(const condition of ['good','degraded','unsafe'])
for(const placement of [null,'','Synthetic desk'])
for(const downtime of [null,0,100,-100])
for(const action of ['start','complete-good','complete-degraded','complete-unsafe','cancel']){
 const before={version:7,condition,lifecycle_status:lifecycle,placement,custodian_id:'synthetic-custodian',updated_at:1};
 db.exec('DELETE FROM operational_assets;DELETE FROM operational_asset_events');
 db.prepare('INSERT INTO operational_assets VALUES(?,?,?,?,?,?,?)').run('asset',before.version,condition,lifecycle,placement,before.custodian_id,1);
 const input={action:action.startsWith('complete-')?'complete':action,resultingCondition:action.slice(9),reason:'Synthetic reason'};
 const steps=loaded.exports.prepareAssetMaintenanceChange(port,{userId:'actor'},'command',{asset_id:'asset',asset_version:7,downtime_started_at:downtime},input,300);
 let refusal=null;
 for(const step of steps){
  if(step.guard){if(!db.prepare(`SELECT ${step.guard.predicate} AS allowed`).get(...step.guard.bindings).allowed){refusal=step.guard.label;break;}}
  else db.prepare(step.sql).run(...step.bindings);
 }
 cases.push({action,downtime,before,refusal,after:db.prepare('SELECT version,condition,lifecycle_status,placement,custodian_id,updated_at FROM operational_assets').get(),events:db.prepare('SELECT action,actor_id,reason,version,condition,lifecycle_status,placement,custodian_id,created_at FROM operational_asset_events').all()});
}
const result={version:1,revision,node:process.version,acceptance:false,description:'Development-only execution of the unchanged B0 prepared SQL scalar CASE/NULL expressions in in-memory SQLite. No authority, PostgreSQL constraints/locking or owner command acceptance is established; actual migrated PostgreSQL effect conformance remains required.',sources:[{path,sha256:createHash('sha256').update(original).digest('hex')}],cases};
const target=resolve('fixtures/assets/rust-maintenance-effects/conformance.v1.json');const bytes=JSON.stringify(result,null,2)+'\n';
if(process.argv.includes('--check')){if(readFileSync(target,'utf8')!==bytes)throw Error('ASSETS_MAINTENANCE_ORACLE_DRIFT');}else writeFileSync(target,bytes);
console.log(JSON.stringify({cases:cases.length,source:result.sources[0],node:process.version}));
