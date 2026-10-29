// Execute the retained TypeScript, not a reimplementation. Synthetic ports only.
import {createRequire} from 'node:module';
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {readFileSync, writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {deepStrictEqual} from 'node:assert';
const require=createRequire(resolve('package.json')), ts=require('typescript'), zod=require('zod');
const revision='ee8700f28fb39aafc8a19a571b1e9b95e79cc7c2';
const paths=['packages/server-maintenance/src/domain/work.ts','packages/server-maintenance/src/application/work.ts','packages/server-maintenance/src/adapters/work.ts','packages/server-shared/src/domain/consumption-lot.ts'];
const sources=new Map(paths.map(p=>[p,execFileSync('git',['show',`${revision}:${p}`],{encoding:'utf8'})]));
for(const p of paths) deepStrictEqual(readFileSync(p,'utf8'),sources.get(p),`source differs from retained ${revision}: ${p}`);
function compile(p,bindings){const m={exports:{}};const code=ts.transpileModule(sources.get(p),{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.CommonJS}}).outputText;new Function('module','exports','require',code)(m,m.exports,n=>{if(n==='zod')return zod;if(Object.hasOwn(bindings,n))return bindings[n];throw Error('Unreviewed dependency: '+n);});return m.exports;}
const lot=compile(paths[3],{}),domain=compile(paths[0],{'@erpx/server-shared/index':lot}),application=compile(paths[1],{'@erpx/server-maintenance/domain/work':domain});
const source={id:'work',status:'open',owner_id:'actor',owner_eligible:1,can_review:0,version:7,scheduled_at:null,downtime_started_at:null};
const input={action:'schedule',expectedVersion:7,ownerId:'target',scheduledAt:1800000000000,reason:'  Schedule work  ',unknown:'stripped'};
const cases=[
 ['permission-before-parse',null,null,{write:false}],['invalid-before-replay',{...input,ownerId:''},null,{replay:true}],
 ['replay-before-missing',input,null,{replay:true}],['replay-before-ineligible-owner-state',input,{...source,owner_eligible:0,status:'completed'},{replay:true}],
 ['missing',input,null,{}],['owner-before-state',input,{...source,owner_eligible:0,status:'completed'},{}],['state-before-clock',input,{...source,status:'completed'},{}],
 ['ineligible-owner',input,{...source,owner_eligible:0},{}],['reviewer-not-eligible',input,{...source,owner_id:'other',can_review:0},{review:true}],
 ['eligible-reviewer',input,{...source,owner_id:'other',owner_eligible:0,can_review:1},{review:true}],['scheduled-source',input,{...source,status:'scheduled'},{}],
 ['stale-version-reaches-change',{...input,expectedVersion:6},source,{}],['stale-version-replay',{...input,expectedVersion:6},null,{replay:true}],
 ['past-positive-time',{...input,scheduledAt:1},source,{}],['zero-time',{...input,scheduledAt:0},source,{}],
 ['owner-space',{...input,ownerId:' '},source,{}],['owner-long',{...input,ownerId:'x'.repeat(1025)},source,{}],
 ['owner-control',{...input,ownerId:'a\u0000\n\tb'},source,{}],['owner-non-bmp',{...input,ownerId:'owner-🙂'},source,{}],
 ['owner-empty',{...input,ownerId:''},source,{}],['version-zero',{...input,expectedVersion:0},source,{}],
];
async function observe(raw,row,{write=true,review=false,replay=false}={}){const calls=[];const repository={replay:async c=>{calls.push(['replay',c]);return replay?{id:'work',version:8,status:'scheduled'}:null},work:async id=>{calls.push(['work',id]);return row},change:async(c,w,state,now)=>{calls.push(['change',c,w.id,state,now]);return {id:'work',version:c.input.expectedVersion+1,status:state.status}}};const app=application.createMaintenanceApplication({actorId:'actor',authorize:w=>{calls.push(['authorize',!!w]);if(w&&!write)throw Object.assign(Error('denied'),{status:403,code:'PERMISSION_REQUIRED'})},canReview:()=>review,canWrite:()=>write,canRead:()=>true,clock:{now:()=>{calls.push(['clock']);return 1800000000100}},repository:async()=>{calls.push(['repository']);return repository},error:(status,code,message)=>Object.assign(Error(message),{status,code})});try{return{result:await app.changeMaintenanceWork('work',raw,'schedule-key'),calls}}catch(e){return{error:{name:e.name,status:e.status??null,code:e.code??null,issues:e.issues?.map(i=>({code:i.code,path:i.path}))??null},calls}}}
const rows=[];for(const [id,raw,row,options] of cases)rows.push({id,input:raw,source:row,options,expected:await observe(raw,row,options)});
const zodRoot=resolve(require.resolve('zod/package.json'),'..');
const zodSources=['package.json','v4/core/checks.js','v4/core/util.js','v4/core/schemas.js'].map(path=>({path,sha256:createHash('sha256').update(readFileSync(resolve(zodRoot,path))).digest('hex')}));
const output={version:1,acceptance:false,description:'Retained TypeScript application and schema with synthetic repository; no PostgreSQL, target eligibility, transaction or receipt fingerprint proof.',revision,node:process.version,zod:{version:require('zod/package.json').version,sources:zodSources},sources:paths.map(path=>({path,sha256:createHash('sha256').update(sources.get(path)).digest('hex')})),rows};
const target='fixtures/maintenance/rust-schedule/application.v1.json';
if(process.env.ERPX_UPDATE_SCHEDULE_ORACLE==='1')writeFileSync(target,JSON.stringify(output,null,2)+'\n');
else deepStrictEqual(output,JSON.parse(readFileSync(target,'utf8')));
console.log(`B0 scheduling: ${rows.length} cases match (${process.version})`);
