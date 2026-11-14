import {withAssetPurchasedRead,assetPurchaseControls} from "@erpx/server-assets/public/purchases";
import { finalRootOwnerProjectionSources as assetsProjectionSources } from "@erpx/server-assets/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as people_payrollProjectionSources } from "@erpx/server-people_payroll/public/final-root-owner-projection-sources";
import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {requirePermission} from '@erpx/server-identity_access/public/iam-permissions';
import {ApiError} from '@erpx/server-runtime/public/http';
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {hasModuleAccess} from '@erpx/catalogue/module-access';
import {createPreventiveApplication,type PreventiveRepository} from '@erpx/server-maintenance/application/preventive-plan';
import type {Plan,PreventivePlanRow} from '@erpx/server-maintenance/domain/preventive-plan';
export {preventivePlanSchema,preventiveActionSchema} from '@erpx/server-maintenance/domain/preventive-plan';
const scope='p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.department_id=?)';
const scopeArgs=(a:WorkContext)=>[a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId];
const eligible=(user:string)=>`EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) x JOIN (${people_payrollProjectionSources.hr_people}) h ON h.user_id=x.user_id AND h.company_id=x.company_id JOIN (${people_payrollProjectionSources.hr_engagements}) e ON e.person_id=h.id AND e.company_id=h.company_id
 WHERE x.company_id=p.company_id AND x.user_id=${user} AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
 AND iam_principal_active(x.user_id,x.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(x.role,x.company_id,'records.write')
 AND (x.scope_kind='holding' OR x.scope_kind='branch' AND x.scope_node_id=p.branch_id OR x.scope_kind='department' AND x.scope_node_id=p.department_id)
 AND h.status='active' AND h.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))`;
function access(a:WorkContext,write=false){requirePermission(a,write?'records.write':'records.read');if(!hasModuleAccess(a.modules,'maintenance'))throw new ApiError(403,'MODULE_FORBIDDEN','Your current role cannot access Maintenance.');}
function bindRepository(db:SqlDatabase,a:WorkContext):PreventiveRepository {
 return {
  async plans(){return (await db.prepare(`SELECT p.*,a.code asset_code,a.name asset_name,CASE WHEN ${eligible('p.owner_id')} THEN 1 ELSE 0 END owner_eligible,
 (SELECT legal_name FROM (${people_payrollProjectionSources.hr_people}) hr_people WHERE company_id=p.company_id AND user_id=p.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name
 FROM maintenance_preventive_plans p JOIN (${assetsProjectionSources.operational_assets}) a ON a.id=p.asset_id WHERE ${scope} ORDER BY p.next_due_at,p.id LIMIT 300`).bind(...scopeArgs(a)).all<PreventivePlanRow>()).results;},
  plan:id=>db.prepare(`SELECT p.*,a.code asset_code,a.name asset_name FROM maintenance_preventive_plans p JOIN (${assetsProjectionSources.operational_assets}) a ON a.id=p.asset_id WHERE p.id=? AND ${scope}`).bind(id,...scopeArgs(a)).first<Plan>(),
  async assetVersion(id){return db.prepare(`SELECT version FROM (${assetsProjectionSources.operational_assets}) operational_assets WHERE id=? AND company_id=?`).bind(id,a.companyId).first<{version:number}>();},
  async people(id){return (await db.prepare(`SELECT DISTINCT person.user_id id,person.legal_name name FROM maintenance_preventive_plans p JOIN (${people_payrollProjectionSources.hr_people}) person ON person.company_id=p.company_id WHERE p.id=? AND person.status='active' AND person.merged_into_id IS NULL AND ${eligible('person.user_id')} ORDER BY person.legal_name`).bind(id).all<{id:string;name:string}>()).results;},
  history:async id=>(await db.prepare('SELECT * FROM maintenance_preventive_events WHERE plan_id=? ORDER BY version').bind(id).all<Record<string,unknown>>()).results,
  occurrences:async id=>(await db.prepare('SELECT o.*,w.status,w.version work_version FROM maintenance_preventive_occurrences o JOIN maintenance_work_orders w ON w.id=o.work_order_id WHERE o.plan_id=? ORDER BY o.due_at DESC').bind(id).all<Record<string,unknown>>()).results,
 };
}
function application(a:WorkContext,providedWrite?:SqlDatabase){return createPreventiveApplication({actorId:a.userId,authorize:write=>access(a,write),canRead:()=>hasPermission(a,'records.read')&&hasModuleAccess(a.modules,'maintenance'),canWrite:()=>hasPermission(a,'records.write'),canReview:()=>hasPermission(a,'records.review'),clock:{now:()=>Date.now()},error:(status,code,message)=>new ApiError(status,code,message),repository:async()=>bindRepository(providedWrite??(await holdingResources(a.holdingId)).write,a)});}
export async function preventivePlans(a:WorkContext,providedWrite?:SqlDatabase){return withAssetPurchasedRead(a,"assets.preventive",db=>application(a,db).preventivePlans(),providedWrite);}
export async function preventivePlanDetail(a:WorkContext,id:string){return withAssetPurchasedRead(a,"assets.preventive",async db=>({...await application(a,db).preventivePlanDetail(id),purchases:await assetPurchaseControls(a)}));}
export async function preventivePlanWork(a:WorkContext){if(!hasPermission(a,"records.read")||!hasModuleAccess(a.modules,"maintenance"))return [];const purchase=(await assetPurchaseControls(a,["assets.preventive"]))?.["assets.preventive"];if(purchase&&!purchase.history)return [];const items=await withAssetPurchasedRead(a,"assets.preventive",db=>application(a,db).preventivePlanWork());return purchase&&!purchase.newWork?items.map(item=>({...item,canAct:false,queue:"waiting" as const})):items;}
