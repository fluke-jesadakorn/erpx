import "server-only";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import type {CapabilityPurpose} from "@erpx/server-commercial/public/rules/packages";
import {commercePackageEnforcementMode,readPurchasedAccess} from "@erpx/server-commercial/public/purchase-command-boundary";
import {purchasedCapabilityPredicate} from "@erpx/server-commercial/public/purchased-capability-sql";
import {qualityPurchaseSourceRows} from "@erpx/server-quality/adapters/purchase-sources";
import {qualityClockCte} from "@erpx/server-quality/adapters/quality-scope-sql";
import {qualityAlertHoldId,requireQualityPurchasedRead} from "@erpx/server-quality/adapters/purchases";
import {qualitySourceArmHead} from "./quality-source-arm-head";
import {nativeQualityReadFirst,RD_HOLD_EXISTS,RD_SPEC_EXISTS} from "./native-quality-read";
/** Fixed owner capability intersection; callers supply only server-owned aliases. */
export function qualitySourcePurchasePredicate(alias:string,purpose:CapabilityPurpose,clock:string){
 const cap=(capability:string)=>purchasedCapabilityPredicate(`${alias}.company_id`,capability,purpose,clock);
 return `((${alias}.kind='advanced' AND ${cap("supply.quality")}) OR (${alias}.kind='receipt' AND ${cap("supply.receiving")} AND ${cap("supply.stock")}) OR (${alias}.kind='customer-return' AND ${cap("sales.orders")} AND ${cap("inventory.stock-core")}))`;
}
export async function qualitySourceReadScope(database:SqlDatabase,actor:WorkContext,surface:"staff"|"inbox"|"records"){
 const mode=await commercePackageEnforcementMode();
 if(mode!=="enforced"){if(mode==="shadow")await readPurchasedAccess(database,mode,{companyId:actor.companyId,capability:"supply.quality"});return {where:"TRUE",args:[] as unknown[]};}
 const id=surface==="staff"?"staff_notifications.record_id":surface==="inbox"?"e.source_id":"workspace_records.id",company=surface==="staff"?"staff_notifications.company_id":surface==="inbox"?"e.company_id":"workspace_records.company_id",type=surface==="staff"?"staff_notifications.record_type":"e.source_type";
 const relevant=surface==="records"?`EXISTS(${qualitySourceArmHead('1','inventory_quality_holds actual_hold',` WHERE actual_hold.id=${id}`)}) OR EXISTS(${qualitySourceArmHead('1','inventory_quality_alerts actual_alert',` WHERE actual_alert.id=${id}`)}) OR EXISTS(${qualitySourceArmHead('1','inventory_quality_specs actual_spec',` WHERE actual_spec.id=${id}`)})`:`${type} IN ('quality_hold','inventory_quality_hold','quality_alert','inventory_quality_alert','inventory_quality_spec')`;
 if(actor.departmentId||!hasPermission(actor,"quality.read"))return {where:`NOT (${relevant})`,args:[]};
 const native=`${qualitySourceArmHead("s.id,s.company_id,s.branch_id,NULL::text department_id,s.kind",`(${qualityPurchaseSourceRows}) s`)} UNION ALL ${qualitySourceArmHead("a.id,s.company_id,s.branch_id,NULL::text,s.kind",`inventory_quality_alerts a JOIN (${qualityPurchaseSourceRows}) s ON s.id=a.hold_id`)} UNION ALL ${qualitySourceArmHead("id,company_id,NULL::text,NULL::text,'advanced'","inventory_quality_specs")}`;
 const visible=`EXISTS(${qualityClockCte("quality_source_clock")} SELECT 1 FROM (${native}) quality_source CROSS JOIN quality_source_clock WHERE quality_source.id=${id} AND quality_source.company_id=${company} AND (?::text IS NULL OR quality_source.branch_id IS NULL OR quality_source.branch_id=?) AND ${assignmentReadSql("quality_source","quality.read")} AND ${qualitySourcePurchasePredicate("quality_source","history","quality_source_clock.now_ms")})`;
 return {where:`(NOT (${relevant}) OR ${visible})`,args:[actor.branchId,actor.branchId,actor.assignmentId,actor.userId,actor.role]};
}
export async function requireQualityWorkspaceRecordRead(database:SqlDatabase,actor:WorkContext,_moduleId:string,id:string){
 if(await commercePackageEnforcementMode()==="disabled")return;
 const holdId=await qualityAlertHoldId(database,id);
 if(holdId){await requireQualityPurchasedRead(database,actor,holdId);return;}
 if(await nativeQualityReadFirst(RD_HOLD_EXISTS,[id],database)){await requireQualityPurchasedRead(database,actor,id);return;}
 if(await nativeQualityReadFirst(RD_SPEC_EXISTS,[id],database))await requireQualityPurchasedRead(database,actor);
}
