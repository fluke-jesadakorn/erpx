import {withAssetPurchasedRead,assetPurchaseControls} from "@erpx/server-assets/adapters/purchases";
import { finalRootOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as maintenanceProjectionSources } from "@erpx/server-maintenance/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as organizationProjectionSources } from "@erpx/server-organization/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as people_payrollProjectionSources } from "@erpx/server-people_payroll/public/final-root-owner-projection-sources";
import 'server-only';
import {createAssetApplication, type AssetRepository} from '@erpx/server-assets/application/assets';
import {type Asset} from '@erpx/server-assets/domain/assets';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {Row} from '@erpx/server-commerce/public/commerce';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {requirePermission} from '@erpx/server-identity_access/public/iam-permissions';
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {hasModuleAccess} from '@erpx/catalogue/module-access';
import {ApiError} from '@erpx/server-runtime/public/http';
const assetScope="a.company_id=? AND (?::text IS NULL OR a.branch_id=?) AND (?::text IS NULL OR a.department_id=?)",args=(a:WorkContext)=>[a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId];
const reviewer=(alias:'a'|'w')=>`EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) x JOIN (${people_payrollProjectionSources.hr_people}) p ON p.user_id=x.user_id AND p.company_id=x.company_id JOIN (${people_payrollProjectionSources.hr_engagements}) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE x.company_id=${alias}.company_id AND x.user_id=? AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(x.user_id,x.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(x.role,x.company_id,'records.review') AND (x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=${alias}.branch_id) OR (x.scope_kind='department' AND x.scope_node_id=${alias}.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))`;
function domain(a:WorkContext,_module:'assets',write=false){requirePermission(a,write?'records.write':'records.read');if(!hasModuleAccess(a.modules,'assets'))throw new ApiError(403,'MODULE_FORBIDDEN','Your current role cannot access assets.');}
function bindRepository(write: SqlDatabase, a: WorkContext): AssetRepository {
 return {
  items:async()=>(await write.prepare(`SELECT a.*,(SELECT legal_name FROM (${people_payrollProjectionSources.hr_people}) hr_people WHERE company_id=a.company_id AND user_id=a.custodian_id AND merged_into_id IS NULL LIMIT 1) custodian_name,CASE WHEN ${reviewer('a')} THEN 1 ELSE 0 END can_review FROM operational_assets a WHERE ${assetScope} ORDER BY a.updated_at DESC,a.id`).bind(a.userId,...args(a)).all<Asset>()).results,
  people:async()=>(await write.prepare(`SELECT DISTINCT p.user_id id,p.legal_name name FROM (${people_payrollProjectionSources.hr_people}) p JOIN (${people_payrollProjectionSources.hr_engagements}) e ON e.person_id=p.id WHERE p.company_id=? AND p.user_id IS NOT NULL AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' ORDER BY p.legal_name`).bind(a.companyId).all<Row>()).results,
  nodes:async()=>(await write.prepare(`SELECT id,kind,display_name,branch_id FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE company_id=? AND status='active' AND kind IN ('branch','headquarters','department') ORDER BY kind,display_name`).bind(a.companyId).all<Row>()).results,
  financeCandidates:async()=>(await write.prepare(`SELECT f.id,f.asset_number,f.name,f.currency,f.cost_minor,f.version,f.status FROM (${financeProjectionSources.finance_assets}) f WHERE f.company_id=? AND f.status IN ('in_service','retired') AND NOT EXISTS(SELECT 1 FROM operational_assets a WHERE a.finance_asset_id=f.id) ORDER BY f.asset_number`).bind(a.companyId).all<Row>()).results,
  finance:id=>write.prepare(`SELECT id,asset_number,status,currency,cost_minor,accumulated_depreciation_minor,version FROM (${financeProjectionSources.finance_assets}) finance_assets WHERE id=? AND company_id=?`).bind(id,a.companyId).first<Row>(),
  history:async id=>(await write.prepare('SELECT * FROM operational_asset_events WHERE asset_id=? ORDER BY version').bind(id).all<Row>()).results,
  maintenance:async id=>(await write.prepare(`SELECT id,title,status,priority,owner_id,scheduled_at,version FROM (${maintenanceProjectionSources.maintenance_work_orders}) maintenance_work_orders WHERE asset_id=? ORDER BY updated_at DESC,id`).bind(id).all<Row>()).results,
 };
}
function application(a:WorkContext,provided?:SqlDatabase){return createAssetApplication({actorId:a.userId,authorize:write=>domain(a,'assets',write),canReadFinance:()=>hasPermission(a,'finance.read'),canWrite:()=>hasPermission(a,'records.write'),canWriteFinance:()=>hasPermission(a,'finance.write'),error:(status,code,message)=>new ApiError(status,code,message),repository:async()=>bindRepository(provided??(await holdingResources(a.holdingId)).write,a)});}
export async function assetWorkspace(a:WorkContext){return withAssetPurchasedRead(a,"assets.register",async db=>({...await application(a,db).assetWorkspace(),purchases:await assetPurchaseControls(a)}));}
export async function assetDetail(a:WorkContext,id:string){return withAssetPurchasedRead(a,"assets.register",async db=>({...await application(a,db).assetDetail(id),purchases:await assetPurchaseControls(a)}));}
