import {withAssetPurchasedRead,assetPurchaseControls} from "@erpx/server-assets/public/purchases";
import {finalRootOwnerProjectionSources as organizationProjectionSources} from "@erpx/server-organization/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as assetsProjectionSources } from "@erpx/server-assets/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as inventoryProjectionSources } from "@erpx/server-inventory/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as people_payrollProjectionSources } from "@erpx/server-people_payroll/public/final-root-owner-projection-sources";
import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {Row} from '@erpx/server-commerce/public/commerce';
import type {Work,MaintenanceDowntimeEvidenceRow} from '@erpx/server-maintenance/domain/work';
export type {MaintenanceDowntimeEvidenceRow} from '@erpx/server-maintenance/domain/work';
import {createMaintenanceApplication,type MaintenanceRepository} from '@erpx/server-maintenance/application/work';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {requirePermission} from '@erpx/server-identity_access/public/iam-permissions';
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {hasModuleAccess} from '@erpx/catalogue/module-access';
import {ApiError} from '@erpx/server-runtime/public/http';
import {availableServiceSpares} from '@erpx/server-inventory/public/inventory-consumption';
import {maintenanceOwnerName} from './maintenance-owner-name';
import {iamScopeGuardSql} from './iam-scope-guard';
import {notExistsGuardPrefix,projectionHead} from './projection-lookup';
import {settlementRefSource} from './asset-settlement-refs';
const assetScope="a.company_id=? AND (?::text IS NULL OR a.branch_id=?) AND (?::text IS NULL OR a.department_id=?)",workScope="w.company_id=? AND (?::text IS NULL OR w.branch_id=?) AND (?::text IS NULL OR w.department_id=?)",args=(a:WorkContext)=>[a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId];
/** Canonical owner-local Work/Asset source projection shared by the workspace
 * items and downtime-evidence reads. Both call the same owner-fixed Work join
 * to the operational Asset projection and project the same code/name columns;
 * callers pass only closed field/join/scope/order fragments, never request-
 * authored SQL or relation names. The single row-projection head is now the
 * shared settlement-ref renderer, so no second head is carried in the TS read
 * tail. Each composed read keeps its pre-dedupe SQL tokens (whitespace-only
 * reflow for the downtime read) and no LOCK statement is accepted or changed.
 * r3-r17 families stay unchanged. */
export const workAssetSource = (fields: string, assetJoin: string, where: string) =>
  settlementRefSource(fields, `maintenance_work_orders w JOIN (${assetsProjectionSources.operational_assets}) a ON a.id=w.asset_id${assetJoin}`, ` WHERE ${where}`);
/** Owner-local row scan over a closed projection source, shared by the Work
 * assets, org-nodes and consumptions read tails (r21). It reuses the single
 * row-projection head so no second query head is carried in the TS read tail;
 * callers supply only fixed field/alias/tail fragments from this owner's closed
 * projection relations, never request-authored SQL, and each composed read stays
 * byte-identical to its pre-dedupe literal. Distinct from the r18 workAssetSource
 * (fixed Work/Asset join): this is a top-level multi-row scan with an arbitrary
 * closed tail. No LOCK or native read is accepted or changed. */
export const projectionRowScan = (fields: string, source: string, alias: string, tail: string) =>
  projectionHead(fields, source, alias, tail);
/** Owner-local adapter over the shared IAM guard: this owner supplies only its
 * closed projection relations, fixed aliases and a row alias/user/permission;
 * the common assignment/principal/role/status tail is owned once in the helper. */
