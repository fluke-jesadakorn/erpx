import "server-only";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {CapabilityPurpose} from "@erpx/server-commercial/public/rules/packages";
import {commercePackageEnforcementMode,readPurchasedAccess} from "@erpx/server-commercial/public/purchase-command-boundary";
import {packageClockSql,purchasedCapabilityPredicate} from "@erpx/server-commercial/public/purchased-capability-sql";
import {qualityPurchaseSourceRows} from "@erpx/server-quality/adapters/purchase-sources";
import {requireQualityPurchasedRead} from "@erpx/server-quality/adapters/purchases";
/** Fixed owner capability intersection; callers supply only server-owned aliases. */
export function qualitySourcePurchasePredicate(alias:string,purpose:CapabilityPurpose,clock:string){
 const cap=(capability:string)=>purchasedCapabilityPredicate(`${alias}.company_id`,capability,purpose,clock);
 return `((${alias}.kind='advanced' AND ${cap("supply.quality")}) OR (${alias}.kind='receipt' AND ${cap("supply.receiving")} AND ${cap("supply.stock")}) OR (${alias}.kind='customer-return' AND ${cap("sales.orders")} AND ${cap("inventory.stock-core")}))`;
}
export async function qualitySourceReadScope(database:SqlDatabase,actor:WorkContext,surface:"staff"|"inbox"|"records"){
 const mode=await commercePackageEnforcementMode();
 if(mode!=="enforced"){if(mode==="shadow")await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:"supply.quality"});return {where:"TRUE",args:[] as unknown[]};}
 const id=surface==="staff"?"staff_notifications.record_id":surface==="inbox"?"e.source_id":"workspace_records.id",company=surface==="staff"?"staff_notifications.company_id":surface==="inbox"?"e.company_id":"workspace_records.company_id",type=surface==="staff"?"staff_notifications.record_type":"e.source_type";
 const relevant=surface==="records"?`EXISTS(SELECT 1 FROM inventory_quality_holds actual_hold WHERE actual_hold.id=${id}) OR EXISTS(SELECT 1 FROM inventory_quality_alerts actual_alert WHERE actual_alert.id=${id}) OR EXISTS(SELECT 1 FROM inventory_quality_specs actual_spec WHERE actual_spec.id=${id})`:`${type} IN ('quality_hold','inventory_quality_hold','quality_alert','inventory_quality_alert','inventory_quality_spec')`;
 if(actor.departmentId||!hasPermission(actor,"quality.read"))return {where:`NOT (${relevant})`,args:[]};
 const native=`SELECT s.id,s.company_id,s.branch_id,NULL::text department_id,s.kind FROM (${qualityPurchaseSourceRows}) s UNION ALL SELECT a.id,s.company_id,s.branch_id,NULL::text,s.kind FROM inventory_quality_alerts a JOIN (${qualityPurchaseSourceRows}) s ON s.id=a.hold_id UNION ALL SELECT id,company_id,NULL::text,NULL::text,'advanced' FROM inventory_quality_specs`;
 const visible=`EXISTS(WITH quality_source_clock AS MATERIALIZED (SELECT ${packageClockSql} now_ms) SELECT 1 FROM (${native}) quality_source CROSS JOIN quality_source_clock WHERE quality_source.id=${id} AND quality_source.company_id=${company} AND (?::text IS NULL OR quality_source.branch_id IS NULL OR quality_source.branch_id=?) AND ${assignmentReadSql("quality_source","quality.read")} AND ${qualitySourcePurchasePredicate("quality_source","history","quality_source_clock.now_ms")})`;
 return {where:`(NOT (${relevant}) OR ${visible})`,args:[actor.branchId,actor.branchId,actor.assignmentId,actor.userId,actor.role]};
}
export async function requireQualityWorkspaceRecordRead(database:SqlDatabase,actor:WorkContext,_moduleId:string,id:string){
 if(await commercePackageEnforcementMode()==="disabled")return;
 const alert=await database.prepare("SELECT hold_id FROM inventory_quality_alerts WHERE id=?").bind(id).first<{hold_id:string}>();
 if(alert){await requireQualityPurchasedRead(database,actor,alert.hold_id);return;}
 if(await database.prepare("SELECT 1 FROM inventory_quality_holds WHERE id=?").bind(id).first()){await requireQualityPurchasedRead(database,actor,id);return;}
 if(await database.prepare("SELECT 1 FROM inventory_quality_specs WHERE id=?").bind(id).first())await requireQualityPurchasedRead(database,actor);
}
