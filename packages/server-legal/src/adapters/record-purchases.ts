import "server-only";
import {hasModuleAccess} from "@erpx/catalogue/module-access";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import {assignmentReadSql} from "@erpx/server-identity_access/public/record-source-authority";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {ApiError} from "@erpx/server-runtime/public/http";
import {legalPackageEnforcementMode,checkPurchasedCapability,readPurchasedAccesses} from "@erpx/server-commercial/public/purchase-command-boundary";
import {legalKindCapability} from "@erpx/server-legal/domain/purchases";
import type {LegalKind} from "@erpx/server-legal/domain/model";
import {visibility} from "@erpx/server-legal/adapters/legal/access";
import {legalItemPurchasePredicate,legalReleasePredicate} from "@erpx/server-legal/adapters/purchases";
function nativeSource(actor:WorkContext,id:string,company:string,purchased=true){const scope=visibility(actor,"legal_native");return {where:`EXISTS(SELECT 1 FROM legal_items legal_native WHERE legal_native.id=${id} AND legal_native.company_id=${company} AND ${scope.sql} AND ${hasModuleAccess(actor.modules,"legal")&&hasPermission(actor,"legal.read")?"TRUE":"FALSE"} AND ${assignmentReadSql("legal_native","legal.read")} AND ${legalReleasePredicate(actor,"legal_native")} ${purchased?`AND ${legalItemPurchasePredicate("legal_native")}`:""})`,args:[...scope.values,actor.assignmentId,actor.userId,actor.role]};}
/** Only persisted Legal sources are classified. Unrelated generic records keep
 * their original policy; a notification with a missing native source is hidden. */
export async function legalSourceReadScope(database:SqlDatabase,actor:WorkContext,surface:"records"|"staff"|"inbox"){
 const mode=await legalPackageEnforcementMode();if(mode!=="enforced"){if(mode==="shadow")await readPurchasedAccesses(database,mode,actor.companyId,["legal.contracts","legal.documents","legal.matters","legal.obligations","legal.access"]);return {where:"TRUE",args:[] as (string|number|null)[]};}
 const id=surface==="records"?"workspace_records.id":surface==="staff"?"staff_notifications.record_id":"e.source_id",company=surface==="records"?"workspace_records.company_id":surface==="staff"?"staff_notifications.company_id":"e.company_id",scope=nativeSource(actor,id,company);
 if(surface==="records")return {where:`(NOT(module_id='legal' AND EXISTS(SELECT 1 FROM legal_items actual_legal_source WHERE actual_legal_source.id=${id})) OR ${scope.where})`,args:scope.args};
 const type=surface==="staff"?"staff_notifications.record_type":"e.source_type";return {where:`(${type}<>'legal' OR ${scope.where})`,args:scope.args};
}
export async function requireLegalWorkspaceRecordRead(database:SqlDatabase,actor:WorkContext,moduleId:string,id:string){
 const mode=await legalPackageEnforcementMode();if(mode==="disabled"||moduleId!=="legal")return;
 const native=await database.prepare('SELECT kind FROM legal_items WHERE id=?').bind(id).first<{kind:LegalKind}>();if(!native)return;
 const scope=nativeSource(actor,"workspace_records.id","workspace_records.company_id",false);if(!await database.prepare(`SELECT 1 FROM workspace_records WHERE module_id='legal' AND id=? AND ${scope.where}`).bind(id,...scope.args).first())throw new ApiError(404,"LEGAL_SOURCE_NOT_FOUND","The actual Legal source is unavailable in this workspace.");
 await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:legalKindCapability(native.kind),purpose:"history"});
}
