import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {commercePackageEnforcementMode,readPurchasedAccesses} from "@erpx/server-commercial/public/purchase-command-boundary";
import {packageClockSql,purchasedCapabilityPredicate} from "@erpx/server-commercial/public/purchased-capability-sql";
import {commerceKindCapability,requireCommerceOrderRead,type CommercePurchaseActor} from "@erpx/server-commerce/adapters/purchase-boundary";

type Order={id:string;kind:"quote"|"sales"|"purchase";state:string};
/** Work's flow enrichment must not restore actions removed by a purchase or
 * current source decision. Draft/approval work needs active purchase; retained
 * fulfillment and cancellation remain available during draining. */
export async function commerceWorkAccess(database:SqlDatabase,actor:CommercePurchaseActor,ids:readonly string[]){
 const mode=await commercePackageEnforcementMode();if(mode==="disabled")return null;
 if(!hasPermission(actor,"commerce.read"))return new Map<string,{history:boolean;canAct:boolean}>();
 const access=await readPurchasedAccesses(database,mode,actor.companyId,["sales.quotes","sales.orders","supply.purchasing"]);
 const rows=ids.length?(await database.prepare(`SELECT o.id,o.kind,o.state FROM commerce_orders o WHERE o.company_id=? AND o.id IN (${ids.map(()=>"?").join(",")}) AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?) AND ${assignmentReadSql("o","commerce.read")}`)
  .bind(actor.companyId,...ids,actor.branchId,actor.branchId,actor.departmentId,actor.departmentId,actor.assignmentId,actor.userId,actor.role).all<Order>()).results:[];
 return new Map(rows.map(row=>{const allowed=access[commerceKindCapability(row.kind)],newWork=["draft","submitted"].includes(row.state)||row.kind==="quote"&&row.state==="confirmed";return [row.id,{history:allowed.history,canAct:newWork?allowed.newWork:allowed.settleExisting}];}));
}
/** Reference mode uses the same pre-enrichment owner gates as normal Work. */
export async function commerceWorkReferenceRows(database:SqlDatabase,actor:WorkContext){
 const rows=(await database.prepare("SELECT * FROM commerce_orders WHERE company_id=? ORDER BY updated_at DESC").bind(actor.companyId).all<Record<string,string|number|null>>()).results;
 const access=await commerceWorkAccess(database,actor,rows.map(row=>String(row.id)));
 return access===null?rows:rows.filter(row=>access.get(String(row.id))?.history);
}

/** The actual owner lookup is inside the predicate, before a consumer's LIMIT.
 * Shared catalogue/party/warehouse references have no Sales/Supply gate here. */
export async function commerceSourceReadScope(database:SqlDatabase,actor:WorkContext,surface:"staff"|"inbox"|"records"){
 const mode=await commercePackageEnforcementMode();if(mode!=="enforced"){
  if(mode==="shadow")await readPurchasedAccesses(database,mode,actor.companyId,["sales.quotes","sales.orders","supply.purchasing"]);
  return {where:"TRUE",args:[] as unknown[]};
 }
 const id=surface==="staff"?"staff_notifications.record_id":surface==="inbox"?"e.source_id":"workspace_records.id";
 const company=surface==="staff"?"staff_notifications.company_id":surface==="inbox"?"e.company_id":"workspace_records.company_id";
 const type=surface==="staff"?"staff_notifications.record_type":"e.source_type";
 const actual=`o.id=${id} AND o.company_id=${company}`;
 const purchase=(["quote","sales","purchase"] as const).map(kind=>`(o.kind='${kind}' AND ${purchasedCapabilityPredicate("o.company_id",commerceKindCapability(kind),"history","commerce_source_clock.now_ms")})`).join(" OR ");
 const visible=`EXISTS(WITH commerce_source_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms) SELECT 1 FROM commerce_orders o CROSS JOIN commerce_source_clock WHERE ${actual} AND (${purchase}) AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?) AND ${assignmentReadSql("o","commerce.read")})`;
 const relevant=surface==="records"?`(module_id IN ('sales','procurement') AND EXISTS(SELECT 1 FROM commerce_orders o WHERE o.id=${id}))`:`${type}='commerce_order'`;
 if(!hasPermission(actor,"commerce.read"))return {where:`NOT (${relevant})`,args:[]};
 return {where:`(NOT (${relevant}) OR ${visible})`,args:[actor.branchId,actor.branchId,actor.departmentId,actor.departmentId,actor.assignmentId,actor.userId,actor.role]};
}
export async function requireCommerceWorkspaceRecordRead(database:SqlDatabase,actor:WorkContext,moduleId:string,id:string){
 if(!["sales","procurement"].includes(moduleId)||await commercePackageEnforcementMode()==="disabled")return;
 if(await database.prepare("SELECT 1 FROM commerce_orders WHERE id=?").bind(id).first())await requireCommerceOrderRead(database,actor,id);
}
