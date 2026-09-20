import { readListPage } from "./list-page";
import { requirePermission } from "@/server/iam-permissions";
import { hasPermission } from "@/lib/permissions";
import { samePerson } from "@/lib/iam";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import { holdingResources } from "./runtime";
import { assertion,executeCommand,replayCommand } from "./commands";
import { prepareJournal } from "./finance";
import { decimalToMinor,minorToDecimal } from "@/lib/finance-money";
import { scaledAmount } from "@/lib/commerce-math";
import { sha256 } from "./crypto";
import { ApiError } from "./http";
export type CountRow=Record<string,string|number|null>;
export const countSchema=z.object({
  warehouseId:z.string().min(1),countDate:z.iso.date(),reason:z.string().trim().min(5).max(2000),
  expectedVersion:z.number().int().positive().optional(),
  lines:z.array(z.object({productId:z.string().min(1),countedQuantity:z.string().regex(/^\d{1,12}(\.\d{1,3})?$/),unitCost:z.string().regex(/^\d{1,12}(\.\d{1,2})?$/).optional()})).min(1).max(50),
});
export const countActionSchema=z.object({expectedVersion:z.number().int().positive(),reason:z.string().trim().min(3).max(2000).optional()});
export function canReadCounts(actor:WorkContext){return !actor.departmentId&&hasPermission(actor,"inventory.read");}
export function canPrepareCounts(actor:WorkContext){return !actor.departmentId&&hasPermission(actor,"inventory.prepare");}
export function canApproveCounts(actor:WorkContext){return !actor.departmentId&&!actor.branchId&&hasPermission(actor,"inventory.approve");}
export function canEditCount(actor:WorkContext,row:CountRow){return canPrepareCounts(actor)&&["draft","rejected"].includes(String(row.status))&&[row.created_by,row.prepared_by].some(id=>samePerson(actor,id));}
export function canReviewCount(actor:WorkContext,row:CountRow){return canApproveCounts(actor)&&["submitted","approved"].includes(String(row.status))&&![row.created_by,row.prepared_by].some(id=>samePerson(actor,id));}
function readable(actor:WorkContext){if(!canReadCounts(actor))throw new ApiError(403,"INVENTORY_SCOPE_REQUIRED","Use a branch or holding inventory or finance assignment.");}
export async function listCounts(actor:WorkContext){
  readable(actor);const {write}=await holdingResources(actor.holdingId);
  return (await write.prepare("SELECT c.*,w.name warehouse_name FROM inventory_counts c JOIN commerce_warehouses w ON w.id=c.warehouse_id WHERE c.company_id=? AND (?::text IS NULL OR c.branch_id=?) ORDER BY c.updated_at DESC LIMIT 250").bind(actor.companyId,actor.branchId,actor.branchId).all<CountRow>()).results;
}
export async function countsPage(actor:WorkContext,query:{q?:string;status?:string;cursor?:string}) {
  readable(actor);const {write}=await holdingResources(actor.holdingId);
  return readListPage<CountRow & {id:string;page_key:string}>(write,{
    sql:`SELECT c.*,w.name warehouse_name,s.currency,c.updated_at::text AS page_key FROM inventory_counts c
      JOIN commerce_warehouses w ON w.id=c.warehouse_id JOIN commerce_settings s ON s.company_id=c.company_id
      WHERE c.company_id=? AND (?::text IS NULL OR c.branch_id=?) AND (?='' OR c.status=?)
      AND (?='' OR strpos(lower(c.number||' '||w.name||' '||c.reason),lower(?))>0)`,
    args:[actor.companyId,actor.branchId,actor.branchId,query.status??'',query.status??'',query.q??'',query.q??''],
    cursor:query.cursor,descending:true,scope:[actor.companyId,actor.branchId,'counts',query.q??'',query.status??''],
  });
}
export async function getCount(actor:WorkContext,id:string){
  readable(actor);const {write}=await holdingResources(actor.holdingId);
  const row=await write.prepare("SELECT c.*,w.name warehouse_name,s.currency FROM inventory_counts c JOIN commerce_warehouses w ON w.id=c.warehouse_id JOIN commerce_settings s ON s.company_id=c.company_id WHERE c.id=? AND c.company_id=? AND (?::text IS NULL OR c.branch_id=?)").bind(id,actor.companyId,actor.branchId,actor.branchId).first<CountRow>();
  if(!row)throw new ApiError(404,"COUNT_NOT_FOUND","Stock count not found in your workspace.");return row;
}
export async function countDetail(actor:WorkContext,id:string){
  const count=await getCount(actor,id),{write}=await holdingResources(actor.holdingId);
  const [lines,history,decisions]=await Promise.all([
    write.prepare("SELECT l.*,p.sku,s.quantity_milli current_quantity_milli,s.value_minor current_value_minor,COALESCE(s.version,0) current_version FROM inventory_count_lines l JOIN commerce_products p ON p.id=l.product_id LEFT JOIN commerce_stock s ON s.product_id=l.product_id AND s.warehouse_id=? WHERE l.count_id=? ORDER BY l.ordinal").bind(count.warehouse_id,id).all<CountRow>(),
    write.prepare("SELECT a.id,a.action,a.created_at,COALESCE(p.legal_name,a.actor_id) actor_name FROM audit_events a LEFT JOIN hr_people p ON p.user_id=a.actor_id AND p.company_id=? AND p.merged_into_id IS NULL WHERE a.record_id=? ORDER BY a.created_at DESC").bind(actor.companyId,id).all<CountRow>(),
    write.prepare("SELECT d.*,COALESCE(p.legal_name,d.actor_id) actor_name FROM inventory_count_decisions d LEFT JOIN hr_people p ON p.user_id=d.actor_id AND p.company_id=? AND p.merged_into_id IS NULL WHERE d.count_id=? ORDER BY d.created_at DESC").bind(actor.companyId,id).all<CountRow>(),
  ]);return {count,lines:lines.results,history:history.results,decisions:decisions.results};
}
export async function countOptions(actor:WorkContext){
  actor = requirePermission(actor,"inventory.prepare");

  readable(actor);const {write}=await holdingResources(actor.holdingId);
  const [warehouses,products,stock,settings]=await Promise.all([
    write.prepare("SELECT id,name,branch_id FROM commerce_warehouses WHERE company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?) ORDER BY name").bind(actor.companyId,actor.branchId,actor.branchId).all<CountRow>(),
    write.prepare("SELECT id,sku,name,unit FROM commerce_products WHERE company_id=? AND status='active' ORDER BY sku").bind(actor.companyId).all<CountRow>(),
    write.prepare("SELECT s.* FROM commerce_stock s JOIN commerce_warehouses w ON w.id=s.warehouse_id WHERE w.company_id=? AND (?::text IS NULL OR w.branch_id=?)").bind(actor.companyId,actor.branchId,actor.branchId).all<CountRow>(),
    write.prepare("SELECT currency FROM commerce_settings WHERE company_id=?").bind(actor.companyId).first<{currency:string}>(),
  ]);return {warehouses:warehouses.results,products:products.results,stock:stock.results,currency:settings?.currency??""};
}
function countGuard(db:SqlDatabase,key:string,row:CountRow,version:number,statuses:string[]){return assertion(db,key,"count-version",`EXISTS(SELECT 1 FROM inventory_counts WHERE id=? AND company_id=? AND version=? AND status IN (${statuses.map(()=>"?").join(",")}))`,[row.id,row.company_id,version,...statuses]);}
function stockGuard(db:SqlDatabase,key:string,index:number,warehouse:CountRow[string],line:CountRow){
  return Number(line.snapshot_version)===0?assertion(db,key,`stock-${index}`,"NOT EXISTS(SELECT 1 FROM commerce_stock WHERE warehouse_id=? AND product_id=?)",[warehouse,line.product_id]):assertion(db,key,`stock-${index}`,"EXISTS(SELECT 1 FROM commerce_stock WHERE warehouse_id=? AND product_id=? AND version=? AND quantity_milli=? AND value_minor=?)",[warehouse,line.product_id,line.snapshot_version,line.snapshot_quantity_milli,line.snapshot_value_minor]);
}
export async function saveCount(actor:WorkContext,id:string|null,input:z.infer<typeof countSchema>,key:string){
  actor = requirePermission(actor,"inventory.prepare");

  if(!canPrepareCounts(actor))throw new ApiError(403,"COUNT_PREPARER_REQUIRED","An inventory operator must prepare the physical count.");
  const {write}=await holdingResources(actor.holdingId),record=id??`count-${key}`,operation=id?"inventory.count.update":"inventory.count.create";
  const prior=await replayCommand(write,actor,key,operation,record,input);if(prior)return prior;
  const current=id?await getCount(actor,id):null;if(current&&!canEditCount(actor,current))throw new ApiError(403,"COUNT_LOCKED","Only the preparer can edit a draft or returned count.");
  if(current&&current.warehouse_id!==input.warehouseId)throw new ApiError(422,"WAREHOUSE_LOCKED","Create a separate count for another warehouse.");
  if(new Set(input.lines.map(l=>l.productId)).size!==input.lines.length)throw new ApiError(422,"DUPLICATE_PRODUCT","Count each product once.");
  const warehouse=await write.prepare("SELECT * FROM commerce_warehouses WHERE id=? AND company_id=? AND status='active' AND (?::text IS NULL OR branch_id=?)").bind(input.warehouseId,actor.companyId,actor.branchId,actor.branchId).first<CountRow>();
  if(!warehouse)throw new ApiError(422,"WAREHOUSE_INVALID","Choose an active warehouse in your scope.");
  const lines:CountRow[]=[],steps:SqlStatement[]=[];
  for(const [index,item] of input.lines.entries()){
    const product=await write.prepare("SELECT * FROM commerce_products WHERE id=? AND company_id=? AND status='active'").bind(item.productId,actor.companyId).first<CountRow>();if(!product)throw new ApiError(422,"PRODUCT_INVALID","Choose an active stock item in this company.");
    const stock=await write.prepare("SELECT * FROM commerce_stock WHERE warehouse_id=? AND product_id=?").bind(warehouse.id,item.productId).first<CountRow>();
    const qty=decimalToMinor(item.countedQuantity,3),oldQty=Number(stock?.quantity_milli??0),oldValue=Number(stock?.value_minor??0),unitCost=item.unitCost?decimalToMinor(item.unitCost):null;
    if(oldQty===0&&qty>0&&(!unitCost||unitCost<=0))throw new ApiError(422,"VALUATION_REQUIRED","Enter a verified unit cost when counting stock without an existing valuation.");
    const value=qty===0?0:oldQty>0?scaledAmount(oldValue,qty,oldQty):scaledAmount(unitCost!,qty,1000);
    if(qty>0&&value===0)throw new ApiError(422,"ZERO_VALUATION","A positive count requires a positive inventory valuation.");
    const line:CountRow={id:`${record}-${index}`,product_id:item.productId,product_version:product.version,description:product.name,unit:product.unit,snapshot_quantity_milli:oldQty,snapshot_value_minor:oldValue,snapshot_version:stock?.version??0,counted_quantity_milli:qty,counted_value_minor:value,unit_cost_minor:oldQty===0?unitCost:null,ordinal:index};lines.push(line);
    steps.push(stockGuard(write,key,index,warehouse.id,line),assertion(write,key,`product-${index}`,"EXISTS(SELECT 1 FROM commerce_products WHERE id=? AND version=? AND status='active')",[item.productId,product.version]),assertion(write,key,`date-${index}`,"NOT EXISTS(SELECT 1 FROM inventory_ledger WHERE warehouse_id=? AND product_id=? AND movement_date>?)",[warehouse.id,item.productId,input.countDate]));
  }
  const net=lines.reduce((s,l)=>s+BigInt(Number(l.counted_value_minor)-Number(l.snapshot_value_minor)),BigInt(0));if(net>BigInt(Number.MAX_SAFE_INTEGER)||net< -BigInt(Number.MAX_SAFE_INTEGER))throw new ApiError(422,"VALUE_OVERFLOW","Stock count value exceeds supported precision.");
  const now=Date.now(),version=current?Number(input.expectedVersion??0)+1:1;
  steps.push(assertion(write,key,"warehouse","EXISTS(SELECT 1 FROM commerce_warehouses WHERE id=? AND version=? AND status='active')",[warehouse.id,warehouse.version]));
  if(current)steps.push(countGuard(write,key,current,input.expectedVersion??0,["draft","rejected"]),write.prepare("UPDATE inventory_counts SET count_date=?,reason=?,net_value_minor=?,status='draft',prepared_by=?,review_reason=NULL,version=version+1,updated_at=? WHERE id=?").bind(input.countDate,input.reason,Number(net),actor.userId,now,record),write.prepare("DELETE FROM inventory_count_lines WHERE count_id=?").bind(record));
  else steps.push(write.prepare("INSERT INTO inventory_counts(id,company_id,branch_id,warehouse_id,number,count_date,reason,net_value_minor,created_by,prepared_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(record,actor.companyId,warehouse.branch_id,warehouse.id,`COUNT-${(await sha256(record)).slice(0,16).toUpperCase()}`,input.countDate,input.reason,Number(net),actor.userId,actor.userId,now,now));
  for(const l of lines)steps.push(write.prepare("INSERT INTO inventory_count_lines(id,count_id,product_id,product_version,description,unit,snapshot_quantity_milli,snapshot_value_minor,snapshot_version,counted_quantity_milli,counted_value_minor,unit_cost_minor,ordinal) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(l.id,record,l.product_id,l.product_version,l.description,l.unit,l.snapshot_quantity_milli,l.snapshot_value_minor,l.snapshot_version,l.counted_quantity_milli,l.counted_value_minor,l.unit_cost_minor,l.ordinal));
  return executeCommand(write,actor,key,operation,input,record,version,{id:record,status:"draft",version},steps);
}
export async function transitionCount(actor:WorkContext,id:string,action:"submit"|"reject"|"cancel"|"approve"|"post",input:z.infer<typeof countActionSchema>,key:string){
  actor = requirePermission(actor,action === "post" || action === "approve" || action === "reject" ? "inventory.approve" : "inventory.prepare");
  const {write}=await holdingResources(actor.holdingId),operation=`inventory.count.${action}`,prior=await replayCommand(write,actor,key,operation,id,input);if(prior)return prior;
  const {count,lines}=await countDetail(actor,id),review=action==="post"||action==="approve"||action==="reject";
  if(review?!canReviewCount(actor,count):!canEditCount(actor,count))throw new ApiError(403,"COUNT_ACTION_FORBIDDEN",review?"An independent holding finance approver must review a submitted stock count.":"Only the preparer can submit or cancel an editable count.");
  if(action!=="submit"&&!input.reason)throw new ApiError(422,"REASON_REQUIRED","Record the reason for this decision.");
  const status=action==="submit"?"submitted":action==="post"?"posted":action==="approve"?"approved":action==="reject"?"rejected":"cancelled",now=Date.now();
  const steps=[countGuard(write,key,count,input.expectedVersion,action==="post"?["approved"]:action==="approve"?["submitted"]:review?["submitted","approved"]:["draft","rejected"])];let journalId:string|null=null;
  if(action==="submit"||action==="approve"||action==="post"){
    if(!lines.length)throw new ApiError(409,"EMPTY_COUNT","Enter the counted products first.");
    steps.push(assertion(write,key,"warehouse","EXISTS(SELECT 1 FROM commerce_warehouses WHERE id=? AND company_id=? AND status='active')",[count.warehouse_id,actor.companyId]));
    for(const [index,line] of lines.entries())steps.push(stockGuard(write,key,index,count.warehouse_id,line),assertion(write,key,`product-${index}`,"EXISTS(SELECT 1 FROM commerce_products WHERE id=? AND version=? AND status='active')",[line.product_id,line.product_version]));
  }
  if(action==="post"){
    const config=await write.prepare("SELECT s.*,a.id variance_account_id FROM commerce_settings s JOIN finance_accounts a ON a.chart_id=s.chart_id AND a.company_id=s.company_id AND a.code='5400' AND a.category='expense' AND a.status='active' WHERE s.company_id=?").bind(actor.companyId).first<CountRow>();if(!config)throw new ApiError(409,"VARIANCE_MAPPING_REQUIRED","Configure the stock count variance account (5400) in the operations ledger.");
    const periods=await write.prepare("SELECT id FROM finance_periods WHERE company_id=? AND state='open' AND start_date<=? AND end_date>=?").bind(actor.companyId,count.count_date,count.count_date).all<CountRow>();if(periods.results.length!==1)throw new ApiError(409,"PERIOD_UNAVAILABLE","Exactly one open period must cover the count date.");
    let gains=0,losses=0;
    for(const [index,line] of lines.entries()){
      const qty=Number(line.counted_quantity_milli)-Number(line.snapshot_quantity_milli),value=Number(line.counted_value_minor)-Number(line.snapshot_value_minor);if(value>0)gains+=value;else losses-=value;
      steps.push(assertion(write,key,`date-${index}`,"NOT EXISTS(SELECT 1 FROM inventory_ledger WHERE warehouse_id=? AND product_id=? AND movement_date>?)",[count.warehouse_id,line.product_id,count.count_date]));
      if(Number(line.snapshot_version)===0)steps.push(write.prepare("INSERT INTO commerce_stock(warehouse_id,product_id) VALUES(?,?)").bind(count.warehouse_id,line.product_id));
      steps.push(write.prepare("UPDATE commerce_stock SET quantity_milli=?,value_minor=?,version=version+1 WHERE warehouse_id=? AND product_id=?").bind(line.counted_quantity_milli,line.counted_value_minor,count.warehouse_id,line.product_id));
      steps.push(write.prepare("INSERT INTO inventory_adjustment_moves(id,count_id,warehouse_id,product_id,quantity_milli,value_minor,movement_date,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(`${id}-${index}`,id,count.warehouse_id,line.product_id,qty,value,count.count_date,now));
    }
    if(!Number.isSafeInteger(gains+losses))throw new ApiError(422,"VALUE_OVERFLOW","Stock count exceeds supported precision.");
    steps.push(assertion(write,key,"open-period","EXISTS(SELECT 1 FROM finance_periods WHERE id=? AND state='open')",[periods.results[0].id]));
    if(gains||losses){const journal=await prepareJournal(write,actor,{chartId:String(config.chart_id),periodId:String(periods.results[0].id),entryDate:String(count.count_date),currency:String(config.currency),memo:`${count.number} · ${input.reason}`,sourceType:"stock_count",sourceId:id,isOpening:false,lines:[...(gains?[{accountId:String(config.inventory_account_id),debit:minorToDecimal(gains),credit:"0"},{accountId:String(config.variance_account_id),debit:"0",credit:minorToDecimal(gains)}]:[]),...(losses?[{accountId:String(config.variance_account_id),debit:minorToDecimal(losses),credit:"0"},{accountId:String(config.inventory_account_id),debit:"0",credit:minorToDecimal(losses)}]:[])]},key,"posted");journalId=journal.result.id;steps.push(...journal.steps);}
  }
  steps.push(write.prepare("UPDATE inventory_counts SET status=?,review_reason=?,approved_by=CASE WHEN ?='approve' THEN ? ELSE approved_by END,submitted_at=CASE WHEN ?='submit' THEN ? ELSE submitted_at END,posted_at=CASE WHEN ?='post' THEN ? ELSE posted_at END,journal_id=COALESCE(?,journal_id),version=version+1,updated_at=? WHERE id=?").bind(status,input.reason??null,action,actor.userId,action,now,action,now,journalId,now,id));
  if(action!=="submit")steps.push(write.prepare("INSERT INTO inventory_count_decisions(id,count_id,command_id,action,actor_id,reviewed_version,reason,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),id,key,action,actor.userId,input.expectedVersion,input.reason,now));
  if(review)steps.push(write.prepare("INSERT INTO decision_evidence(id,command_id,record_id,action,actor_id,assignment_id,reviewed_version,input_digest,created_at) VALUES(?,?,?,?,?,?,?,?,?)").bind(crypto.randomUUID(),key,id,operation,actor.userId,actor.assignmentId,input.expectedVersion,await sha256(JSON.stringify({reason:input.reason,lines})),now));
  return executeCommand(write,actor,key,operation,input,id,input.expectedVersion+1,{id,status,version:input.expectedVersion+1,journalId},steps);
}
