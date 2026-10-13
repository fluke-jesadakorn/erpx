import assert from "node:assert/strict";
import { getLocalProxy } from "../../local/fixtures";
const origin=process.env.ERPX_TEST_BASE_URL??"http://localhost:3100";
if(!["localhost","127.0.0.1"].includes(new URL(origin).hostname))throw new Error("This acceptance scenario is local only.");
const cookieJars=new Map<string,Map<string,string>>();
const workContexts=new Map<string,{revision:number;company:string;assignment:string}>();
const scenario=crypto.randomUUID();
function remember(user:string,response:Response){const jar=cookieJars.get(user)??new Map<string,string>();for(const value of response.headers.getSetCookie()){const pair=value.split(";")[0];const i=pair.indexOf("=");jar.set(pair.slice(0,i),pair.slice(i+1));}cookieJars.set(user,jar);}
async function login(user:string,assignment?:string){const response=await fetch(`${origin}/api/auth/sign-in/email`,{method:"POST",headers:{"Content-Type":"application/json",Origin:origin},body:JSON.stringify({email:`${user}@erpx.dev`,password:`${user[0].toUpperCase()+user.slice(1)}-ERPX-2026!`})});assert.equal(response.status,200,`Login failed for ${user}`);remember(user,response);const active=await request(user,"assignments/active");const selected=assignment??(active.assignmentId as string|null)??((active.options as Data[])[0]?.id as string|undefined);assert.ok(selected,`No active assignment for ${user}`);const chosen=await request(user,"assignments/active",{assignmentId:selected,revision:Number(active.revision??0)},crypto.randomUUID());workContexts.set(user,{revision:Number(chosen.revision),company:"acme-co",assignment:String(chosen.assignmentId)});}
async function switchAssignment(user:string,assignment:string){const active=await request(user,"assignments/active");const chosen=await request(user,"assignments/active",{assignmentId:assignment,revision:Number(active.revision??0)},crypto.randomUUID());workContexts.set(user,{revision:Number(chosen.revision),company:"acme-co",assignment:String(chosen.assignmentId)});}
function cookies(user:string){return [...(cookieJars.get(user)??new Map()).entries()].map(([k,v])=>`${k}=${v}`).join("; ");}
async function key(name:string){return new Bun.CryptoHasher("sha256").update(`office-scenario-v1:${scenario}:${name}`).digest("hex").slice(0,36);}
type Data=Record<string,unknown>;
const validEvidencePdf=Uint8Array.from(Buffer.from("JVBERi0xLjQKMSAwIG9iago8PCAvVHlwZSAvQ2F0YWxvZyAvUGFnZXMgMiAwIFIgPj4KZW5kb2JqCjIgMCBvYmoKPDwgL1R5cGUgL1BhZ2VzIC9LaWRzIFszIDAgUl0gL0NvdW50IDEgPj4KZW5kb2JqCjMgMCBvYmoKPDwgL1R5cGUgL1BhZ2UgL1BhcmVudCAyIDAgUiAvTWVkaWFCb3ggWzAgMCA2MTIgNzkyXSAvUmVzb3VyY2VzIDw8IC9Gb250IDw8IC9GMSA0IDAgUiA+PiA+PiAvQ29udGVudHMgNSAwIFIgPj4KZW5kb2JqCjQgMCBvYmoKPDwgL1R5cGUgL0ZvbnQgL1N1YnR5cGUgL1R5cGUxIC9CYXNlRm9udCAvSGVsdmV0aWNhID4+CmVuZG9iago1IDAgb2JqCjw8IC9MZW5ndGggNDkgPj4Kc3RyZWFtCkJUIC9GMSAxNiBUZiA3MiA3MjAgVGQgKEVSUFggdGVzdCBldmlkZW5jZSkgVGogRVQKZW5kc3RyZWFtCmVuZG9iagp4cmVmCjAgNgowMDAwMDAwMDAwIDY1NTM1IGYgCjAwMDAwMDAwMDkgMDAwMDAgbiAKMDAwMDAwMDA1OCAwMDAwMCBuIAowMDAwMDAwMTE1IDAwMDAwIG4gCjAwMDAwMDAyNDEgMDAwMDAgbiAKMDAwMDAwMDMxMSAwMDAwMCBuIAp0cmFpbGVyCjw8IC9TaXplIDYgL1Jvb3QgMSAwIFIgPj4Kc3RhcnR4cmVmCjQxMAolJUVPRgo=","base64"));
async function request(user:string,path:string,input?:unknown,id?:string,status=200):Promise<Data>{const context=workContexts.get(user);const r=await fetch(`${origin}/api/v1/holdings/acme/${path}`,{method:input?"POST":"GET",headers:{Cookie:cookies(user),Origin:origin,"Content-Type":"application/json",...(context?{"x-erpx-context-revision":String(context.revision),"x-erpx-context-company":context.company,"x-erpx-context-assignment":context.assignment}:{}),...(id?{"Idempotency-Key":id}:{})},...(input?{body:JSON.stringify(input)}:{})});remember(user,r);const body=await r.json() as {data:Data;error:{message:string}};assert.equal(r.status,status,`${path}: ${JSON.stringify(body)}`);return body.data;}
async function invoiceSlip(user:string,invoiceId:string,name:string){const bytes=validEvidencePdf;const intent=await request(user,"uploads",{ownerType:"invoice_payment",ownerId:invoiceId,purpose:"bank_payment_slip",filename:`${name}.pdf`,mediaType:"application/pdf",bytes:bytes.length},await key(`slip-reserve-${name}`),201);const context=workContexts.get(user);const response=await fetch(`${origin}/api/v1/holdings/acme/uploads/${String(intent.id)}/content`,{method:"PUT",headers:{Cookie:cookies(user),Origin:origin,"Content-Type":"application/pdf","Content-Length":String(bytes.length),...(context?{"x-erpx-context-revision":String(context.revision),"x-erpx-context-company":context.company,"x-erpx-context-assignment":context.assignment}: {})},body:bytes});remember(user,response);const body=await response.json() as {data:Data;error:Data};assert.equal(response.status,201,`invoice slip: ${JSON.stringify(body)}`);return String(body.data.id);}
async function deliveryProof(user:string,orderId:string,expectedVersion:number){const bytes=validEvidencePdf;const intent=await request(user,"uploads",{ownerType:"commerce_order_delivery",ownerId:orderId,expectedVersion,purpose:"proof_of_delivery",filename:"signed-delivery.pdf",mediaType:"application/pdf",bytes:bytes.length},await key("delivery-proof-reserve"),201);const context=workContexts.get(user);const response=await fetch(`${origin}/api/v1/holdings/acme/uploads/${String(intent.id)}/content`,{method:"PUT",headers:{Cookie:cookies(user),Origin:origin,"Content-Type":"application/pdf","Content-Length":String(bytes.length),...(context?{"x-erpx-context-revision":String(context.revision),"x-erpx-context-company":context.company,"x-erpx-context-assignment":context.assignment}: {})},body:bytes});remember(user,response);const body=await response.json() as {data:Data;error:Data};assert.equal(response.status,201,`delivery proof: ${JSON.stringify(body)}`);return String(body.data.id);}
const command=async(user:string,name:string,path:string,input:unknown,status=200)=>request(user,`commerce/${path}`,input,await key(name),status);
async function assignFlowKind(kind:string,recordId:string,assignments:Record<string,string>){let flow=await request("mali",`flows/${kind}/${recordId}`);for(const [stepKey,userId] of Object.entries(assignments)){const step=(flow.steps as Data[]).find(item=>item.key===stepKey);if(step?.primary)continue;flow=await request("mali",`flows/${kind}/${recordId}`,{stepKey,userId,expectedVersion:Number(flow.version),recordVersion:Number(flow.recordVersion),reason:`Acceptance routing: ${stepKey} owner is responsible for the next handoff.`,saveDefault:false},await key(`flow-${kind}-${recordId}-${stepKey}`));}}
async function assignFlow(recordId:string,assignments:Record<string,string>){return assignFlowKind("order",recordId,assignments);}
await login("henry");await login("sofia");await login("leila");await login("priya");await login("owen");await login("daniel");await login("nisa","workflow_controller");await login("mali","asg_mali_owner");
const baselinePlatform=await getLocalProxy({persist:true,remoteBindings:false});
const baselineClearing=Number((await baselinePlatform.env.H001_DB.prepare("SELECT COALESCE(SUM(debit_minor-credit_minor),0) net FROM finance_journal_lines WHERE account_id='commerce-chart-acme-co-2150'").first<{net:number}>())?.net??0);
await baselinePlatform.dispose();
const product=await command("owen","product","products",{sku:`CASE-DESK-${scenario.slice(0,8)}`,name:"Harbor office fit-out desk",unit:"each",price:"350.00",taxBps:700,status:"active"});
const purchaseInput={kind:"purchase",partyId:"commerce-acme-co-supplier",warehouseId:"warehouse-acme-co",orderDate:"2026-09-06",dueDate:"2026-09-20",note:"Office fit-out: receive 20 desks, deliver 5 to Harbor Office Solutions.",lines:[{productId:product.id,quantity:"20",price:"200.00",taxBps:700}]};
const po=await command("leila","purchase","orders",purchaseInput);
assert.deepEqual(await command("leila","purchase","orders",purchaseInput),po,"Create replay must return the original receipt");
await command("leila","purchase","orders",{...purchaseInput,note:"Changed request with same key"},409);
await assignFlow(String(po.id),{submitted:"usr_priya",confirmed:"usr_owen",stock_gl:"usr_nisa",invoicing:"usr_daniel"});
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
const purchaseDetail=await request("nisa",`commerce/orders/${po.id}`);
for(const fulfillment of purchaseDetail.fulfillments as Data[])await request("nisa",`finance/journals/${String(fulfillment.journal_id)}/post`,{expectedVersion:1},await key(`purchase-stock-gl-${String(fulfillment.journal_id)}`));
const bill=await command("daniel","po-invoice-12",`fulfillments/${receipt1.fulfillmentId}/invoice`,{expectedVersion:1,date:"2026-09-06",dueDate:"2026-09-30"});
const bill2=await command("daniel","po-invoice-8",`fulfillments/${receipt2.fulfillmentId}/invoice`,{expectedVersion:1,date:"2026-09-06",dueDate:"2026-09-30"});
const periods=await request("nisa","finance/periods");
const periodId=String((periods as unknown as Data[]).find(period=>String(period.start_date)<="2026-09-06"&&String(period.end_date)>="2026-09-06")?.id);
assert.ok(periodId&&periodId!=="undefined","An open Acme accounting period must cover the acceptance date.");
for(const invoice of [bill,bill2]){await assignFlowKind("invoice",String(invoice.invoiceId),{draft:"usr_nisa",posted:"usr_mali"});await request("nisa",`finance/invoices/${String(invoice.invoiceId)}/post`,{expectedVersion:1,periodId,entryDate:"2026-09-06",fxRate:"1"},await key(`supplier-invoice-post-${String(invoice.invoiceId)}`));}
await switchAssignment("mali","workflow_treasury");
await command("daniel","self-payment",`invoices/${bill.invoiceId}/settle`,{expectedVersion:1,date:"2026-09-06",amount:"2568.00",reference:"CASE-SUPPLIER-SELF"},403);
const supplierSlip=await invoiceSlip("mali",String(bill.invoiceId),"supplier-payment");
const payableAfterEvidence=await request("mali",`finance/invoices/${String(bill.invoiceId)}`);
const payableVersion=Number((payableAfterEvidence.invoice as Data).version);
await command("mali","supplier-payment",`invoices/${bill.invoiceId}/settle`,{paymentSlipId:supplierSlip,expectedVersion:payableVersion,date:"2026-09-06",amount:"2568.00",reference:`CASE-${scenario.slice(0,8)}-SUPPLIER-001`});
await switchAssignment("mali","asg_mali_owner");
const priceList=await request("henry","commerce/pricing",{name:`Acceptance ${scenario.slice(0,8)}`,currency:"USD",customerId:"commerce-acme-co-customer",validFrom:"2026-01-01",validUntil:"2026-12-31",maxAutoDiscountBps:0,items:[{productId:product.id,price:"350.00",taxBps:700}]},await key("price-list"));
await request("sofia",`commerce/pricing/${String(priceList.id)}`,{expectedVersion:1,action:"activate",reason:"Independent sales manager approves the acceptance price list."},await key("price-list-activate"));
const quote=await command("henry","quote","orders",{...purchaseInput,kind:"quote",partyId:"commerce-acme-co-customer",priceListId:String(priceList.id),lines:[{productId:product.id,quantity:"5",price:"350.00",taxBps:700}]});
await assignFlow(String(quote.id),{submitted:"usr_sofia",confirmed:"usr_henry",stock_gl:"usr_nisa",invoicing:"usr_daniel"});
await command("henry","quote-submit",`orders/${quote.id}/transition`,{action:"submit",expectedVersion:1});
await command("sofia","quote-approve",`orders/${quote.id}/transition`,{action:"approve",expectedVersion:2});
const converted=await command("henry","quote-convert",`orders/${quote.id}/transition`,{action:"convert",expectedVersion:3});
const so=converted.salesId;
await assignFlow(String(so),{submitted:"usr_sofia",confirmed:"usr_owen",stock_gl:"usr_nisa",invoicing:"usr_daniel"});
await command("henry","sales-submit",`orders/${so}/transition`,{action:"submit",expectedVersion:1});
await command("sofia","sales-approve",`orders/${so}/transition`,{action:"approve",expectedVersion:2});
const soDetail=await request("owen",`commerce/orders/${so}`);const soLine=(soDetail.lines as Data[])[0];
const proofId=await deliveryProof("owen",String(so),Number((soDetail.order as Data).version));
const soAfterProof=await request("owen",`commerce/orders/${so}`);
const delivery=await command("owen","sales-delivery",`orders/${so}/fulfill`,{expectedVersion:Number((soAfterProof.order as Data).version),date:"2026-09-06",lines:[{lineId:soLine.id,quantity:"5"}],evidenceIds:[proofId],evidenceNote:"Customer signed delivery confirmation for Harbor Office Solutions."});
const salesDetail=await request("nisa",`commerce/orders/${so}`);
const salesFulfillment=(salesDetail.fulfillments as Data[]).find(item=>String(item.id)===String(delivery.fulfillmentId));
assert.ok(salesFulfillment?.journal_id,"Sales delivery must create a goods movement journal.");
await request("nisa",`finance/journals/${String(salesFulfillment.journal_id)}/post`,{expectedVersion:1},await key(`sales-stock-gl-${String(salesFulfillment.journal_id)}`));
const invoiceInput={expectedVersion:1,date:"2026-09-06",dueDate:"2026-09-30"};
const invoice=await command("daniel","sales-invoice",`fulfillments/${delivery.fulfillmentId}/invoice`,invoiceInput);
assert.deepEqual(await command("daniel","sales-invoice",`fulfillments/${delivery.fulfillmentId}/invoice`,invoiceInput),invoice);
await assignFlowKind("invoice",String(invoice.invoiceId),{draft:"usr_nisa",posted:"usr_mali"});
await request("nisa",`finance/invoices/${String(invoice.invoiceId)}/post`,{expectedVersion:1,periodId,entryDate:"2026-09-06",fxRate:"1"},await key(`sales-invoice-post-${String(invoice.invoiceId)}`));
await switchAssignment("mali","workflow_treasury");
const postedSalesInvoice=await request("mali",`finance/invoices/${String(invoice.invoiceId)}`);
const partial={expectedVersion:Number((postedSalesInvoice.invoice as Data).version),date:"2026-09-06",amount:"1000.00",reference:`CASE-${scenario.slice(0,8)}-RECEIPT-001`};
await command("mali","receipt-partial",`invoices/${invoice.invoiceId}/settle`,partial);
await command("mali","receipt-partial",`invoices/${invoice.invoiceId}/settle`,partial);
const partialSalesInvoice=await request("mali",`finance/invoices/${String(invoice.invoiceId)}`);
const partialVersion=Number((partialSalesInvoice.invoice as Data).version);
await command("mali","receipt-overpay",`invoices/${invoice.invoiceId}/settle`,{...partial,expectedVersion:partialVersion,reference:`CASE-${scenario.slice(0,8)}-OVERPAY`,amount:"999999.00"},422);
await command("mali","receipt-final",`invoices/${invoice.invoiceId}/settle`,{...partial,expectedVersion:partialVersion,reference:`CASE-${scenario.slice(0,8)}-RECEIPT-002`,amount:"872.50"});
const platform=await getLocalProxy({persist:true,remoteBindings:false});
try{
  const db=platform.env.H001_DB;
  const stock=await db.prepare("SELECT quantity_milli,value_minor FROM commerce_stock WHERE product_id=? AND warehouse_id='warehouse-acme-co'").bind(product.id as string).first();assert.deepEqual(stock,{quantity_milli:15000,value_minor:300000});
  const inv=await db.prepare("SELECT status,outstanding_minor,total_minor,tax_minor FROM finance_invoices WHERE id=?").bind(invoice.invoiceId as string).first();assert.deepEqual(inv,{status:"paid",outstanding_minor:0,total_minor:187250,tax_minor:12250});
  const unbalanced=await db.prepare("SELECT j.id FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id WHERE j.chart_id='commerce-chart-acme-co' GROUP BY j.id HAVING SUM(l.debit_minor)!=SUM(l.credit_minor)").all();assert.equal(unbalanced.results.length,0);
  const clearing=await db.prepare("SELECT SUM(debit_minor-credit_minor) net FROM finance_journal_lines WHERE account_id='commerce-chart-acme-co-2150'").first<{net:number}>();assert.equal(Number(clearing?.net??0),baselineClearing);
  console.log(JSON.stringify({result:"PASS",purchaseOrder:po.id,salesOrder:so,invoice:invoice.invoiceId,stock,invoiceState:inv,checks:["independent login and roles","duplicate command replay","changed-payload conflict","self-approval denied","partial receipts","stale receipt rollback","over-receipt denied","quote conversion","perpetual inventory and COGS","separate tax postings","independent supplier payment","partial customer receipt","allocation settlement","balanced journals","zero receipt clearing balance"]},null,2));
}finally{await platform.dispose();}
