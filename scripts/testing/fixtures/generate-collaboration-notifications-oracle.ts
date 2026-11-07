/** Exact B0 SQL executed by pinned Node's disposable in-memory SQLite. */
import {execFileSync} from 'node:child_process';
import {mkdir,readFile,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
const root=execFileSync('git',['rev-parse','--show-toplevel'],{encoding:'utf8'}).trim();
const outputRoot=resolve(import.meta.dir,'../../..');
const args=process.argv.slice(2);
if(args.length>1||args.some(arg=>arg!=='--check'))throw Error('NOTIFICATION_ORACLE_ARGUMENTS_INVALID');
const node=process.env.ERPX_NODE_BIN??'node';
if(execFileSync(node,['--version'],{encoding:'utf8'}).trim()!=='v26.11.1')throw Error('PINNED_NOTIFICATION_NODE_REQUIRED');
const script=String.raw`
import {createRequire} from 'node:module';
import {createHash} from 'node:crypto';
import {readFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {resolve} from 'node:path';
import {DatabaseSync} from 'node:sqlite';
const ownedRequire=createRequire(resolve('package.json'));
const ts=ownedRequire('typescript');
const revision='a5ed8737a2f05eff1ee9841fda4a094b3926d732';
const paths=['packages/server-collaboration/src/adapters/maintenance-notifications.ts','packages/server-maintenance/src/adapters/collaboration-notification-snapshot-sources.ts'];
const sources=new Map(paths.map(path=>{const bytes=execFileSync('git',['show',revision+':'+path]);if(!bytes.equals(readFileSync(path)))throw Error('B0_NOTIFICATION_SOURCE_CHANGED:'+path);return[path,bytes.toString()];}));
function compile(path,bindings){const loaded={exports:{}};const text=ts.transpileModule(sources.get(path),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;new Function('module','exports','require',text)(loaded,loaded.exports,name=>{if(name==='server-only')return{};if(Object.hasOwn(bindings,name))return bindings[name];throw Error('UNREVIEWED_NOTIFICATION_IMPORT:'+name);});return loaded.exports;}
const snapshot=compile(paths[1],{});
const publisher=compile(paths[0],{'@erpx/server-maintenance/public/collaboration-notification-snapshot-sources':snapshot});
const columns=['id','company_id','branch_id','department_id','asset_id','title','priority','status','owner_id','scheduled_at','downtime_started_at','downtime_ended_at','blocked_reason','completion_evidence','resulting_condition','version','created_by','created_at','updated_at'];
const cases=[];
for(const length of [8,174,175,180])for(const created of [false,true])for(const unicode of [false,true]){
 const key='x'.repeat(length),id=unicode?'งาน-🛠':'work';
 const facts={id,company_id:unicode?'บริษัท':'company',owner_id:unicode?'ผู้รับ':'owner',version:unicode?9007199254740991:8,updated_at:1800000000007};
 const db=new DatabaseSync(':memory:');
 db.exec('CREATE TABLE maintenance_work_orders('+columns.map(name=>name+' '+(['version','created_at','updated_at'].includes(name)?'INTEGER':'TEXT')).join(',')+');CREATE TABLE staff_notifications(id TEXT PRIMARY KEY,company_id TEXT,user_id TEXT,record_type TEXT,record_id TEXT,record_version INTEGER,message TEXT,created_at INTEGER,read_at INTEGER);');
 const row=columns.map(name=>Object.hasOwn(facts,name)?facts[name]:null);
 db.prepare('INSERT INTO maintenance_work_orders VALUES('+columns.map(()=>'?').join(',')+')').run(...row);
 let sql='',parameters=[];
 const statement=publisher.prepareMaintenanceNotification({prepare(query){sql=query;return{bind(...values){parameters=values;return{sql,values};}};}},key,id,created);
 db.prepare(statement.sql).run(...statement.values);
 const expected=db.prepare('SELECT * FROM staff_notifications').get();
 cases.push({key,created,facts,sql,parameters,expected});db.close();
}
// Original INSERT SELECT on an actually absent Work writes no row. A native
// required source is stricter and refuses to mint a prepared value for absence.
const output={version:1,revision,node:process.version,description:'Actual B0 notification SQL only; no owner permission, PostgreSQL trigger, or release acceptance.',sources:paths.map(path=>({path,sha256:createHash('sha256').update(sources.get(path)).digest('hex')})),cases};
process.stdout.write(JSON.stringify(output,null,2)+'\n');
`;
const output=execFileSync(node,['--input-type=module','-'],{input:script,cwd:root,encoding:'utf8',maxBuffer:2*1024*1024,env:{PATH:process.env.PATH??'/usr/bin:/bin',NODE_ENV:'test',LANG:'en_US.UTF-8',TZ:'UTC'}});
const destination=resolve(outputRoot,'fixtures/collaboration/rust-maintenance-notifications/conformance.v1.json');
if(args.includes('--check')){if(await readFile(destination,'utf8')!==output)throw Error('NOTIFICATION_ORACLE_STALE');}
else{await mkdir(resolve(destination,'..'),{recursive:true});await writeFile(destination,output);}
console.log(JSON.stringify({cases:JSON.parse(output).cases.length,checked:args.includes('--check')}));
