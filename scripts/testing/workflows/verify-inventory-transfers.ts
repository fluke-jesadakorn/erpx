import assert from "node:assert/strict";
import { getLocalProxy } from "../../local/fixtures";
const origin=process.env.ERPX_TEST_BASE_URL??"http://localhost:3100";
if(!["localhost","127.0.0.1"].includes(new URL(origin).hostname))throw new Error("Local verification only.");
const run=crypto.randomUUID().slice(0,8),jars=new Map<string,Map<string,string>>(),contexts=new Map<string,{revision:number;company:string;assignment:string}>();
type Row=Record<string,string|number|null>;type Data=Record<string,unknown>;
function cookie(who:string){return [...(jars.get(who)??new Map())].map(([k,v])=>`${k}=${v}`).join("; ");}
async function api(who:string,path:string,input?:unknown,status:number|number[]=200,phase=crypto.randomUUID()):Promise<Data>{
 const context=contexts.get(who);const r=await fetch(`${origin}${path.startsWith("/")?path:`/api/v1/holdings/acme/${path}`}`,{method:input?"POST":"GET",headers:{Origin:origin,"Content-Type":"application/json","Idempotency-Key":`transfer-${run}-${phase}`,Cookie:cookie(who),...(context?{"x-erpx-context-revision":String(context.revision),"x-erpx-context-company":context.company,"x-erpx-context-assignment":context.assignment}: {})},...(input?{body:JSON.stringify(input)}:{})});
 const jar=jars.get(who)??new Map<string,string>();for(const v of r.headers.getSetCookie()){const pair=v.split(";")[0],i=pair.indexOf("=");jar.set(pair.slice(0,i),pair.slice(i+1));}jars.set(who,jar);
 const body=await r.json() as {data:Data;error?:unknown};assert.ok((Array.isArray(status)?status:[status]).includes(r.status),`${who} ${path}: ${r.status} ${JSON.stringify(body.error)}`);return body.data??{};
}
async function selectWorkRole(who:string,assignment?:string){const active=await api(who,"/api/v1/holdings/acme/assignments/active");const selected=assignment??(active.assignmentId as string|null)??((active.options as Data[])[0]?.id as string|undefined);assert.ok(selected,`No active assignment for ${who}`);const chosen=await api(who,"/api/v1/holdings/acme/assignments/active",{assignmentId:selected,revision:Number(active.revision??0)});contexts.set(who,{revision:Number(chosen.revision),company:"acme-co",assignment:String(chosen.assignmentId)});}
async function assignCountFlow(id:string){let flow=await api("mali",`/api/v1/holdings/acme/flows/stock_count/${id}`);for(const [stepKey,userId] of [["submitted","usr_nisa"],["approved","usr_nisa"]] as const){const step=(flow.steps as Data[]).find(item=>item.key===stepKey);if(!step||step.primary)continue;flow=await api("mali",`/api/v1/holdings/acme/flows/stock_count/${id}`,{stepKey,userId,expectedVersion:Number(flow.version),recordVersion:Number(flow.recordVersion),reason:`Acceptance routing: ${stepKey} inventory review belongs to Finance.`,saveDefault:false});}}
async function assignTransferFlow(id:string){const flow=await api("mali",`/api/v1/holdings/acme/flows/transfer/${id}`);const step=(flow.steps as Data[]).find(item=>item.key==="submitted");if(step&&!step.primary)await api("mali",`/api/v1/holdings/acme/flows/transfer/${id}`,{stepKey:"submitted",userId:"usr_mali",expectedVersion:Number(flow.version),recordVersion:Number(flow.recordVersion),reason:"Acceptance routing: the holding operations owner reviews the receiving branch.",saveDefault:false});}
for(const who of ["owen","nisa","mali","henry","sam"])await api(who,"/api/auth/sign-in/email",{email:`${who}@erpx.dev`,password:`${who[0].toUpperCase()+who.slice(1)}-ERPX-2026!`});
await selectWorkRole("owen");await selectWorkRole("nisa","workflow_controller");await selectWorkRole("mali","asg_mali_owner");await selectWorkRole("henry");
const platform=await getLocalProxy({persist:true,remoteBindings:false});
try{
 const db=platform.env.H001_DB;
 const target=await api("owen","commerce/warehouses",{code:`NY-${run}`,name:`New York installation receiving ${run}`,branchId:"new-york",status:"active"});
 const product=await api("owen","commerce/products",{sku:`MOVE-${run}`,name:"Harbor monitor mounting kit",unit:"kit",price:"25.00",taxBps:0,status:"active"});
 for(const [phase,warehouse,qty,cost]of [["source","warehouse-acme-co","20","12.50"],["target",target.id as string,"5","10.00"]]){
  const c=await api("owen","stock-counts",{warehouseId:warehouse,countDate:"2026-09-07",reason:"Physical inventory of monitor mounting kits",lines:[{productId:product.id,countedQuantity:qty,unitCost:cost}]},201,phase);
  await api("owen",`stock-counts/${c.id}/submit`,{expectedVersion:1});await assignCountFlow(String(c.id));await api("nisa",`stock-counts/${c.id}/approve`,{expectedVersion:2,reason:"Inventory quantity and recorded unit value verified"});await api("nisa",`stock-counts/${c.id}/post`,{expectedVersion:3,reason:"Review Debit / Credit and post the count"});
 }
 const stock=(warehouse:string)=>db.prepare("SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id=? AND product_id=?").bind(warehouse,product.id as string).first();
 const input={sourceWarehouseId:"warehouse-acme-co",targetWarehouseId:target.id,transferDate:"2026-09-08",reason:"Move mounting kits to the installation receiving area",lines:[{productId:product.id,quantity:"4"}]};
 const src=(path="")=>`/api/v1/holdings/acme/branches/bangkok/stock-transfers${path}`,dst=(path="")=>`/api/v1/holdings/acme/branches/new-york/stock-transfers${path}`;
 await api("henry","stock-transfers",input,403,"sales-denied");await api("owen",src(),{...input,targetWarehouseId:input.sourceWarehouseId},422,"same-warehouse");
 await api("owen",src(),{...input,targetWarehouseId:"warehouse-beacon-co"},422,"cross-company");await api("owen",dst(),input,422,"wrong-source-branch");
 await api("owen",src(),{...input,lines:[{productId:product.id,quantity:"21"}]},422,"too-many");await api("owen",src(),{...input,lines:[{productId:product.id,quantity:"0"}]},422,"zero");
 await api("owen",src(),{...input,lines:[input.lines[0],input.lines[0]]},422,"duplicate");
 const t=await api("owen",src(),input,201,"create");assert.deepEqual(await api("owen",src(),input,201,"create"),t);await api("owen",src(),{...input,reason:"changed payload"},409,"create");
 await api("sam",`/api/v1/holdings/northstar/stock-transfers/${t.id}`,undefined,404);
 await api("owen",src(`/${t.id}`),{...input,targetWarehouseId:"warehouse-acme-co-staging",expectedVersion:1},422,"route-locked");
 await api("owen",src(`/${t.id}/submit`),{expectedVersion:1});await api("owen",src(`/${t.id}/post`),{expectedVersion:2,reason:"Own transfer review"},403,"self-review");
 await api("mali",src(`/${t.id}/post`),{expectedVersion:2,reason:"Wrong receiving branch"},403,"wrong-review-branch");await api("nisa",dst(`/${t.id}/post`),{expectedVersion:2,reason:"Branch finance not holding controller"},403,"branch-finance-denied");
 await assignTransferFlow(String(t.id));
 await api("mali",dst(`/${t.id}/reject`),{expectedVersion:2,reason:"Recheck the packing count before receipt confirmation"});
 await api("owen",src(`/${t.id}`),{...input,expectedVersion:3},200,"correct");await api("owen",src(`/${t.id}/submit`),{expectedVersion:4});
 await assignTransferFlow(String(t.id));
 const staleCount=await api("owen","stock-counts",{warehouseId:"warehouse-acme-co",countDate:"2026-09-08",reason:"Concurrent cycle count awaiting transfer",lines:[{productId:product.id,countedQuantity:"19"}]},201,"count");await api("owen",`stock-counts/${staleCount.id}/submit`,{expectedVersion:1});await assignCountFlow(String(staleCount.id));
 const journalBefore=await db.prepare("SELECT COUNT(*) n FROM finance_journals").first<{n:number}>();
 const post=await api("mali",dst(`/${t.id}/post`),{expectedVersion:5,reason:"Received four complete mounting kits and verified the packing count"},200,"post");assert.deepEqual(await api("mali",dst(`/${t.id}/post`),{expectedVersion:5,reason:"Received four complete mounting kits and verified the packing count"},200,"post"),post);
 assert.deepEqual(await stock("warehouse-acme-co"),{quantity_milli:16000,value_minor:20000});assert.deepEqual(await stock(target.id as string),{quantity_milli:9000,value_minor:10000});
 assert.equal((await db.prepare("SELECT COUNT(*) n FROM finance_journals").first<{n:number}>())?.n,journalBefore?.n,"Same-company warehouse movement changes no company GL balance");
 const detail=await api("nisa",`stock-transfers/${t.id}`);assert.equal((detail.decisions as Row[]).length,2);
 await api("nisa",`stock-counts/${staleCount.id}/post`,{expectedVersion:2,reason:"Attempt old snapshot after warehouse transfer"},409,"stale-count-denied");await api("nisa",`stock-counts/${staleCount.id}/reject`,{expectedVersion:2,reason:"Transfer changed the source inventory"});await api("owen",`stock-counts/${staleCount.id}/cancel`,{expectedVersion:3,reason:"Obsolete cycle count cancelled"});
 const a=await api("owen","stock-transfers",{...input,lines:[{productId:product.id,quantity:"10"}]},201,"competing-a"),b=await api("owen","stock-transfers",{...input,lines:[{productId:product.id,quantity:"10"}]},201,"competing-b");
 await api("owen",`stock-transfers/${a.id}/submit`,{expectedVersion:1});await api("owen",`stock-transfers/${b.id}/submit`,{expectedVersion:1});
 await Promise.all([a,b].map(x=>assignTransferFlow(String(x.id))));
 const competing=await Promise.all([a,b].map((x,i)=>api("mali",`stock-transfers/${x.id}/post`,{expectedVersion:2,reason:"Received ten installation kits"},[200,409],`competing-${i}`)));assert.equal(competing.filter(x=>x.id).length,1);
 const loser=[a,b][competing.findIndex(x=>!x.id)];await api("mali",`stock-transfers/${loser.id}/reject`,{expectedVersion:2,reason:"Source stock changed; refresh available quantity"});await api("owen",`stock-transfers/${loser.id}`,{...input,lines:[{productId:product.id,quantity:"5"}],expectedVersion:3});await api("owen",`stock-transfers/${loser.id}/submit`,{expectedVersion:4});await assignTransferFlow(String(loser.id));await api("mali",`stock-transfers/${loser.id}/post`,{expectedVersion:5,reason:"Received five remaining installation kits"});
 assert.deepEqual(await stock("warehouse-acme-co"),{quantity_milli:1000,value_minor:1250});assert.deepEqual(await stock(target.id as string),{quantity_milli:24000,value_minor:28750});
 const last=await api("owen","stock-transfers",{...input,targetWarehouseId:"warehouse-acme-co-staging",lines:[{productId:product.id,quantity:"1"}]},201,"empty-destination");await api("owen",`stock-transfers/${last.id}/submit`,{expectedVersion:1});await assignTransferFlow(String(last.id));await api("mali",`stock-transfers/${last.id}/post`,{expectedVersion:2,reason:"Received final kit in staging warehouse"});
 assert.deepEqual(await stock("warehouse-acme-co"),{quantity_milli:0,value_minor:0});assert.deepEqual(await stock("warehouse-acme-co-staging"),{quantity_milli:1000,value_minor:1250});
 const badMoves=await db.prepare("SELECT transfer_id FROM inventory_transfer_moves GROUP BY transfer_id,product_id HAVING COUNT(*)!=2 OR SUM(quantity_milli)!=0 OR SUM(value_minor)!=0").all();assert.equal(badMoves.results.length,0);
 const total=await db.prepare("SELECT SUM(quantity_milli) qty,SUM(value_minor) value FROM commerce_stock WHERE product_id=?").bind(product.id as string).first();assert.deepEqual(total,{qty:25000,value:30000});
 await api("owen","stock-transfers",{...input,sourceWarehouseId:target.id,targetWarehouseId:"warehouse-acme-co",transferDate:"2026-09-07",lines:[{productId:product.id,quantity:"1"}]},409,"backdated");
 for(const [path,title]of [["stock-transfers","Warehouse transfers"],[`stock-transfers/${t.id}`,"Receipt and review decisions"]]){const r=await fetch(`${origin}/holdings/acme/${path}`,{headers:{Cookie:cookie("owen")}}),html=await r.text();assert.equal(r.status,200);assert.ok(html.includes(title)&&html.includes('data-source="live"'));}
 console.log(`PASS warehouse transfer ${t.id}: scoped source/receiver roles, independent receipt review, corrections, immutable decisions, average-cost preservation, existing/empty destination, competing transfer rollback, count invalidation, backdating denial, balanced movement pairs and unchanged company valuation.`);
}finally{await platform.dispose();}
