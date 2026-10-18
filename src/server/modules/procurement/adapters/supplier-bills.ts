import {supplierBillOwnerPredicate} from "./bill-source-sql";
import {withSupplierBillPurchasedRead,withSupplierBillPurchasedCommand,supplierBillPurchaseAccess} from "./bill-purchases";
import {supplierBillDecisionSchema} from "../domain/supplier-bills";
import { platformOwnerProjectionSources as commercePlatformSources } from "../../commerce/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as financePlatformSources } from "../../finance/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as identity_accessPlatformSources } from "../../identity_access/public/platform-owner-projection-sources";
import { platformOwnerProjectionSources as people_payrollPlatformSources } from "../../people_payroll/public/platform-owner-projection-sources";
import "server-only";
import type {WorkContext} from "../../identity_access/public/authorization";
import type {SqlDatabase,SqlStatement} from "../../runtime/public/runtime/types";
import type {invoiceFromDeliverySchema} from "../../commerce/public/commerce";
import type {z} from "zod";
import {getOrder} from "../../commerce/public/commerce";
import {stageSupplierBillReceiptVersion,stageSupplierBillReceiptLinks} from "../../commerce/public/supplier-bill-receipts";
import {stageSupplierBillDraftVoid} from "../../finance/public/supplier-bill-draft-void";
import {stageSupplierBillNotifications} from "../../collaboration/public/supplier-bill-notifications";
import {hasPermission} from "@/lib/identity_access/permissions";
import {samePerson} from "@/lib/identity_access/iam";
import {holdingResources} from "../../runtime/public/runtime";
import {requirePermission} from "../../identity_access/public/iam-permissions";
import {assertion,executeCommand,replayCommand} from "../../workflow/public/commands";
import {resolveFlow} from "../../workflow/public/flow-routing";
import {ApiError} from "../../runtime/public/http";
import {authorityGuard,companyAuthority} from "../../operations/public/staff-policy";
import {BusinessRuleError} from "../../shared";
import {createSupplierBillApplication,type BillPorts,type BillMutation} from "../application/supplier-bills";
import type {BillRow as Row} from "../domain/supplier-bills";
export {supplierBillSchema,supplierBillDecisionSchema,supplierInvoicePolicySchema,matchedBillLines} from "../domain/supplier-bills";
const scope="m.company_id=? AND (?::text IS NULL OR m.branch_id=?) AND (?::text IS NULL OR m.department_id=?)";
const args=(actor:WorkContext)=>[actor.companyId,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId];
function event(db:SqlDatabase,actor:WorkContext,id:string,key:string,nextVersion:number,action:string,note:string,now:number){return db.prepare('INSERT INTO procurement_bill_match_events VALUES(?,?,?,?,?,?,?)').bind(`bill-event-${key}`,id,nextVersion,action,actor.userId,note,now);}
function application(actor:WorkContext,provided?:SqlDatabase){
 let resource:Promise<SqlDatabase>|null=null;const database=()=>provided?Promise.resolve(provided):resource??=holdingResources(actor.holdingId).then(x=>x.write);
 const ports:BillPorts<SqlStatement>={clock:{now:()=>Date.now()},companyId:actor.companyId,userId:actor.userId,canRead:hasPermission(actor,'finance.read'),canWrite:hasPermission(actor,'finance.write'),canApprove:hasPermission(actor,'finance.approve'),canCommerceRead:hasPermission(actor,'commerce.read'),samePerson:(id)=>samePerson(actor,id),
 async authorizeRead(){requirePermission(actor,'finance.read');},async authorizeWrite(){requirePermission(actor,'finance.write');},async authorizeApprove(){requirePermission(actor,'finance.approve');},async authorizeCommerceRead(){requirePermission(actor,'commerce.read');},
 replay:async(key,operation,id,input)=>replayCommand(await database(),actor,key,operation,id,input),
 bill:async(id)=>{const write=await database();
  const row = await write.prepare(`
    SELECT m.*,p.name supplier_name,o.number order_number,f.version fulfillment_version,
      i.status invoice_status,i.created_by invoice_created_by,i.version invoice_version,
      (SELECT count(*) FROM procurement_bill_match_receipts br WHERE br.match_id=m.id) receipt_count
    FROM procurement_bill_matches m
    JOIN (${financePlatformSources.finance_parties}) p ON p.id=m.supplier_id
    JOIN (${commercePlatformSources.commerce_orders}) o ON o.id=m.order_id
    JOIN (${commercePlatformSources.commerce_fulfillments}) f ON f.id=m.fulfillment_id
    LEFT JOIN (${financePlatformSources.finance_invoices}) i ON i.id=m.invoice_id
    WHERE m.id=? AND ${scope}
  `).bind(id, ...args(actor)).first<Row>();
  return row;
},workspace:async()=>{const write=await database();
  return (await write.prepare(`
    SELECT m.*,p.name supplier_name,o.number order_number,
      COALESCE((SELECT json_agg(br.fulfillment_id ORDER BY f.fulfillment_date,f.id)::text
        FROM procurement_bill_match_receipts br JOIN (${commercePlatformSources.commerce_fulfillments}) f ON f.id=br.fulfillment_id
        WHERE br.match_id=m.id),'[]') receipt_ids_json,
      (SELECT count(*) FROM procurement_bill_match_receipts br WHERE br.match_id=m.id) receipt_count
    FROM procurement_bill_matches m
    JOIN (${financePlatformSources.finance_parties}) p ON p.id=m.supplier_id
    JOIN (${commercePlatformSources.commerce_orders}) o ON o.id=m.order_id
    WHERE ${scope} AND ${supplierBillOwnerPredicate("m")} ORDER BY m.updated_at DESC
  `).bind(...args(actor)).all<Row>()).results;
},captureOptions:async()=>{const write=await database();
  const receipts = (await write.prepare(`
    SELECT f.*,o.number order_number,o.party_id supplier_id,o.currency order_currency,
      o.branch_id,o.department_id,p.name supplier_name,j.status stock_journal_status,
      j.currency journal_currency,j.chart_id journal_chart_id
    FROM (${commercePlatformSources.commerce_fulfillments}) f
    JOIN (${commercePlatformSources.commerce_orders}) o ON o.id=f.order_id AND o.company_id=? AND o.kind='purchase'
    JOIN (${financePlatformSources.finance_parties}) p ON p.id=o.party_id
    JOIN (${financePlatformSources.finance_journals}) j ON j.id=f.journal_id AND j.status='posted'
    WHERE f.invoice_id IS NULL
      AND (?::text IS NULL OR o.branch_id=?)
      AND (?::text IS NULL OR o.department_id=?)
      AND NOT EXISTS (
        SELECT 1 FROM procurement_bill_match_receipts br
        WHERE br.fulfillment_id=f.id AND br.active=TRUE
      )
    ORDER BY p.name,o.number,f.fulfillment_date,f.id
  `).bind(actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId).all<Row>()).results;
  if (!receipts.length) return { fulfillments: [], lines: [], currency: null };
  const ids = receipts.map(receipt => String(receipt.id));
  const placeholders = ids.map(() => "?").join(",");
  const lines = (await write.prepare(`
    SELECT fl.*,ol.product_id,ol.description,p.sku,p.unit,p.tracking_mode
    FROM (${commercePlatformSources.commerce_fulfillment_lines}) fl
    JOIN (${commercePlatformSources.commerce_order_lines}) ol ON ol.id=fl.order_line_id
    JOIN (${commercePlatformSources.commerce_products}) p ON p.id=ol.product_id
    WHERE fl.fulfillment_id IN (${placeholders})
    ORDER BY fl.fulfillment_id,ol.ordinal
  `).bind(...ids).all<Row>()).results;
  return { fulfillments: receipts, lines, currency: String(receipts[0].order_currency) };
},detail:async(bill)=>{const write=await database(),id=String(bill.id);
  const [lines, receipts, events, accounts] = await Promise.all([
    write.prepare(`
      SELECT l.*,ol.description,fl.fulfillment_id,f.number receipt_number,f.fulfillment_date,o.number order_number
      FROM procurement_bill_match_lines l
      JOIN (${commercePlatformSources.commerce_fulfillment_lines}) fl ON fl.id=l.fulfillment_line_id
      JOIN (${commercePlatformSources.commerce_fulfillments}) f ON f.id=fl.fulfillment_id
      JOIN (${commercePlatformSources.commerce_orders}) o ON o.id=f.order_id
      JOIN (${commercePlatformSources.commerce_order_lines}) ol ON ol.id=fl.order_line_id
      WHERE l.match_id=? ORDER BY f.fulfillment_date,f.id,ol.ordinal
    `).bind(id).all<Row>(),
    write.prepare(`
      SELECT br.*,f.number receipt_number,f.fulfillment_date,f.version current_version,f.invoice_id,o.id order_id,o.number order_number
      FROM procurement_bill_match_receipts br JOIN (${commercePlatformSources.commerce_fulfillments}) f ON f.id=br.fulfillment_id
      JOIN (${commercePlatformSources.commerce_orders}) o ON o.id=f.order_id
      WHERE br.match_id=? ORDER BY f.fulfillment_date,f.id
    `).bind(id).all<Row>(),
    write.prepare("SELECT * FROM procurement_bill_match_events WHERE match_id=? ORDER BY version DESC").bind(id).all<Row>(),
    write.prepare(`SELECT a.id,a.code,a.name FROM (${financePlatformSources.finance_accounts}) a JOIN (${financePlatformSources.finance_charts}) c ON c.id=a.chart_id AND c.company_id=a.company_id WHERE a.company_id=? AND a.chart_id=? AND a.currency=c.base_currency AND a.category='expense' AND a.status='active' ORDER BY a.code`)
      .bind(actor.companyId, bill.chart_id).all<Row>(),
  ]);
  const flow = hasPermission(actor, "commerce.read")
    ? await (await import("@/server/modules/workflow/public/flow-routing")).resolveFlow(write, actor, "order", String(bill.order_id))
    : null;
return {lines:lines.results,receipts:receipts.results,events:events.results,accounts:accounts.results,invoicingUserId:flow?.steps.find(step=>step.key==='invoicing')?.primary?.userId??null};
},companyAuthority:async()=>companyAuthority(await database(),actor,'finance.approve'),
 policy:async()=>(await database()).prepare(`SELECT p.*,COALESCE(person.legal_name,p.updated_by) updated_by_name FROM procurement_invoice_policies p LEFT JOIN (${people_payrollPlatformSources.hr_people}) person ON person.company_id=p.company_id AND person.user_id=p.updated_by AND person.merged_into_id IS NULL WHERE p.company_id=?`).bind(actor.companyId).first<Row>(),
 policyEvents:async()=>(await (await database()).prepare(`SELECT e.*,COALESCE(person.legal_name,e.actor_id) actor_name FROM procurement_invoice_policy_events e LEFT JOIN (${people_payrollPlatformSources.hr_people}) person ON person.company_id=e.company_id AND person.user_id=e.actor_id AND person.merged_into_id IS NULL WHERE e.company_id=? ORDER BY e.version DESC`).bind(actor.companyId).all<Row>()).results,
 receipt:async(id)=>(await database()).prepare(`SELECT f.*,j.status journal_status,j.currency journal_currency,j.chart_id journal_chart_id FROM (${commercePlatformSources.commerce_fulfillments}) f LEFT JOIN (${financePlatformSources.finance_journals}) j ON j.id=f.journal_id WHERE f.id=?`).bind(id).first<Row>(),
 order:async(id)=>(await getOrder(actor,id,provided)).order,
 configuration:async()=>(await database()).prepare(`SELECT * FROM (${commercePlatformSources.commerce_settings}) commerce_settings WHERE company_id=?`).bind(actor.companyId).first<Row>(),
 postings:async(id)=>(await (await database()).prepare(`SELECT * FROM (${financePlatformSources.finance_journal_lines}) finance_journal_lines WHERE journal_id=? ORDER BY ordinal`).bind(id).all<Row>()).results,
 receiptLines:async(ids)=>(await (await database()).prepare(`SELECT fl.*,ol.product_id,ol.description,p.sku,p.unit,p.tracking_mode FROM (${commercePlatformSources.commerce_fulfillment_lines}) fl JOIN (${commercePlatformSources.commerce_order_lines}) ol ON ol.id=fl.order_line_id JOIN (${commercePlatformSources.commerce_products}) p ON p.id=ol.product_id WHERE fl.fulfillment_id IN (${ids.map(()=>'?').join(',')}) ORDER BY fl.fulfillment_id,ol.ordinal`).bind(...ids).all<Row>()).results,
 variance:async(id)=>!!await (await database()).prepare('SELECT id FROM procurement_bill_match_lines WHERE match_id=? AND subtotal_minor<>expected_subtotal_minor LIMIT 1').bind(id).first(),
 varianceAccount:async(id,chartId)=>!!await (await database()).prepare(`SELECT a.id FROM (${financePlatformSources.finance_accounts}) a JOIN (${financePlatformSources.finance_charts}) c ON c.id=a.chart_id AND c.company_id=a.company_id WHERE a.id=? AND a.company_id=? AND a.chart_id=? AND a.currency=c.base_currency AND a.category='expense' AND a.status='active'`).bind(id,actor.companyId,chartId).first(),
 commit:async(m,result)=>commit(await database(),actor,m,result),
 receiptHistory:async(id)=>(await database()).prepare('SELECT br.match_id id FROM procurement_bill_match_receipts br WHERE br.fulfillment_id=? LIMIT 1').bind(id).first<Row>(),
 matchingRequired:async()=>!!await (await database()).prepare("SELECT 1 FROM procurement_invoice_policies WHERE company_id=? AND matching_mode='required'").bind(actor.companyId).first(),
 billReceipts:async(id)=>(await (await database()).prepare('SELECT * FROM procurement_bill_match_receipts WHERE match_id=? ORDER BY id').bind(id).all<Row>()).results,
 stageInvoice:async(h)=>{const db=await database(),bill=h.bill,nextVersion=Number(bill.version)+1;
 const steps:SqlStatement[]=[assertion(db,h.key,'supplier-bill-handoff',"EXISTS(SELECT 1 FROM procurement_bill_matches WHERE id=? AND company_id=? AND fulfillment_id=? AND status='approved' AND version=? AND invoice_id IS NULL)",[bill.id,actor.companyId,h.fulfillment.id,h.matchVersion])];
 for(const [index,receipt]of h.receipts.entries())steps.push(assertion(db,h.key,`supplier-bill-receipt-${index}`,`EXISTS(SELECT 1 FROM procurement_bill_match_receipts br JOIN (${commercePlatformSources.commerce_fulfillments}) f ON f.id=br.fulfillment_id WHERE br.id=? AND br.match_id=? AND br.active=TRUE AND f.version=br.captured_version AND f.invoice_id IS NULL)`,[receipt.id,bill.id]));
 steps.push(stageSupplierBillReceiptLinks(db,String(bill.id),h.invoiceId),db.prepare("UPDATE procurement_bill_matches SET status='invoiced',invoice_id=?,version=version+1,updated_at=? WHERE id=?").bind(h.invoiceId,h.now,bill.id),event(db,actor,String(bill.id),h.key,nextVersion,'invoice-created',`Draft invoice ${h.invoiceId} prepared from ${h.receipts.length} receipt(s) for independent GL posting`,h.now),stageSupplierBillNotifications(db,String(bill.id),h.key));return steps;},
 billByInvoice:async(id)=>(await database()).prepare('SELECT * FROM procurement_bill_matches WHERE invoice_id=? AND company_id=?').bind(id,actor.companyId).first<Row>(),
 stagePosting:async(h)=>{const db=await database(),bill=h.bill;return [assertion(db,h.key,'supplier-bill-post',"EXISTS(SELECT 1 FROM procurement_bill_matches WHERE id=? AND version=? AND status='invoiced' AND invoice_id=?)",[bill.id,bill.version,h.invoice.id]),db.prepare("UPDATE procurement_bill_matches SET status='posted',version=version+1,updated_at=? WHERE id=?").bind(h.now,bill.id),event(db,actor,String(bill.id),h.key,Number(bill.version)+1,'posted','Matched invoice and price variance posted to GL',h.now),stageSupplierBillNotifications(db,String(bill.id),h.key)];},
 workFacts:async(row)=>{const write=await database();
let latest:Row|null=null,eligible=0,primary:{userId:string;name:string}|null=null,flowBlocker:string|null=null;
 if(row.status==='returned')latest=await write.prepare("SELECT id,status FROM procurement_bill_matches WHERE fulfillment_id=? ORDER BY (status<>'returned') DESC,created_at DESC,id DESC LIMIT 1").bind(row.fulfillment_id).first<Row>();
 if(row.status==='review')eligible=Number(await write.prepare(`SELECT count(DISTINCT c.user_id) n FROM (${identity_accessPlatformSources.assignments}) c WHERE c.company_id=? AND iam_assignment_active(c.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(c.role,c.company_id,'finance.write') AND iam_role_allows(c.role,c.company_id,'finance.approve') AND (c.scope_kind='holding' OR (c.scope_kind='branch' AND c.scope_node_id=?) OR (c.scope_kind='department' AND c.scope_node_id=?)) AND c.user_id<>? AND NOT EXISTS(SELECT 1 FROM (${identity_accessPlatformSources.iam_members}) cm JOIN (${identity_accessPlatformSources.iam_members}) creator ON creator.company_id=cm.company_id AND creator.person_id=cm.person_id WHERE cm.company_id=c.company_id AND cm.user_id=c.user_id AND creator.user_id=?)`).bind(actor.companyId,row.branch_id,row.department_id,row.created_by,row.created_by).first('n'));
 else if(row.status!=='returned'||latest?.id===row.id&&latest.status==='returned'){const flow=await resolveFlow(write,actor,'order',String(row.order_id)),step=flow.steps.find(x=>x.key==='invoicing');primary=step?.primary?{userId:step.primary.userId,name:step.primary.name}:null;flowBlocker=step?.blocker??null;}
 return {latest,eligible,primary,flowBlocker};
},};return createSupplierBillApplication(ports);
}
async function commit<Result>(write:SqlDatabase,actor:WorkContext,m:BillMutation,result:Result):Promise<Result>{
 const {key,operation,id,now}=m;
 if(m.kind==='capture'){const input=m.input;
  const {requested,receipts:receiptRows,primary,order,config,clearing,lines:prepared,expected,subtotal,tax,quantityMatches}=m.plan;
  const steps: SqlStatement[] = [write.prepare(`
    INSERT INTO procurement_bill_matches(id,company_id,branch_id,department_id,order_id,fulfillment_id,supplier_id,currency,chart_id,clearing_account_id,payable_account_id,tax_account_id,reference,invoice_date,due_date,evidence,expected_subtotal_minor,subtotal_minor,tax_minor,quantity_matches,status,created_by,created_at,updated_at)
    VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'review',?,?,?)
  `).bind(id, actor.companyId, order.branch_id, order.department_id, order.id, primary.id, order.party_id, order.currency, config.chart_id, clearing, config.payable_account_id, config.input_tax_account_id, input.reference, input.invoiceDate, input.dueDate, input.evidence, expected, subtotal, tax, quantityMatches, actor.userId, now, now)];
  for (const [index, receipt] of receiptRows.entries()) {
    const selected = requested.find(candidate => candidate.fulfillmentId === receipt.id)!;
    steps.push(
      assertion(write, key, `receipt-${index}`, `EXISTS(SELECT 1 FROM (${commercePlatformSources.commerce_fulfillments}) f JOIN (${financePlatformSources.finance_journals}) j ON j.id=f.journal_id WHERE f.id=? AND f.order_id=? AND f.version=? AND f.invoice_id IS NULL AND j.status='posted')`, [receipt.id, receipt.order_id, selected.expectedVersion]),
      write.prepare("INSERT INTO procurement_bill_match_receipts(id,match_id,fulfillment_id,captured_version) VALUES(?,?,?,?)").bind(`${id}-receipt-${index}`, id, receipt.id, selected.expectedVersion + 1),
      stageSupplierBillReceiptVersion(write,String(receipt.id)),
    );
  }
  for (const line of prepared) steps.push(write.prepare("INSERT INTO procurement_bill_match_lines VALUES(?,?,?,?,?,?,?,?,?,?)").bind(`${id}-${line.index}`, id, line.line.id, line.quantity, line.line.quantity_milli, line.price, line.taxBps, line.line.subtotal_minor, line.subtotal, line.tax));
  steps.push(event(write, actor, id, key, 1, "capture", input.evidence,now), stageSupplierBillNotifications(write, id, key));
  return executeCommand(write, actor, key, operation, input, id, 1, result, steps, { companyId: actor.companyId, branchId: String(order.branch_id), departmentId: order.department_id ? String(order.department_id) : null });
 }
 if(m.kind==='policy'){const input=m.input;return executeCommand(write,actor,key,operation,input,id,m.version,result,[
 authorityGuard(write,key,actor,'finance.approve'),
 assertion(write,key,'policy-version',input.expectedVersion===0?'NOT EXISTS(SELECT 1 FROM procurement_invoice_policies WHERE company_id=?)':'EXISTS(SELECT 1 FROM procurement_invoice_policies WHERE company_id=? AND version=?)',input.expectedVersion===0?[actor.companyId]:[actor.companyId,input.expectedVersion]),
 write.prepare('INSERT INTO procurement_invoice_policies(company_id,matching_mode,version,updated_by,updated_at) VALUES(?,?,?,?,?) ON CONFLICT(company_id) DO UPDATE SET matching_mode=excluded.matching_mode,version=excluded.version,updated_by=excluded.updated_by,updated_at=excluded.updated_at').bind(actor.companyId,input.matchingMode,m.version,actor.userId,now),
 write.prepare('INSERT INTO procurement_invoice_policy_events(id,company_id,version,previous_mode,matching_mode,actor_id,reason,created_at) VALUES(?,?,?,?,?,?,?,?)').bind(`supplier-policy-${key}`,actor.companyId,m.version,m.previous,input.matchingMode,actor.userId,input.reason,now),
 ],{companyId:actor.companyId,branchId:null,departmentId:null});}
 const {input,bill,returnDraft,variance,status}=m;
 const steps:SqlStatement[]=[assertion(write,key,'bill-review',`EXISTS(SELECT 1 FROM procurement_bill_matches m WHERE m.id=? AND m.version=? AND m.status=? AND ${scope})`,[id,input.expectedVersion,bill.status,...args(actor)])];
 if(input.action==='approve'&&variance)steps.push(assertion(write,key,'variance-account',`EXISTS(SELECT 1 FROM (${financePlatformSources.finance_accounts}) a JOIN (${financePlatformSources.finance_charts}) c ON c.id=a.chart_id AND c.company_id=a.company_id WHERE a.id=? AND a.company_id=? AND a.chart_id=? AND a.currency=c.base_currency AND a.category='expense' AND a.status='active')`,[input.varianceAccountId!,actor.companyId,bill.chart_id]));
 if(returnDraft)steps.push(...stageSupplierBillDraftVoid(write,actor.companyId,{invoiceId:String(bill.invoice_id),invoiceVersion:Number(bill.invoice_version),matchId:id},key,now),stageSupplierBillReceiptLinks(write,id,null));
 if(status==='returned')steps.push(write.prepare('UPDATE procurement_bill_match_receipts SET active=FALSE WHERE match_id=? AND active=TRUE').bind(id));
 steps.push(write.prepare('UPDATE procurement_bill_matches SET status=?,reviewed_by=?,review_reason=?,variance_account_id=?,version=version+1,updated_at=? WHERE id=?').bind(status,actor.userId,input.reason,input.action==='approve'&&variance?input.varianceAccountId??null:bill.variance_account_id??null,now,id),event(write,actor,id,key,input.expectedVersion+1,returnDraft?'return-ap-draft':input.action,input.reason,now),stageSupplierBillNotifications(write,id,key));
 return executeCommand(write,actor,key,operation,input,id,input.expectedVersion+1,result,steps,{companyId:actor.companyId,branchId:String(bill.branch_id),departmentId:bill.department_id?String(bill.department_id):null});
}
async function invoke<T>(call:()=>Promise<T>):Promise<T>{try{return await call();}catch(error){if(error instanceof BusinessRuleError)throw new ApiError(error.status,error.code,error.message);throw error;}}
export const getSupplierBill=(a:WorkContext,id:string,provided?:SqlDatabase)=>withSupplierBillPurchasedRead(a,db=>invoke(()=>application(a,db).get(id)),provided,id);
export const supplierBillWorkspace=(a:WorkContext)=>withSupplierBillPurchasedRead(a,db=>invoke(()=>application(a,db).workspace()));
export const supplierBillCaptureOptions=(a:WorkContext)=>withSupplierBillPurchasedRead(a,db=>invoke(()=>application(a,db).captureOptions()));
export const supplierInvoicePolicy=(a:WorkContext)=>withSupplierBillPurchasedRead(a,async(db,purchase)=>({...await invoke(()=>application(a,db).policy()),companyId:a.companyId,purchase}));
export const saveSupplierInvoicePolicy=(a:WorkContext,raw:unknown,key:string)=>withSupplierBillPurchasedCommand(a,{key,operation:"procurement.invoice-policy.update",id:a.companyId,policy:true,purpose:"new-work",permissions:["finance.approve"]},db=>invoke(()=>application(a,db).savePolicy(raw,key)));
export const supplierBillDetail=(a:WorkContext,id:string)=>withSupplierBillPurchasedRead(a,async(db,purchase)=>({...await invoke(()=>application(a,db).detail(id)),purchase}),undefined,id);
export const captureSupplierBill=(a:WorkContext,raw:unknown,key:string)=>withSupplierBillPurchasedCommand(a,{key,operation:"procurement.bill.capture",id:`bill-match-${key}`,capture:raw,purpose:"settle-existing",permissions:["finance.write","commerce.read"]},db=>invoke(()=>application(a,db).capture(raw,key)));
export const decideSupplierBill=(a:WorkContext,id:string,raw:unknown,key:string)=>{const input=supplierBillDecisionSchema.parse(raw);return withSupplierBillPurchasedCommand(a,{key,operation:`procurement.bill.${input.action}`,id,purpose:"settle-existing",permissions:["finance.read","finance.write","finance.approve"]},db=>invoke(()=>application(a,db).decide(id,raw,key)));};
export const supplierBillInvoiceHandoff=(db:SqlDatabase,a:WorkContext,fulfillment:Row,input:z.infer<typeof invoiceFromDeliverySchema>,invoiceId:string,key:string)=>invoke(()=>application(a,db).invoiceHandoff(fulfillment,input,invoiceId,key));
export const matchedInvoicePosting=(db:SqlDatabase,a:WorkContext,invoice:Row,key:string)=>invoke(()=>application(a,db).invoicePosting(invoice,key));
export const supplierBillWork=async(a:WorkContext)=>{if(!hasPermission(a,"finance.read"))return [];const access=await supplierBillPurchaseAccess((await holdingResources(a.holdingId)).write,a);if(access&&!access.history)return [];const items=await invoke(()=>application(a).work());return access&&!access.settleExisting?items.map(item=>({...item,canAct:false,queue:"waiting" as const})):items;};
