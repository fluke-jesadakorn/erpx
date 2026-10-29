/** Development-only exact B0 parts preparation/error-order corpus. */
import {execFileSync} from 'node:child_process';
import {mkdir,readFile,writeFile} from 'node:fs/promises';
import {resolve} from 'node:path';
const root=resolve(import.meta.dir,'../../..');
const args=process.argv.slice(2);
if(args.some(v=>v!=='--check')||args.length>1)throw new Error('PARTS_ORACLE_ARGUMENTS_INVALID');
const node=process.env.ERPX_NODE_BIN??'node';
if(execFileSync(node,['--version'],{encoding:'utf8'}).trim()!=='v26.11.1')throw new Error('PINNED_PARTS_NODE_REQUIRED');
const template=String.raw`// Development-only exact B0 application execution with explicit fact/effect
// ports. This proves control-flow/error order, not database or authority parity.
import {createInventoryConsumptionApplication} from "../../../../packages/server-inventory/src/application/consumption";
import {createJournalPreparation} from "../../../../packages/server-finance/src/application/journal";
import {createCompanyMoney} from "../../../../packages/server-finance/src/application/company-money";
import {createFunctionalFx} from "../../../../packages/server-finance/src/application/functional-fx";
import {createHash} from "node:crypto";
import {readFileSync} from "node:fs";
import {execFileSync} from "node:child_process";
const sourceCommit="a5ed8737a2f05eff1ee9841fda4a094b3926d732";
if(process.version!=="v26.11.1")throw Error("PINNED_NODE_REQUIRED");
const paths=[
 "packages/server-inventory/src/application/consumption.ts",
 "packages/server-finance/src/application/journal.ts",
 "packages/server-finance/src/application/company-money.ts",
 "packages/server-finance/src/application/functional-fx.ts",
 "packages/server-finance/src/domain/finance-policy.ts",
 "packages/server-finance/src/domain/company-money.ts",
 "packages/feature-finance/src/lib/finance-money.ts",
 "packages/feature-finance/src/lib/company-money.ts",
 "packages/feature-commerce/src/lib/commerce-math.ts",
];
const sources=paths.map(path=>{
 const original=execFileSync("git",["show",sourceCommit+":"+path]);
 const current=readFileSync(path);if(!original.equals(current))throw Error("B0_SOURCE_CHANGED:"+path);
 return{path,sha256:createHash("sha256").update(original).digest("hex")};
});
type Scenario={id:string;mapping?:boolean;quote?:boolean;account?:boolean;chart?:boolean;key?:string;kind?:"maintenance"|"field_service";reason?:string;valueMinor?:number;sourceVersion?:number};
const scenarios:Scenario[]=[
 {id:"mapping-invalid-and-no-quote",mapping:false,quote:false},
 {id:"mapping-invalid-with-quote",mapping:false},
 {id:"mapping-valid-and-no-quote",quote:false},
 {id:"account-invalid-and-no-quote",account:false,quote:false},
 {id:"chart-missing-and-no-quote",chart:false,quote:false},
 {id:"separate-captured-times"},
 {id:"outer-key-8",key:"x".repeat(8)},
 {id:"outer-key-174",key:"x".repeat(174)},
 {id:"outer-key-175",key:"x".repeat(175)},
 {id:"outer-key-180",key:"x".repeat(180)},
 {id:"field-service-data-only",kind:"field_service"},
 {id:"original-thai-reason",reason:"เบิกอะไหล่ตามงานที่ตรวจสอบแล้ว"},
 {id:"maximum-safe-cost",valueMinor:Number.MAX_SAFE_INTEGER},
 {id:"maximum-maintenance-thai-reason",reason:"ก".repeat(2000)},
 {id:"maximum-maintenance-astral-reason",reason:"🛠".repeat(2000)},
 {id:"maximum-provisional-source-version-and-no-quote",sourceVersion:9007199254740992,quote:false},
];
const outcomes=[];
for(const scenario of scenarios){
 const events:string[]=[];let now=1800000000000;
 const clock=()=>++now;
 const error=(status:number,code:string,message:string)=>Object.assign(new Error(message),{status,code});
 const rates=[{id:"reviewed-rate",currency:"THB",effectiveDate:"2026-01-01",currencyPerUsd:"35",confirmedBy:"reviewer",confirmedAt:1}];
 const journalPrepare=createJournalPreparation({
  error,now:()=>{events.push("journal-clock");return clock();},
  chart:async()=>scenario.chart===false?null:{id:"chart",base_currency:"THB"},
  period:async()=>{},moneyScale:async()=>2,
  functionalFx:createFunctionalFx({error,rates:async()=>rates}),
  accounts:async(_company,_chart,ids)=>scenario.account===false?[]:ids.map(id=>({id})),
  openingExists:async()=>false,
 });
 let moneyPlan:any;let journalPlan:any;let journalInput:any;let journalSeed:string|undefined;let consumptionArguments:any;
 const money=createCompanyMoney({
  error,now:()=>{events.push("money-clock");return clock();},
  scale:async()=>({scale:2}),rates:async()=>scenario.quote===false?[]:rates,
 }as any);
 const effect=(name:string,args:unknown[])=>({name,args,run(){
  events.push("apply:"+name);
  if(name==="guardFinanceMapping"&&scenario.mapping===false)
   throw error(409,"MAPPING_GUARD_DENIED","Synthetic database mapping assertion denied.");
 }});
 const repository=new Proxy({
  serviceStock:async()=>({quantity_milli:1000,value_minor:scenario.valueMinor??123,version:2,tracking_mode:"none",branch_id:"branch"}),
  servicePostingConfig:async()=>({chart_id:"chart",currency:"THB",base_currency:"THB",inventory_account_id:"inventory",cogs_account_id:"expense"}),
  postingPeriods:async()=>[{id:"period"}],
 },{get(target,name){if(name in target)return target[name as keyof typeof target];return(...args:unknown[])=>{
  if(name==="prepareConsumption")consumptionArguments=args;
  return effect(String(name),args);
 };}});
 const app=createInventoryConsumptionApplication({
  repository:repository as any,clock:{now:()=>{events.push("consumption-clock");return clock();}},
  prepareJournal:async(actor,input,key,status)=>{
   events.push("prepare-journal");journalInput=input;journalSeed=key;journalPlan=await journalPrepare(actor,input,key,status);
   return{result:journalPlan.result,steps:[effect("journal",[])]};
  },
  snapshotMoney:async(actor,input)=>{
   events.push("prepare-money");moneyPlan=await money.snapshotMoney(actor,input);
   return{statement:effect("money",[])};
  },
 });
 const key=scenario.key??"parts-owner-command";
 try{
  const result=await app.prepareInventoryConsumption({companyId:"company",userId:"operator"}as any,{
   key,sourceKind:scenario.kind??"maintenance",sourceId:"work",sourceVersion:scenario.sourceVersion??3,branchId:"branch",departmentId:null,
   warehouseId:"warehouse",productId:"product",quantityMilli:1000,entryDate:"2026-01-02",
   reason:scenario.reason??"Recorded original source",allocations:[],
  });
  for(const step of result.steps as any[])step.run();
  outcomes.push({id:scenario.id,ownerKey:key,kind:scenario.kind??"maintenance",reason:scenario.reason??"Recorded original source",quantityMilli:1000,valueMinor:scenario.valueMinor??123,journalInput,journalSeed,events,journalId:result.journalId,consumptionId:result.id,
   journalIdBytes:Buffer.byteLength(result.journalId),ownerKeyBytes:Buffer.byteLength(key),
   consumptionCreatedAt:consumptionArguments.at(-1),journalCreatedAt:journalPlan.now,moneyCapturedAt:moneyPlan.capturedAt});
 }catch(e:any){outcomes.push({id:scenario.id,sourceVersion:scenario.sourceVersion??3,events,error:{status:e.status??null,code:e.code??null,message:e.message}});}
}
process.stdout.write(JSON.stringify({sourceCommit,node:process.version,sources,
 observation:"Actual B0 applications and scalar helpers; explicit pure fact/deferred-effect ports only, no PostgreSQL or authority acceptance",outcomes},null,2)+"\n");
`;
const entrySource=template.replace(/from "(\.\.\/\.\.\/\.\.\/\.\.\/packages\/[^"\n]+)"/g,(_all:string,path:string)=>'from '+JSON.stringify(resolve(root,path.slice('../../../../'.length))));
const directory=resolve(root,'.scratch/dev/finance-parts-oracle');await mkdir(directory,{recursive:true,mode:0o700});
const entry=resolve(directory,'entry.ts');await writeFile(entry,entrySource,{mode:0o600});
const result=await Bun.build({entrypoints:[entry],target:'node',format:'esm',outdir:directory,naming:'oracle.mjs'});
if(!result.success)throw new Error('PARTS_ORACLE_BUILD_FAILED:'+result.logs.join('\n'));
const output=execFileSync(node,[resolve(directory,'oracle.mjs')],{cwd:root,encoding:'utf8',maxBuffer:2*1024*1024,env:{PATH:process.env.PATH??'/usr/bin:/bin',NODE_ENV:'test',LANG:'en_US.UTF-8',TZ:'UTC'}});
const parsed:unknown=JSON.parse(output);if(!parsed||typeof parsed!=='object'||!('outcomes'in parsed)||!Array.isArray(parsed.outcomes))throw new Error('PARTS_ORACLE_OUTPUT_INVALID');
const destination=resolve(root,'fixtures/finance/rust-parts-draft/conformance.v1.json');
if(args.includes('--check')){if(await readFile(destination,'utf8')!==output)throw new Error('PARTS_ORACLE_STALE');}
else{await mkdir(resolve(destination,'..'),{recursive:true});await writeFile(destination,output);}
console.log(JSON.stringify({cases:parsed.outcomes.length,checked:args.includes('--check')}));
