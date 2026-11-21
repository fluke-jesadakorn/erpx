/** Development-only: execute exact retained B0 helpers/application on pinned Node. */
import {createHash} from "node:crypto";
import {execFileSync} from "node:child_process";
import {mkdir,readFile,writeFile} from "node:fs/promises";
import {resolve} from "node:path";
const sourceCommit="a5ed8737a2f05eff1ee9841fda4a094b3926d732";
const paths=["packages/server-shared/src/domain/error.ts","packages/feature-finance/src/lib/finance-money.ts","packages/feature-commerce/src/lib/commerce-math.ts","packages/server-inventory/src/domain/landed-costs.ts","packages/server-inventory/src/domain/transit.ts","packages/server-inventory/src/domain/reorder-quantity.ts","packages/server-inventory/src/application/consumption.ts"];
const bytes=paths.map(path=>execFileSync("git",["show",`${sourceCommit}:${path}`]));
const sources=paths.map((path,i)=>({path,sha256:createHash("sha256").update(bytes[i]!).digest("hex")}));
const texts=bytes.map(b=>b.toString("utf8"));
const node=process.env.ERPX_NODE_BIN??"node";
if(execFileSync(node,["--version"],{encoding:"utf8"}).trim()!=="v26.11.1")throw new Error("PINNED_INVENTORY_NODE_REQUIRED");
function slice(index:number,start:string,end?:string){const text=texts[index]!;const a=text.indexOf(start),b=end?text.indexOf(end,a):text.length;if(a<0||b<=a)throw new Error("INVENTORY_ORACLE_SOURCE_BOUNDARY_CHANGED");return text.slice(a,b);}
const declarations=[texts[0],texts[1],slice(2,"export function quantityMilli"),slice(3,"export function allocateLandedCost"),slice(4,"export function dateAfterDispatch","export function transitWorkItem"),slice(5,"export function milliQuantity"),slice(6,"export function createInventoryConsumptionApplication")].join("\n");
const directory=resolve(".scratch/dev/inventory-valuation-oracle");await mkdir(directory,{recursive:true,mode:0o700});
const entry=resolve(directory,"entry.ts");
await writeFile(entry,`${declarations}
import {readFileSync} from "node:fs";
const input=JSON.parse(readFileSync(0,"utf8"));
function outcome(run){try{return {value:run()};}catch(e){return {error:{status:e.status??null,code:e.code??null,message:e.message}};}}
const allocations=input.allocations.map(row=>({...row,expected:outcome(()=>allocateLandedCost(row.total,row.weights))}));
const transit=input.transit.map(row=>({...row,expected:outcome(()=>quantityAndValue(row.line,row.quantity))}));
const dates=input.dates.map(row=>({...row,expected:outcome(()=>{dateAfterDispatch(row.document,row.date);return true;})}));
const reorder=input.reorder.map(row=>({...row,expected:outcome(()=>milliQuantity(row.input))}));
const consumption=[];
for(const row of input.consumption){
 const queries=[];const handlers={serviceStock:()=>{queries.push("stock");return row.stock;},servicePostingConfig:()=>{queries.push("config");return row.config;},postingPeriods:()=>{queries.push("periods");return row.periods;},serviceLot:id=>{queries.push("lot:"+id);return row.lots[id]??null;}};
 const repository=new Proxy(handlers,{get:(target,name)=>target[name]??((...args)=>({effect:name,args}))});
 const app=createInventoryConsumptionApplication({repository,clock:{now:()=>1800000000000},prepareJournal:async()=>({result:{id:"synthetic-journal"},steps:[]}),snapshotMoney:async()=>({statement:{effect:"money"}})});
 try{const result=await app.prepareInventoryConsumption({userId:"operator",companyId:"company"},row.input);consumption.push({...row,queries,expected:{value:{valueMinor:result.valueMinor,currency:result.currency,lots:result.lots}}});}
 catch(e){consumption.push({...row,queries,expected:{error:{status:e.status??null,code:e.code??null,message:e.message}}});}
}
process.stdout.write(JSON.stringify({node:process.version,allocations,transit,dates,reorder,consumption}));
`,{mode:0o600});
const built=await Bun.build({entrypoints:[entry],target:"node",format:"esm",outdir:directory,naming:"oracle.mjs"});if(!built.success)throw new Error("INVENTORY_ORACLE_BUILD_FAILED:"+built.logs.join("\n"));
const allocations=[];for(const total of [0,1,2,3,5,99,100,101,9007199254740991])for(const weights of [[],[0],[1],[1,1],[1,1,1],[0,1,0,1],[1,2,3],[333,333,334],[9007199254740990,1]])allocations.push({total,weights});
const transit=[];for(const quantity_milli of [1,3,1000,10000])for(const value_minor of [1,5,101,9007199254740991])for(const used of [0,1,2]){const received_milli=Math.min(used,quantity_milli);for(const quantity of [0,1,quantity_milli-received_milli,quantity_milli+1])transit.push({line:{quantity_milli,value_minor,received_milli,lost_milli:0,pending_loss_milli:0,settled_value_minor:0},quantity});}
transit.push(...[0,1,2,3,4].map(quantity=>({line:{quantity_milli:10,value_minor:101,received_milli:3,lost_milli:2,pending_loss_milli:2,settled_value_minor:51},quantity})));
const dates=["awaiting_dispatch","in_transit","completed","cancelled"].flatMap(status=>["2026-01-01","2026-01-02","2026-01-03"].map(date=>({document:{status,dispatch_date:"2026-01-02"},date})));
const reorder=["0","0.000","0.001","1","0001.230","999999999999.999","12.1","12.01","12.001"].map(input=>({input}));
const config={chart_id:"chart",currency:"THB",base_currency:"THB",inventory_account_id:"inventory-account",cogs_account_id:"expense-account"};
const stock={quantity_milli:10000,value_minor:101,version:2,tracking_mode:"none",branch_id:"branch"};
const input={key:"inventory:test:consume",sourceKind:"maintenance",sourceId:"source-work",sourceVersion:1,branchId:"branch",departmentId:null,warehouseId:"warehouse",productId:"product",quantityMilli:3333,entryDate:"2026-01-02",reason:"Recorded spare consumption",allocations:[]};
const base={stock,config,periods:[{id:"period"}],lots:{},input};
const consumption=[{id:"untracked-partial",...base},{id:"untracked-full",...base,input:{...input,quantityMilli:10000}},{id:"no-stock",...base,stock:null},{id:"insufficient-stock",...base,input:{...input,quantityMilli:10001}},{id:"no-config",...base,config:null},{id:"foreign-functional-currency",...base,config:{...config,base_currency:"USD"}},{id:"no-period",...base,periods:[]},{id:"ambiguous-periods",...base,periods:[{id:"a"},{id:"b"}]},{id:"zero-value",...base,stock:{...stock,value_minor:0}},{id:"untracked-lot-rejected",...base,input:{...input,allocations:[{lotId:"lot-a",quantityMilli:3333,balanceVersion:1}]}}];
const lot={quantity_milli:6000,value_minor:61,version:3,tracking_mode:"lot",expiry_date:null,status:"active"};
const tracked={...base,stock:{...stock,tracking_mode:"lot"},input:{...input,quantityMilli:3000,allocations:[{lotId:"lot-a",quantityMilli:3000,balanceVersion:3}]},lots:{"lot-a":lot}};
consumption.push({id:"lot-partial",...tracked},{id:"lot-full",...tracked,input:{...tracked.input,quantityMilli:6000,allocations:[{lotId:"lot-a",quantityMilli:6000,balanceVersion:3}]}},{id:"no-allocations",...tracked,input:{...tracked.input,allocations:[]}},{id:"quantity-mismatch",...tracked,input:{...tracked.input,quantityMilli:3001}},{id:"duplicate-allocation",...tracked,input:{...tracked.input,quantityMilli:6000,allocations:[...tracked.input.allocations,...tracked.input.allocations]}},{id:"lot-missing",...tracked,lots:{}},...[{version:4},{status:"depleted"},{quantity_milli:2999},{expiry_date:"2026-01-01"},{value_minor:0},{expiry_date:"2026-01-02"}].map((change,index)=>({id:"lot-change-"+index,...tracked,lots:{"lot-a":{...lot,...change}}})),{id:"two-ordered-lots",...tracked,input:{...tracked.input,quantityMilli:6000,allocations:[...tracked.input.allocations,{lotId:"lot-b",quantityMilli:3000,balanceVersion:2}]},lots:{"lot-a":lot,"lot-b":{...lot,value_minor:41,version:2}}});
for(const quantityMilli of [1,999,1000,2000])consumption.push({id:"serial-"+quantityMilli,...tracked,stock:{...stock,tracking_mode:"serial"},input:{...tracked.input,quantityMilli,allocations:[{lotId:"lot-a",quantityMilli,balanceVersion:3}]},lots:{"lot-a":{...lot,tracking_mode:"serial",quantity_milli:2000}}});
for(const sourceKind of ["maintenance","field_service"])for(const value_minor of [1,2,3,999999999999,9007199254740991])for(const quantityMilli of [1,3333,10000])consumption.push({id:`cost-${sourceKind}-${value_minor}-${quantityMilli}`,...base,stock:{...stock,value_minor},input:{...input,sourceKind,quantityMilli}});
const result=JSON.parse(execFileSync(node,[resolve(directory,"oracle.mjs")],{input:JSON.stringify({allocations,transit,dates,reorder,consumption}),encoding:"utf8",maxBuffer:16*1024*1024}));
const output=JSON.stringify({version:1,sourceCommit,sources,oracle:"Exact B0 helper and application function slices; Node 26.11.1; synthetic planning ports only; no PostgreSQL/authority acceptance",...result},null,2)+"\n";
const destination=resolve("fixtures/inventory/rust-valuation/conformance.v1.json");
if(process.argv.includes("--check")){if(await readFile(destination,"utf8")!==output)throw new Error("INVENTORY_ORACLE_STALE");}else{await mkdir(resolve("fixtures/inventory/rust-valuation"),{recursive:true});await writeFile(destination,output);}
console.log(JSON.stringify({allocations:allocations.length,transit:transit.length,dates:dates.length,reorder:reorder.length,consumption:consumption.length,sourceFiles:sources.length,checked:process.argv.includes("--check")}));
