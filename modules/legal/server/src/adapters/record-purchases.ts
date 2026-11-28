import "server-only";
import {hasModuleAccess} from "@erpx/platform-catalogue/module-access";
import {hasPermission} from "@erpx/identity-access-ui/lib/permissions";
import type {WorkContext} from "@erpx/identity-access-server/public/authorization";
import {assignmentReadSql} from "@erpx/identity-access-server/public/record-source-authority";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {ApiError} from "@erpx/platform-runtime-server/public/http";
import {legalPackageEnforcementMode,checkPurchasedCapability,readPurchasedAccesses} from "@erpx/commercial-server/public/purchase-command-boundary";
import {legalKindCapability} from "@erpx/legal-server/domain/purchases";
import type {LegalKind} from "@erpx/legal-server/domain/model";
import {visibility} from "@erpx/legal-server/adapters/legal/access";
import {legalItemPurchasePredicate,legalReleasePredicate} from "@erpx/legal-server/adapters/purchases";
import {legalPointByKeySql} from './legal-read-tail-r36-sources';
import contract from './legal-record-sources.json' with {type:'json'};
const fill = (template: string, values: Record<string, string>) => {
  let rendered = template;
  for (const [name, value] of Object.entries(values)) {
    const token = `{${name}}`;
    if (!rendered.includes(token)) throw new Error('LEGAL_RECORD_SOURCES_CONTRACT_INVALID');
    rendered = rendered.split(token).join(value);
  }
  return rendered;
};
function nativeSource(actor:WorkContext,id:string,company:string,purchased=true){const scope=visibility(actor,"legal_native");return {where:fill(contract.nativeSource,{id,company,scope:scope.sql,permission:hasModuleAccess(actor.modules,"legal")&&hasPermission(actor,"legal.read")?"TRUE":"FALSE",assignment:assignmentReadSql("legal_native","legal.read"),release:legalReleasePredicate(actor,"legal_native"),purchase:purchased?`AND ${legalItemPurchasePredicate("legal_native")}`:""}),args:[...scope.values,actor.assignmentId,actor.userId,actor.role]};}
/** Only persisted Legal sources are classified. Unrelated generic records keep
 * their original policy; a notification with a missing native source is hidden. */
export async function legalSourceReadScope(database:SqlDatabase,actor:WorkContext,surface:"records"|"staff"|"inbox"){
 const mode=await legalPackageEnforcementMode();if(mode!=="enforced"){if(mode==="shadow")await readPurchasedAccesses(database,mode,actor.companyId,["legal.contracts","legal.documents","legal.matters","legal.obligations","legal.access"]);return {where:"TRUE",args:[] as (string|number|null)[]};}
 const id=surface==="records"?"workspace_records.id":surface==="staff"?"staff_notifications.record_id":"e.source_id",company=surface==="records"?"workspace_records.company_id":surface==="staff"?"staff_notifications.company_id":"e.company_id",scope=nativeSource(actor,id,company);
 if(surface==="records")return {where:`(NOT(module_id='legal' AND ${fill(contract.actualSource,{id})}) OR ${scope.where})`,args:scope.args};
 const type=surface==="staff"?"staff_notifications.record_type":"e.source_type";return {where:`(${type}<>'legal' OR ${scope.where})`,args:scope.args};
}
export async function requireLegalWorkspaceRecordRead(database:SqlDatabase,actor:WorkContext,moduleId:string,id:string){
 const mode=await legalPackageEnforcementMode();if(mode==="disabled"||moduleId!=="legal")return;
 const native=await database.prepare(legalPointByKeySql("kind","legal_items","id")).bind(id).first<{kind:LegalKind}>();if(!native)return;
 const scope=nativeSource(actor,"workspace_records.id","workspace_records.company_id",false);if(!await database.prepare(fill(contract.workspaceGuard,{scope:scope.where})).bind(id,...scope.args).first())throw new ApiError(404,"LEGAL_SOURCE_NOT_FOUND","The actual Legal source is unavailable in this workspace.");
 await checkPurchasedCapability(database,mode,{companyId:actor.companyId,capability:legalKindCapability(native.kind),purpose:"history"});
}
