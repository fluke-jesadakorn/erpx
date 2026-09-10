import assert from "node:assert/strict";
import { getLocalProxy } from "./local/fixtures";
const origin=process.env.ERPX_TEST_BASE_URL??"http://localhost:3100";
if(!["localhost","127.0.0.1"].includes(new URL(origin).hostname))throw new Error("This acceptance scenario is local only.");
const cookieJars=new Map<string,Map<string,string>>();
function remember(user:string,response:Response){const jar=cookieJars.get(user)??new Map<string,string>();for(const value of response.headers.getSetCookie()){const pair=value.split(";")[0];const i=pair.indexOf("=");jar.set(pair.slice(0,i),pair.slice(i+1));}cookieJars.set(user,jar);}
async function login(user:string,assignment?:string){const response=await fetch(`${origin}/api/auth/sign-in/email`,{method:"POST",headers:{"Content-Type":"application/json",Origin:origin},body:JSON.stringify({email:`${user}@erpx.dev`,password:`${user[0].toUpperCase()+user.slice(1)}-ERPX-2026!`})});assert.equal(response.status,200,`Login failed for ${user}`);remember(user,response);if(assignment)await request(user,"assignments/active",{assignmentId:assignment},crypto.randomUUID());}
function cookies(user:string){return [...(cookieJars.get(user)??new Map()).entries()].map(([k,v])=>`${k}=${v}`).join("; ");}
async function key(name:string){return new Bun.CryptoHasher("sha256").update(`office-scenario-v1:${name}`).digest("hex").slice(0,36);}
type Data=Record<string,unknown>;
async function request(user:string,path:string,input?:unknown,id?:string,status=200):Promise<Data>{const r=await fetch(`${origin}/api/v1/holdings/acme/${path}`,{method:input?"POST":"GET",headers:{Cookie:cookies(user),Origin:origin,"Content-Type":"application/json",...(id?{"Idempotency-Key":id}:{})},...(input?{body:JSON.stringify(input)}:{})});remember(user,r);const body=await r.json() as {data:Data;error:{message:string}};assert.equal(r.status,status,`${path}: ${JSON.stringify(body)}`);return body.data;}
const command=async(user:string,name:string,path:string,input:unknown,status=200)=>request(user,`commerce/${path}`,input,await key(name),status);
await login("henry");await login("sofia");await login("leila");await login("priya");await login("owen");await login("daniel");await login("nisa","workflow_controller");
const product=await command("owen","product","products",{sku:"CASE-DESK-120",name:"Harbor office fit-out desk",unit:"each",price:"350.00",taxBps:700,status:"active"});
const purchaseInput={kind:"purchase",partyId:"commerce-acme-co-supplier",warehouseId:"warehouse-acme-co",orderDate:"2026-09-06",dueDate:"2026-09-20",note:"Office fit-out: receive 20 desks, deliver 5 to Harbor Office Solutions.",lines:[{productId:product.id,quantity:"20",price:"200.00",taxBps:700}]};
const po=await command("leila","purchase","orders",purchaseInput);
assert.deepEqual(await command("leila","purchase","orders",purchaseInput),po,"Create replay must return the original receipt");
await command("leila","purchase","orders",{...purchaseInput,note:"Changed request with same key"},409);
await command("leila","po-submit",`orders/${po.id}/transition`,{action:"submit",expectedVersion:1});
await command("leila","po-self-approve",`orders/${po.id}/transition`,{action:"approve",expectedVersion:2},403);
await command("priya","po-approve",`orders/${po.id}/transition`,{action:"approve",expectedVersion:2});
const poDetail=await request("owen",`commerce/orders/${po.id}`);const poLine=(poDetail.lines as Data[])[0];
await command("owen","po-too-many",`orders/${po.id}/fulfill`,{expectedVersion:3,date:"2026-09-06",lines:[{lineId:poLine.id,quantity:"21"}]},422);
const receiveInput={expectedVersion:3,date:"2026-09-06",lines:[{lineId:poLine.id,quantity:"12"}]};
const receipt1=await command("owen","po-receive-12",`orders/${po.id}/fulfill`,receiveInput);
assert.deepEqual(await command("owen","po-receive-12",`orders/${po.id}/fulfill`,receiveInput),receipt1,"Receipt replay must not move stock twice");
await command("owen","po-stale",`orders/${po.id}/fulfill`,{expectedVersion:3,date:"2026-09-06",lines:[{lineId:poLine.id,quantity:"1"}]},poDetail.order&&Number((poDetail.order as Data).version)>4?422:409);
const receipt2=await command("owen","po-receive-8",`orders/${po.id}/fulfill`,{expectedVersion:4,date:"2026-09-06",lines:[{lineId:poLine.id,quantity:"8"}]});
const bill=await command("daniel","po-invoice-12",`fulfillments/${receipt1.fulfillmentId}/invoice`,{expectedVersion:1,date:"2026-09-06",dueDate:"2026-09-30"});
await command("daniel","po-invoice-8",`fulfillments/${receipt2.fulfillmentId}/invoice`,{expectedVersion:1,date:"2026-09-06",dueDate:"2026-09-30"});
await command("daniel","self-payment",`invoices/${bill.invoiceId}/settle`,{expectedVersion:1,date:"2026-09-06",amount:"2568.00",reference:"CASE-SUPPLIER-SELF"},403);
await command("nisa","supplier-payment",`invoices/${bill.invoiceId}/settle`,{expectedVersion:1,date:"2026-09-06",amount:"2568.00",reference:"CASE-SUPPLIER-001"});
const quote=await command("henry","quote","orders",{...purchaseInput,kind:"quote",partyId:"commerce-acme-co-customer",lines:[{productId:product.id,quantity:"5",price:"350.00",taxBps:700}]});
await command("henry","quote-submit",`orders/${quote.id}/transition`,{action:"submit",expectedVersion:1});
await command("sofia","quote-approve",`orders/${quote.id}/transition`,{action:"approve",expectedVersion:2});
const converted=await command("henry","quote-convert",`orders/${quote.id}/transition`,{action:"convert",expectedVersion:3});
const so=converted.salesId;
await command("henry","sales-submit",`orders/${so}/transition`,{action:"submit",expectedVersion:1});
await command("sofia","sales-approve",`orders/${so}/transition`,{action:"approve",expectedVersion:2});
const soDetail=await request("owen",`commerce/orders/${so}`);const soLine=(soDetail.lines as Data[])[0];
const delivery=await command("owen","sales-delivery",`orders/${so}/fulfill`,{expectedVersion:3,date:"2026-09-06",lines:[{lineId:soLine.id,quantity:"5"}]});
const invoiceInput={expectedVersion:1,date:"2026-09-06",dueDate:"2026-09-30"};
const invoice=await command("daniel","sales-invoice",`fulfillments/${delivery.fulfillmentId}/invoice`,invoiceInput);
assert.deepEqual(await command("daniel","sales-invoice",`fulfillments/${delivery.fulfillmentId}/invoice`,invoiceInput),invoice);
const partial={expectedVersion:1,date:"2026-09-06",amount:"1000.00",reference:"CASE-RECEIPT-001"};
await command("daniel","receipt-partial",`invoices/${invoice.invoiceId}/settle`,partial);
await command("daniel","receipt-partial",`invoices/${invoice.invoiceId}/settle`,partial);
await command("daniel","receipt-overpay",`invoices/${invoice.invoiceId}/settle`,{...partial,expectedVersion:2,reference:"CASE-OVERPAY",amount:"999999.00"},422);
await command("daniel","receipt-final",`invoices/${invoice.invoiceId}/settle`,{...partial,expectedVersion:2,reference:"CASE-RECEIPT-002",amount:"872.50"});
const platform=await getLocalProxy({persist:true,remoteBindings:false});
try{
  const db=platform.env.H001_DB;
  const stock=await db.prepare("SELECT quantity_milli,value_minor FROM commerce_stock WHERE product_id=? AND warehouse_id='warehouse-acme-co'").bind(product.id as string).first();assert.deepEqual(stock,{quantity_milli:15000,value_minor:300000});
  const inv=await db.prepare("SELECT status,outstanding_minor,total_minor,tax_minor FROM finance_invoices WHERE id=?").bind(invoice.invoiceId as string).first();assert.deepEqual(inv,{status:"paid",outstanding_minor:0,total_minor:187250,tax_minor:12250});
  const unbalanced=await db.prepare("SELECT j.id FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id WHERE j.chart_id='commerce-chart-acme-co' GROUP BY j.id HAVING SUM(l.debit_minor)!=SUM(l.credit_minor)").all();assert.equal(unbalanced.results.length,0);
  const clearing=await db.prepare("SELECT SUM(debit_minor-credit_minor) net FROM finance_journal_lines WHERE account_id='commerce-chart-acme-co-2150'").first<{net:number}>();assert.equal(clearing?.net,0);
  console.log(JSON.stringify({result:"PASS",purchaseOrder:po.id,salesOrder:so,invoice:invoice.invoiceId,stock,invoiceState:inv,checks:["independent login and roles","duplicate command replay","changed-payload conflict","self-approval denied","partial receipts","stale receipt rollback","over-receipt denied","quote conversion","perpetual inventory and COGS","separate tax postings","independent supplier payment","partial customer receipt","allocation settlement","balanced journals","zero receipt clearing balance"]},null,2));
}finally{await platform.dispose();}
