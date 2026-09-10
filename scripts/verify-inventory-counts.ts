import assert from "node:assert/strict";
import { getLocalProxy } from "./local/fixtures";
const origin=process.env.ERPX_TEST_BASE_URL??"http://localhost:3100";
if(!["localhost","127.0.0.1"].includes(new URL(origin).hostname))throw new Error("Local verification only.");
const run=crypto.randomUUID().slice(0,8),jars=new Map<string,Map<string,string>>();
type Row=Record<string,string|number|null>;type Data=Record<string,unknown>;
function cookie(who:string){return [...(jars.get(who)??new Map())].map(([k,v])=>`${k}=${v}`).join("; ");}
async function api(who:string,path:string,input?:unknown,status:number|number[]=200,phase=crypto.randomUUID()):Promise<Data>{
 const r=await fetch(`${origin}${path.startsWith("/")?path:`/api/v1/holdings/acme/${path}`}`,{method:input?"POST":"GET",headers:{Origin:origin,"Content-Type":"application/json","Idempotency-Key":`inventory-${run}-${phase}`,Cookie:cookie(who)},...(input?{body:JSON.stringify(input)}:{})});
 const jar=jars.get(who)??new Map<string,string>();for(const v of r.headers.getSetCookie()){const pair=v.split(";")[0],i=pair.indexOf("=");jar.set(pair.slice(0,i),pair.slice(i+1));}jars.set(who,jar);
 const body=await r.json() as {data:Data;error?:unknown};assert.ok((Array.isArray(status)?status:[status]).includes(r.status),`${who} ${path}: ${r.status} ${JSON.stringify(body.error)}`);return body.data??{};
}
for(const who of ["owen","nisa","henry","sofia","sam","noah"])await api(who,"/api/auth/sign-in/email",{email:`${who}@erpx.dev`,password:`${who[0].toUpperCase()+who.slice(1)}-ERPX-2026!`});
await api("nisa","assignments/active",{assignmentId:"workflow_controller"});
const platform=await getLocalProxy({persist:true,remoteBindings:false});
try{
 const db=platform.env.H001_DB;
 const product=await api("owen","commerce/products",{sku:`COUNT-${run}`,name:"Harbor installation cable kit",unit:"kit",price:"40.00",taxBps:0,status:"active"});
 const draft={warehouseId:"warehouse-acme-co",countDate:"2026-09-07",reason:"Opening physical count of installation cable kits",lines:[{productId:product.id,countedQuantity:"10",unitCost:"20.00"}]};
 await api("henry","stock-counts",draft,403,"sales-cannot-count");
 await api("noah","/api/v1/holdings/acme/branches/new-york/stock-counts",undefined,403);
 await api("owen","stock-counts",{...draft,warehouseId:"warehouse-beacon-co"},422,"wrong-company-warehouse");
 await api("owen","stock-counts",{...draft,lines:[{productId:"commerce-beacon-co-desk",countedQuantity:"1",unitCost:"20"}]},422,"wrong-company-product");
 await api("owen","stock-counts",{...draft,lines:[{productId:product.id,countedQuantity:"-1"}]},422,"negative-count");
 await api("owen","stock-counts",{...draft,lines:[{productId:product.id,countedQuantity:"10"}]},422,"no-cost");
 await api("owen","stock-counts",{...draft,lines:[draft.lines[0],draft.lines[0]]},422,"duplicate-product");
 const opening=await api("owen","stock-counts",draft,201,"opening");const id=String(opening.id);
 assert.deepEqual(await api("owen","stock-counts",draft,201,"opening"),opening);
 await api("owen","stock-counts",{...draft,reason:"Changed data"},409,"opening");
 await api("sam",`/api/v1/holdings/northstar/stock-counts/${id}`,undefined,404);
 await api("owen",`stock-counts/${id}/submit`,{expectedVersion:1},200,"opening-submit");
 await api("owen",`stock-counts/${id}`,{...draft,expectedVersion:2},403,"submitted-locked");
 await api("owen",`stock-counts/${id}/post`,{expectedVersion:2,reason:"Self approval attempted"},403,"self-post");
 await api("nisa",`stock-counts/${id}/post`,{expectedVersion:1,reason:"Review first counted stock"},409,"stale-count");
 const posted=await api("nisa",`stock-counts/${id}/post`,{expectedVersion:2,reason:"Physical quantity and acquisition value independently verified"},200,"opening-post");
 assert.deepEqual(await api("nisa",`stock-counts/${id}/post`,{expectedVersion:2,reason:"Physical quantity and acquisition value independently verified"},200,"opening-post"),posted);
 const stock=()=>db.prepare("SELECT quantity_milli,value_minor FROM commerce_stock WHERE warehouse_id='warehouse-acme-co' AND product_id=?").bind(product.id as string).first();
 assert.deepEqual(await stock(),{quantity_milli:10000,value_minor:20000});
 const journal=await db.prepare("SELECT a.code,l.debit_minor,l.credit_minor FROM finance_journal_lines l JOIN finance_accounts a ON a.id=l.account_id WHERE journal_id=? ORDER BY a.code").bind(posted.journalId as string).all();assert.deepEqual(journal.results,[{code:"1300",debit_minor:20000,credit_minor:0},{code:"5400",debit_minor:0,credit_minor:20000}]);
 await api("owen",`stock-counts/${id}/cancel`,{expectedVersion:3,reason:"Posted history cannot be cancelled"},403,"posted-locked");
 const input=(qty:string,date="2026-09-08")=>({...draft,countDate:date,reason:"Cycle count after warehouse shelf inspection",lines:[{productId:product.id,countedQuantity:qty,unitCost:"999.00"}]});
 const shrink=await api("owen","stock-counts",input("8"),201,"shrink");
 await api("owen",`stock-counts/${shrink.id}/submit`,{expectedVersion:1},200,"shrink-submit");
 for(const path of ["home","work"]){const r=await fetch(`${origin}/holdings/acme/${path}`,{headers:{Cookie:cookie("nisa")}}),html=await r.text();assert.equal(r.status,200);assert.ok(html.includes(`/stock-counts/${shrink.id}`),`${path} exposes pending independent review`);}
 await api("nisa",`stock-counts/${shrink.id}/post`,{expectedVersion:2,reason:"Two damaged installation kits removed after count"},200,"shrink-post");
 assert.deepEqual(await stock(),{quantity_milli:8000,value_minor:16000},"Existing stock retains average cost even when caller supplies another rate");
 const a=await api("owen","stock-counts",input("7"),201,"parallel-a"),b=await api("owen","stock-counts",input("6"),201,"parallel-b");
 await api("owen",`stock-counts/${a.id}/submit`,{expectedVersion:1});await api("owen",`stock-counts/${b.id}/submit`,{expectedVersion:1});
 const concurrent=await Promise.all([a,b].map((c,i)=>api("nisa",`stock-counts/${c.id}/post`,{expectedVersion:2,reason:"Independent review of concurrent warehouse count"},[200,409],`concurrent-${i}`)));
 assert.equal(concurrent.filter(r=>r.journalId).length,1,"Only one count with the same stock snapshot can commit");
 const loser=[a,b][concurrent.findIndex(r=>!r.journalId)];
 const noMoves=await db.prepare("SELECT COUNT(*) n FROM inventory_adjustment_moves WHERE count_id=?").bind(loser.id as string).first<{n:number}>();assert.equal(noMoves?.n,0,"Failed post rolls back stock and journal");
 await api("nisa",`stock-counts/${loser.id}/reject`,{expectedVersion:2,reason:"Stock moved while this count was awaiting review; recount required"},200,"return-stale");
 await api("owen",`stock-counts/${loser.id}`,{...input("5"),expectedVersion:3},200,"recount");
 await api("owen",`stock-counts/${loser.id}/submit`,{expectedVersion:4},200,"recount-submit");
 await api("nisa",`stock-counts/${loser.id}/post`,{expectedVersion:5,reason:"Recount and updated valuation independently verified"},200,"recount-post");
 const detail=await api("nisa",`stock-counts/${loser.id}`);assert.equal((detail.decisions as Row[]).length,2);assert.ok((detail.decisions as Row[]).some(d=>String(d.reason).includes("recount required")));
 assert.deepEqual(await stock(),{quantity_milli:5000,value_minor:10000});
 const so=await api("henry","commerce/orders",{kind:"sales",partyId:"commerce-acme-co-customer",warehouseId:"warehouse-acme-co",orderDate:"2026-09-07",dueDate:"2026-09-20",note:"Deliver installation kits from counted stock",lines:[{productId:product.id,quantity:"2",price:"40",taxBps:0}]},200,"sale");
 await api("henry",`commerce/orders/${so.id}/transition`,{action:"submit",expectedVersion:1});await api("sofia",`commerce/orders/${so.id}/transition`,{action:"approve",expectedVersion:2});
 const salesDetail=await api("owen",`commerce/orders/${so.id}`),line=(salesDetail.lines as Row[])[0];
 await api("owen",`commerce/orders/${so.id}/fulfill`,{expectedVersion:3,date:"2026-09-07",lines:[{lineId:line.id,quantity:"2"}]},409,"backdated-delivery");
 assert.deepEqual(await stock(),{quantity_milli:5000,value_minor:10000});
 await api("owen",`commerce/orders/${so.id}/fulfill`,{expectedVersion:3,date:"2026-09-09",lines:[{lineId:line.id,quantity:"2"}]},200,"delivery");
 assert.deepEqual(await stock(),{quantity_milli:3000,value_minor:6000});
 await api("owen","stock-counts",input("2","2026-09-08"),409,"backdated-count");
 for(const [phase,qty,date,cost] of [["zero","0","2026-09-10",""],["restart","2","2026-09-11","25.00"],["no-change","2","2026-09-11",""]]){
  const c=await api("owen","stock-counts",{...input(qty,date),lines:[{productId:product.id,countedQuantity:qty,...(cost?{unitCost:cost}:{})}]},201,phase);
  await api("owen",`stock-counts/${c.id}/submit`,{expectedVersion:1});const result=await api("nisa",`stock-counts/${c.id}/post`,{expectedVersion:2,reason:"Count quantity and valuation checked independently"});
  if(phase==="zero")assert.deepEqual(await stock(),{quantity_milli:0,value_minor:0});
  if(phase==="no-change")assert.equal(result.journalId,null,"A no-difference count does not create an empty journal");
 }
 const closed=await api("owen","stock-counts",input("2","2030-01-01"),201,"closed-period");await api("owen",`stock-counts/${closed.id}/submit`,{expectedVersion:1});
 await api("nisa",`stock-counts/${closed.id}/post`,{expectedVersion:2,reason:"No available accounting period"},409,"closed-post");
 await api("nisa",`stock-counts/${closed.id}/reject`,{expectedVersion:2,reason:"Accounting period unavailable"});await api("owen",`stock-counts/${closed.id}/cancel`,{expectedVersion:3,reason:"Future count cancelled after verification"});
 const ledger=await db.prepare("SELECT SUM(quantity_milli) quantity_milli,SUM(value_minor) value_minor FROM inventory_ledger WHERE product_id=?").bind(product.id as string).first();assert.deepEqual(await stock(),ledger);assert.deepEqual(ledger,{quantity_milli:2000,value_minor:5000});
 const gl=await db.prepare("SELECT SUM(l.debit_minor-l.credit_minor) balance FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE l.account_id='commerce-chart-acme-co-1300' AND (j.source_id IN (SELECT id FROM inventory_counts WHERE id IN (SELECT count_id FROM inventory_count_lines WHERE product_id=?)) OR j.source_id IN (SELECT f.id FROM commerce_fulfillments f WHERE f.order_id=?))").bind(product.id as string,so.id as string).first<{balance:number}>();assert.equal(gl?.balance,5000,"Inventory GL agrees with stock valuation across counts and delivery");
 for(const path of ["stock-counts",`stock-counts/${id}`,"commerce?view=stock"]){const r=await fetch(`${origin}/holdings/acme/${path}`,{headers:{Cookie:cookie("owen")}});assert.equal(r.status,200);assert.ok((await r.text()).includes('data-source="live"'));}
 console.log(`PASS inventory ${product.id}: opening count, average-cost loss, immutable decisions, independent posting, concurrent stale-count rollback, sale/COGS, backdate protection, zero and no-change counts, period enforcement, work visibility and inventory/GL reconciliation.`);
}finally{await platform.dispose();}
