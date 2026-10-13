import { finalRootOwnerProjectionSources as assetsProjectionSources } from "../../assets/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as commerceProjectionSources } from "../../commerce/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as financeProjectionSources } from "../../finance/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "../../identity_access/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as inventoryProjectionSources } from "../../inventory/public/final-root-owner-projection-sources";
import { finalRootOwnerProjectionSources as people_payrollProjectionSources } from "../../people_payroll/public/final-root-owner-projection-sources";
import 'server-only';
import type {WorkContext} from '../../identity_access/public/authorization';
import type {SqlDatabase,SqlStatement} from '../../runtime/public/runtime/types';
import type {Row} from '../../commerce/public/commerce';
import type {Work,MaintenanceDowntimeEvidenceRow} from '../domain/work';
export type {MaintenanceDowntimeEvidenceRow} from '../domain/work';
import {createMaintenanceApplication,type MaintenanceRepository} from '../application/work';
import {holdingResources} from '../../runtime/public/runtime';
import {requirePermission} from '../../identity_access/public/iam-permissions';
import {hasPermission} from "@/lib/identity_access/permissions";
import {hasModuleAccess} from '@/data/module-access';
import {assertion,executeCommand,replayCommand} from '../../workflow/public/commands';
import {ApiError} from '../../runtime/public/http';
import {availableServiceSpares,prepareInventoryConsumption} from '../../inventory/public/inventory-consumption';
import {lockAssetForMaintenance,prepareAssetMaintenanceChange} from '../../assets/public/maintenance-transaction';
import {prepareMaintenanceNotification} from "../../collaboration/public/maintenance-notifications";
const assetScope="a.company_id=? AND (?::text IS NULL OR a.branch_id=?) AND (?::text IS NULL OR a.department_id=?)",workScope="w.company_id=? AND (?::text IS NULL OR w.branch_id=?) AND (?::text IS NULL OR w.department_id=?)",args=(a:WorkContext)=>[a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId];
const worker=`EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) x JOIN (${people_payrollProjectionSources.hr_people}) p ON p.user_id=x.user_id AND p.company_id=x.company_id JOIN (${people_payrollProjectionSources.hr_engagements}) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE x.company_id=w.company_id AND x.user_id=w.owner_id AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(x.user_id,x.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(x.role,x.company_id,'records.write') AND (x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=w.branch_id) OR (x.scope_kind='department' AND x.scope_node_id=w.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))`;
const reviewer=(alias:'a'|'w')=>`EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) x JOIN (${people_payrollProjectionSources.hr_people}) p ON p.user_id=x.user_id AND p.company_id=x.company_id JOIN (${people_payrollProjectionSources.hr_engagements}) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE x.company_id=${alias}.company_id AND x.user_id=? AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(x.user_id,x.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(x.role,x.company_id,'records.review') AND (x.scope_kind='holding' OR (x.scope_kind='branch' AND x.scope_node_id=${alias}.branch_id) OR (x.scope_kind='department' AND x.scope_node_id=${alias}.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))`;
function access(a:WorkContext,write=false){requirePermission(a,write?'records.write':'records.read');if(!hasModuleAccess(a.modules,'maintenance'))throw new ApiError(403,'MODULE_FORBIDDEN','Your current role cannot access maintenance.');}
function bindRepository(write:SqlDatabase,a:WorkContext):MaintenanceRepository {
 let consumption:SqlStatement[]=[];
 return {
  replay:c=>replayCommand(write,a,c.key,c.operation,c.id,c.input),
  items:async()=>(await write.prepare(`SELECT w.*,a.code asset_code,a.name asset_name,CASE WHEN ${worker} THEN 1 ELSE 0 END owner_eligible,CASE WHEN ${reviewer('w')} THEN 1 ELSE 0 END can_review,(SELECT legal_name FROM (${people_payrollProjectionSources.hr_people}) hr_people WHERE company_id=w.company_id AND user_id=w.owner_id AND merged_into_id IS NULL LIMIT 1) owner_name FROM maintenance_work_orders w JOIN (${assetsProjectionSources.operational_assets}) a ON a.id=w.asset_id WHERE ${workScope} ORDER BY w.updated_at DESC,w.id`).bind(a.userId,...args(a)).all<Work>()).results,
  assets:async()=>(await write.prepare(`SELECT a.id,a.code,a.name,a.version,a.condition,a.lifecycle_status FROM (${assetsProjectionSources.operational_assets}) a WHERE ${assetScope} AND a.lifecycle_status IN ('in_service','registered','maintenance') AND NOT EXISTS(SELECT 1 FROM maintenance_work_orders w WHERE w.asset_id=a.id AND w.status NOT IN ('completed','cancelled')) ORDER BY a.code`).bind(...args(a)).all<Row>()).results,
  people:async()=>(await write.prepare(`SELECT DISTINCT p.user_id id,p.legal_name name FROM (${people_payrollProjectionSources.hr_people}) p JOIN (${identity_accessProjectionSources.assignments}) x ON x.user_id=p.user_id AND x.company_id=p.company_id WHERE p.company_id=? AND p.status='active' AND p.merged_into_id IS NULL AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(x.role,x.company_id,'records.write') ORDER BY p.legal_name`).bind(a.companyId).all<Row>()).results,
  async work(id){return await write.prepare(`SELECT w.*,a.version asset_version,CASE WHEN ${worker} THEN 1 ELSE 0 END owner_eligible,CASE WHEN ${reviewer('w')} THEN 1 ELSE 0 END can_review FROM maintenance_work_orders w JOIN (${assetsProjectionSources.operational_assets}) a ON a.id=w.asset_id WHERE w.id=? AND ${workScope}`).bind(a.userId,id,...args(a)).first<Work&{asset_id:string;asset_version:number;scheduled_at:number|null;downtime_started_at:number|null}>();},
  spares:branchId=>availableServiceSpares(write,a,branchId),
  consumptions:async id=>(await write.prepare(`SELECT p.*,j.status journal_status,product.name product_name FROM (${inventoryProjectionSources.operational_inventory_consumptions}) p JOIN (${financeProjectionSources.finance_journals}) j ON j.id=p.journal_id JOIN (${commerceProjectionSources.commerce_products}) product ON product.id=p.product_id WHERE p.source_kind='maintenance' AND p.source_id=? ORDER BY p.created_at,p.id`).bind(id).all<Row>()).results,
  history:async id=>(await write.prepare('SELECT * FROM maintenance_work_events WHERE work_order_id=? ORDER BY version').bind(id).all<Row>()).results,
  async downtime(input){ const where=[workScope,"w.status IN ('completed','cancelled')","w.downtime_started_at IS NOT NULL","w.downtime_ended_at IS NOT NULL","w.downtime_ended_at>w.downtime_started_at","w.downtime_started_at<?","w.downtime_ended_at>?"];const values:unknown[]=[...args(a),input.to,input.from];
 if(input.query){where.push("position(lower(?) in lower(concat_ws(' ',w.title,a.code,a.name)))>0");values.push(input.query);}
 if(input.cursor){where.push("(w.downtime_ended_at,w.id)<(?::bigint,?)");values.push(input.cursor.endedAt,input.cursor.id);}
 if(input.recordId){where.push("w.id=?");values.push(input.recordId);}
 values.push(input.recordId?1:Math.max(1,Math.min(51,input.limit+1)));
 return (await write.prepare(`SELECT w.id,w.version::text version,w.status,w.title,a.code asset_code,a.name asset_name,
   w.downtime_started_at::text downtime_started_at,w.downtime_ended_at::text downtime_ended_at
   FROM maintenance_work_orders w JOIN (${assetsProjectionSources.operational_assets}) a ON a.id=w.asset_id AND a.company_id=w.company_id
   WHERE ${where.join(' AND ')} ORDER BY w.downtime_ended_at DESC,w.id DESC LIMIT ?`).bind(...values).all<MaintenanceDowntimeEvidenceRow>()).results;
 },
  async create(command,status,now){const {input,id,key,operation:op}=command;
return executeCommand(write,a,key,op,input,id,1,{id,version:1,status},[
 lockAssetForMaintenance(write,input.assetId),
 assertion(write,key,'asset',`EXISTS(SELECT 1 FROM (${assetsProjectionSources.operational_assets}) a WHERE a.id=? AND a.version=? AND a.lifecycle_status IN ('registered','in_service','maintenance') AND NOT EXISTS(SELECT 1 FROM maintenance_work_orders w WHERE w.asset_id=a.id AND w.status NOT IN ('completed','cancelled')) AND ${assetScope})`,[input.assetId,input.assetVersion,...args(a)]),
 write.prepare(`INSERT INTO maintenance_work_orders(id,company_id,branch_id,department_id,asset_id,title,priority,status,owner_id,scheduled_at,created_by,created_at,updated_at) SELECT ?,company_id,branch_id,department_id,id,?,?,?,?,?,?,?,? FROM (${assetsProjectionSources.operational_assets}) operational_assets WHERE id=?`).bind(id,input.title,input.priority,status,input.ownerId,input.scheduledAt??null,a.userId,now,now,input.assetId),
 assertion(write,key,'owner',`EXISTS(SELECT 1 FROM maintenance_work_orders w WHERE w.id=? AND ${worker})`,[id]),
 write.prepare('INSERT INTO maintenance_work_events VALUES(?,?,1,?,?,?,?,?,?,NULL,NULL,?)').bind(`maintenance-event-${key}`,id,'create',a.userId,input.reason,status,input.ownerId,input.scheduledAt??null,now),
 prepareMaintenanceNotification(write,key,id,true)
 ]);
  },
  async prepareConsumption(command,row){const {input,id,key}=command;if(input.action!=='issue-part')throw new Error('MAINTENANCE_CONSUMPTION_COMMAND_REQUIRED');const consumed=await prepareInventoryConsumption(write,a,{key,sourceKind:'maintenance',sourceId:id,sourceVersion:input.expectedVersion+1,branchId:String(row.branch_id),departmentId:row.department_id?String(row.department_id):null,warehouseId:input.warehouseId,productId:input.productId,quantityMilli:input.quantityMilli,entryDate:input.entryDate,reason:input.reason,allocations:input.allocations});consumption=consumed.steps;},
  async change(command,row,state,now){const {input,id,key,operation:op}=command;const {status,owner,scheduled,downtimeStart,downtimeEnd}=state;
   const steps=[lockAssetForMaintenance(write,row.asset_id),write.prepare('SELECT id FROM maintenance_work_orders WHERE id=? FOR UPDATE').bind(id),assertion(write,key,'current-worker',`EXISTS(SELECT 1 FROM maintenance_work_orders w WHERE w.id=? AND ((w.owner_id=? AND ${worker}) OR ${reviewer('w')}))`,[id,a.userId,a.userId]),assertion(write,key,'version',`EXISTS(SELECT 1 FROM maintenance_work_orders w WHERE w.id=? AND w.version=? AND ${workScope})`,[id,input.expectedVersion,...args(a)])];steps.push(...consumption);
if(['complete','cancel'].includes(input.action))steps.push(assertion(write,key,'parts-gl-posted',`NOT EXISTS(SELECT 1 FROM (${inventoryProjectionSources.operational_inventory_consumptions}) p JOIN (${financeProjectionSources.finance_journals}) j ON j.id=p.journal_id WHERE p.source_kind='maintenance' AND p.source_id=? AND j.status<>'posted')`,[id]));if(input.action==='schedule')steps.push(assertion(write,key,'owner',`EXISTS(SELECT 1 FROM maintenance_work_orders w WHERE w.id=? AND ${worker.replaceAll('w.owner_id','?::text')})`,[id,input.ownerId]));steps.push(write.prepare('UPDATE maintenance_work_orders SET status=?,owner_id=?,scheduled_at=?,downtime_started_at=?,downtime_ended_at=?,blocked_reason=?,completion_evidence=?,resulting_condition=?,version=version+1,updated_at=? WHERE id=?').bind(status,owner,scheduled,downtimeStart,downtimeEnd,input.action==='block'?input.reason:null,input.action==='complete'?input.evidence:null,input.action==='complete'?input.resultingCondition:null,now,id));
   steps.push(...prepareAssetMaintenanceChange(write,a,key,row,input,now));
 steps.push(write.prepare('INSERT INTO maintenance_work_events VALUES(?,?,?,?,?,?,?,?,?,?,?,?)').bind(`maintenance-event-${key}`,id,input.expectedVersion+1,input.action,a.userId,input.reason,status,owner,scheduled,downtimeStart,downtimeEnd,now),prepareMaintenanceNotification(write,key,id,false));return executeCommand(write,a,key,op,input,id,input.expectedVersion+1,{id,version:input.expectedVersion+1,status},steps);
  }
 };
}
function application(a:WorkContext,providedWrite?:SqlDatabase){return createMaintenanceApplication({actorId:a.userId,authorize:write=>access(a,write),canRead:()=>hasPermission(a,'records.read')&&hasModuleAccess(a.modules,'maintenance'),canWrite:()=>hasPermission(a,'records.write'),canReview:()=>hasPermission(a,'records.review'),clock:{now:()=>Date.now()},error:(status,code,message)=>new ApiError(status,code,message),repository:async()=>bindRepository(providedWrite??(await holdingResources(a.holdingId)).write,a)});}
export async function maintenanceWorkspace(a:WorkContext){return application(a).maintenanceWorkspace();}
export async function maintenanceDetail(a:WorkContext,id:string){return application(a).maintenanceDetail(id);}
export async function maintenanceDowntimeEvidenceRows(a:WorkContext,input:Parameters<ReturnType<typeof createMaintenanceApplication>['maintenanceDowntimeEvidenceRows']>[0],provided?:SqlDatabase){return application(a,provided).maintenanceDowntimeEvidenceRows(input);}
export async function createMaintenanceWork(a:WorkContext,raw:unknown,key:string,providedWrite?:SqlDatabase){return application(a,providedWrite).createMaintenanceWork(raw,key);}
export async function changeMaintenanceWork(a:WorkContext,id:string,raw:unknown,key:string){return application(a).changeMaintenanceWork(id,raw,key);}
export async function maintenanceWork(a:WorkContext){return application(a).maintenanceWork();}
