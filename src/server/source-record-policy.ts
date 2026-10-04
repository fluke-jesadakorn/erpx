import "server-only";
import {z} from "zod";
import type {SourceRecordReference} from "@/lib/source-record";
import {sourceRecordLabel} from "@/lib/source-record";
import {hasModuleAccess} from "@/data/module-access";
import type {WorkContext} from "./authorization";
import type {SqlDatabase,SqlStatement} from "./runtime/types";
import {holdingResources} from "./runtime";
import {assertion} from "./commands";
import {ApiError} from "./http";
import {requirePermission} from "./iam-permissions";
import {pmsSourceReadableSql,pmsSourcesSql} from "./pms/source-policy";
import {readablePmsSource,pmsSourceLink} from "./pms/source";
import {getTask,taskSourceReadGuard} from "./staff-tasks";
import {readablePost,sourceGuard} from "./social-source";

export const sourceRecordKind=z.enum(["commerce_order","support_ticket","field_service_order","staff_task","pos_sale","pms_record","asset","maintenance_plan","maintenance_work"]);
export const sourceRecordInput=z.object({kind:sourceRecordKind,id:z.string().min(1).max(180),version:z.number().int().positive().optional()});
type SourceKind=z.infer<typeof sourceRecordKind>;
type SourceRow={id:string;company_id:string;branch_id:string|null;department_id:string|null;version:number;title:string;status:string};
export type AuthorizedSourceRecord=SourceRecordReference & {
  kind:SourceKind;version:number;title:string;companyId:string;branchId:string|null;departmentId:string|null;status:string;
};
const definitions:Record<Exclude<SourceKind,"staff_task"|"pms_record">,{table:string;permission:string;title:string;status:string;href:string}>={
  commerce_order:{table:"commerce_orders",permission:"commerce.read",title:"s.number",status:"s.state",href:"/commerce/orders/"},
  support_ticket:{table:"support_tickets",permission:"support.read",title:"s.title",status:"s.status",href:"/support/"},
  field_service_order:{table:"field_service_orders",permission:"records.read",title:"s.issue",status:"s.status",href:"/field-service/"},
  pos_sale:{table:"pos_sales",permission:"pos.read",title:"s.number",status:"s.state",href:"/pos?sale="},
  asset:{table:"operational_assets",permission:"records.read",title:"s.name",status:"s.lifecycle_status",href:"/assets/"},
  maintenance_plan:{table:"maintenance_preventive_plans",permission:"records.read",title:"s.title",status:"s.status",href:"/maintenance/plans/"},
  maintenance_work:{table:"maintenance_work_orders",permission:"records.read",title:"s.title",status:"s.status",href:"/maintenance/"},
};
function assignmentReadSql(alias:string,permission:string) {
  return `EXISTS(SELECT 1 FROM assignments g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=${alias}.company_id
    AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
    AND iam_role_allows(g.role,g.company_id,'${permission}')
    AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=${alias}.branch_id OR g.scope_kind='department' AND g.scope_node_id=${alias}.department_id))`;
}
/** Reuses the owning source's live authority. Titles and links are never persisted as grants. */
export async function readSourceRecord(a:WorkContext,raw:z.infer<typeof sourceRecordInput>,db?:SqlDatabase):Promise<AuthorizedSourceRecord> {
  const input=sourceRecordInput.parse(raw),write=db??(await holdingResources(a.holdingId)).write;
  let row:SourceRow|null=null,href:string;
  if(input.kind==="staff_task") {
    requirePermission(a,"records.read");
    const current=await write.prepare("SELECT 1 FROM assignments WHERE id=? AND company_id=? AND user_id=? AND role=? AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?) AND iam_role_allows(role,company_id,'records.read')").bind(a.assignmentId,a.companyId,a.userId,a.role,Date.now(),Date.now()).first();
    if(!current)throw new ApiError(404,"SOURCE_RECORD_NOT_FOUND","Source record is unavailable in this work scope.");
    const task=await getTask(write,a,input.id);
    row={id:task.id,company_id:task.company_id,branch_id:task.branch_id,department_id:task.department_id,version:Number(task.version),title:task.title,status:task.state};href=`/staff-tasks/${encodeURIComponent(input.id)}`;
  } else if(input.kind==="pms_record") {
    requirePermission(a,"pms.read");
    if(a.departmentId)throw new ApiError(404,"SOURCE_RECORD_NOT_FOUND","Source record is unavailable in this work scope.");
    const current=await readablePmsSource(write,a,input.id),link=pmsSourceLink(current);
    row={id:current.id,company_id:current.company_id,branch_id:current.branch_id,department_id:null,version:Number(current.version),title:link.title,status:current.data.state??"open"};href=link.href;
  } else {
    const d=definitions[input.kind];if(input.kind!=="support_ticket")requirePermission(a,d.permission);
    if(["asset","field_service_order"].includes(input.kind)&&!hasModuleAccess(a.modules,input.kind==="asset"?"assets":"field_service"))throw new ApiError(403,"MODULE_FORBIDDEN","This source's owning workspace is not available to the selected role.");
    if(["maintenance_plan","maintenance_work"].includes(input.kind)&&!hasModuleAccess(a.modules,"maintenance"))throw new ApiError(403,"MODULE_FORBIDDEN","This source's owning workspace is not available to the selected role.");
    if(input.kind==="pos_sale"&&a.departmentId)throw new ApiError(404,"SOURCE_RECORD_NOT_FOUND","Source record is unavailable in this work scope.");
    // Support's owner controls include employee self-report visibility.
    if(input.kind==="support_ticket") {
      const ticket=await (await import("./support")).getSupportTicket(a,input.id);
      row={id:String(ticket.id),company_id:String(ticket.company_id),branch_id:ticket.branch_id?String(ticket.branch_id):null,department_id:ticket.department_id?String(ticket.department_id):null,version:Number(ticket.version),title:String(ticket.title),status:String(ticket.status)};
    } else {
      const department=input.kind==="pos_sale"?"NULL::text":"s.department_id";
      row=await write.prepare(`SELECT s.id,s.company_id,s.branch_id,${department} department_id,s.version,${d.title} title,${d.status} status FROM ${d.table} s
        WHERE s.id=? AND s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND (?::text IS NULL OR ${department}=?) AND ${assignmentReadSql("s",d.permission).replaceAll("s.department_id",department)}`)
        .bind(input.id,a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId,a.assignmentId,a.userId,a.role).first<SourceRow>();
    }
    href=`${d.href}${encodeURIComponent(input.id)}`;
  }
  if(!row)throw new ApiError(404,"SOURCE_RECORD_NOT_FOUND","Source record is unavailable in this work scope.");
  if(input.version!==undefined&&input.version!==Number(row.version))throw new ApiError(409,"SOURCE_RECORD_CHANGED","The source version changed. Reload its owning workspace before handing it off.");
  return {kind:input.kind,id:row.id,version:Number(row.version),title:row.title,href,label:`${sourceRecordLabel(input.kind)} · ${row.title}`,companyId:row.company_id,branchId:row.branch_id,departmentId:row.department_id,status:row.status};
}
export async function visibleSourceRecord(a:WorkContext,input:z.infer<typeof sourceRecordInput>,db?:SqlDatabase) {
  try{return await readSourceRecord(a,input,db);}catch(error){if(error instanceof ApiError&&[403,404].includes(error.status))return null;throw error;}
}
/** Current source version and current read grant commit with every derived command. */
export async function sourceRecordGuards(db:SqlDatabase,a:WorkContext,key:string,source:AuthorizedSourceRecord,step="source-record"):Promise<SqlStatement[]> {
  await readSourceRecord(a,{kind:source.kind,id:source.id,version:source.version},db);
  if(source.kind==="staff_task") {
    const task=await getTask(db,a,source.id),guards=[taskSourceReadGuard(db,a,source.id,source.version,key,step)];
    if(task.source_post_id)guards.push(sourceGuard(db,key,await readablePost(db,a,task.source_post_id),a.userId,`${step}-post`));
    return guards;
  }
  if(source.kind==="pms_record")return [assertion(db,key,step,`EXISTS(SELECT 1 FROM (${pmsSourcesSql}) s WHERE s.id=? AND s.company_id=? AND s.version=? AND ${pmsSourceReadableSql("s","?::text","?::text")})`,[source.id,a.companyId,source.version,a.userId,a.assignmentId])];
  if(source.kind==="support_ticket")return (await import("./support")).supportTicketSourceGuards(db,a,key,source.id,source.version,step);
  const d=definitions[source.kind],department=source.kind==="pos_sale"?"NULL::text":"s.department_id";
  return [db.prepare(`SELECT id FROM ${d.table} WHERE id=? AND company_id=? FOR SHARE`).bind(source.id,a.companyId),
    assertion(db,key,step,`EXISTS(SELECT 1 FROM ${d.table} s WHERE s.id=? AND s.company_id=? AND s.version=? AND (?::text IS NULL OR s.branch_id=?) AND (?::text IS NULL OR ${department}=?)
      AND ${assignmentReadSql("s",d.permission).replaceAll("s.department_id",department)})`,[source.id,a.companyId,source.version,a.branchId,a.branchId,a.departmentId,a.departmentId,a.assignmentId,a.userId,a.role])];
}