const workIamFrom=`(${identity_accessProjectionSources.assignments}) x JOIN (${people_payrollProjectionSources.hr_people}) p ON p.user_id=x.user_id AND p.company_id=x.company_id JOIN (${people_payrollProjectionSources.hr_engagements}) e ON e.person_id=p.id AND e.company_id=p.company_id`;
const iamScopeGuard=(alias:string,user:string,permission:'records.write'|'records.review')=>iamScopeGuardSql(workIamFrom,'x','p','e',alias,user,permission,`(x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=${alias}.branch_id) OR (x.scope_kind='department' AND x.scope_node_id=${alias}.department_id))`);
const worker=iamScopeGuard('w','w.owner_id','records.write');
const reviewer=(alias:'a'|'w')=>iamScopeGuard(alias,'?','records.review');
function access(a:WorkContext,write=false){requirePermission(a,write?'records.write':'records.read');if(!hasModuleAccess(a.modules,'maintenance'))throw new ApiError(403,'MODULE_FORBIDDEN','Your current role cannot access maintenance.');}
function bindRepository(write:SqlDatabase,a:WorkContext,canReadParts=true):MaintenanceRepository {
 return {
  items:async()=>(await write.prepare(`${workAssetSource(`w.*,a.code asset_code,a.name asset_name,CASE WHEN ${worker} THEN 1 ELSE 0 END owner_eligible,CASE WHEN ${reviewer('w')} THEN 1 ELSE 0 END can_review,${maintenanceOwnerName('w')} owner_name`,"",workScope)} ORDER BY w.updated_at DESC,w.id`).bind(a.userId,...args(a)).all<Work>()).results,
  assets:async()=>(await write.prepare(`${projectionRowScan("a.id,a.code,a.name,a.version,a.condition,a.lifecycle_status",assetsProjectionSources.operational_assets,"a",`WHERE ${assetScope} AND a.lifecycle_status IN ('in_service','registered','maintenance') AND ${notExistsGuardPrefix('maintenance_work_orders w')}w.asset_id=a.id AND w.status NOT IN ('completed','cancelled')) ORDER BY a.code`)}`).bind(...args(a)).all<Row>()).results,
  nodes:async()=>(await write.prepare(`${projectionRowScan("id,kind,display_name,branch_id",organizationProjectionSources.organization_nodes,"n","WHERE company_id=? AND status='active' AND kind IN ('branch','headquarters','department') AND (?::text IS NULL OR id=? OR branch_id=?) AND (?::text IS NULL OR id=?) ORDER BY kind,display_name")}`).bind(a.companyId,a.branchId,a.branchId,a.branchId,a.departmentId,a.departmentId).all<Row>()).results,
  spares:branchId=>canReadParts?availableServiceSpares(write,a,branchId):Promise.resolve({stock:[],lots:[]}),
  consumptions:async id=>canReadParts?(await write.prepare(`${projectionRowScan("p.*,j.status journal_status,product.name product_name",inventoryProjectionSources.operational_inventory_consumptions,"p",`JOIN (${financeProjectionSources.finance_journals}) j ON j.id=p.journal_id JOIN (${commerceProjectionSources.commerce_products}) product ON product.id=p.product_id WHERE p.source_kind='maintenance' AND p.source_id=? ORDER BY p.created_at,p.id`)}`).bind(id).all<Row>()).results:[],
  history:async id=>(await write.prepare(settlementRefSource("*","maintenance_work_events"," WHERE work_order_id=? ORDER BY version")).bind(id).all<Row>()).results,
  async downtime(input){ const where=[workScope,"w.status IN ('completed','cancelled')","w.downtime_started_at IS NOT NULL","w.downtime_ended_at IS NOT NULL","w.downtime_ended_at>w.downtime_started_at","w.downtime_started_at<?","w.downtime_ended_at>?"];const values:unknown[]=[...args(a),input.to,input.from];
 if(input.query){where.push("position(lower(?) in lower(concat_ws(' ',w.title,a.code,a.name)))>0");values.push(input.query);}
 if(input.cursor){where.push("(w.downtime_ended_at,w.id)<(?::bigint,?)");values.push(input.cursor.endedAt,input.cursor.id);}
 if(input.recordId){where.push("w.id=?");values.push(input.recordId);}
 values.push(input.recordId?1:Math.max(1,Math.min(51,input.limit+1)));
 return (await write.prepare(`${workAssetSource(`w.id,w.version::text version,w.status,w.title,a.code asset_code,a.name asset_name,w.downtime_started_at::text downtime_started_at,w.downtime_ended_at::text downtime_ended_at`," AND a.company_id=w.company_id",where.join(' AND '))} ORDER BY w.downtime_ended_at DESC,w.id DESC LIMIT ?`).bind(...values).all<MaintenanceDowntimeEvidenceRow>()).results;
 }
 };
}
function application(a:WorkContext,providedWrite?:SqlDatabase,canReadParts=true){return createMaintenanceApplication({actorId:a.userId,authorize:write=>access(a,write),canRead:()=>hasPermission(a,'records.read')&&hasModuleAccess(a.modules,'maintenance'),canWrite:()=>hasPermission(a,'records.write'),canReview:()=>hasPermission(a,'records.review'),clock:{now:()=>Date.now()},error:(status,code,message)=>new ApiError(status,code,message),repository:async()=>bindRepository(providedWrite??(await holdingResources(a.holdingId)).write,a,canReadParts)});}
export async function maintenanceWorkspace(a:WorkContext){return withAssetPurchasedRead(a,"assets.maintenance",async db=>({...await application(a,db).maintenanceWorkspace(),purchases:await assetPurchaseControls(a)}));}
export async function maintenanceDetail(a:WorkContext,id:string){return withAssetPurchasedRead(a,"assets.maintenance",async db=>{const purchases=await assetPurchaseControls(a);return {...await application(a,db,purchases?.["assets.parts-costs"]?.history!==false).maintenanceDetail(id),purchases};});}
export async function maintenanceDowntimeEvidenceRows(a:WorkContext,input:Parameters<ReturnType<typeof createMaintenanceApplication>['maintenanceDowntimeEvidenceRows']>[0],provided?:SqlDatabase){return withAssetPurchasedRead(a,"assets.downtime",db=>application(a,db).maintenanceDowntimeEvidenceRows(input),provided);}
export async function maintenanceWork(a:WorkContext){if(!hasPermission(a,"records.read")||!hasModuleAccess(a.modules,"maintenance"))return [];const purchase=(await assetPurchaseControls(a,["assets.maintenance"]))?.["assets.maintenance"];if(purchase&&!purchase.history)return [];const items=await withAssetPurchasedRead(a,"assets.maintenance",db=>application(a,db).maintenanceWork());return purchase&&!purchase.settleExisting?items.map(item=>({...item,canAct:false,queue:"waiting" as const})):items;}
