import "server-only";
import { z } from "zod";
import { hasPermission } from "@/lib/permissions";
import { metricKey, definitionSchema, filtersSchema, emptyDefinition, defaultFilters, shareSchema, sharePreviewSchema, transferOwnershipSchema, type Filters, type ReportView, type ReportSummary, type ShareView, type MetricData } from "@/lib/executive/canvas";
import type { SqlDatabase } from "../runtime/types";
import type { WorkAssignment, WorkContext } from "../authorization";
import { ApiError } from "../http";
import { assertion, executeCommand, replayCommand } from "../commands";
import { currentAuthority, canAuthor, requireAuthor, executiveEligible } from "./access";
import { metricBreakdowns, metricCatalogFor, metricSupportsComparison, metricUnavailable, queryMetric } from "./metrics";
import { htmlWidgetDocument } from "./html-widget";
import { executiveCoverage } from "@/lib/executive/coverage";
import { loadIamAuthority, iamRoles } from "../iam-store";
import { betaAccess } from "../beta-access";
import { modulesForRole } from "@/lib/business-roles";

type ReportRow={id:string;company_id:string;owner_id:string;definition_json:string;filters_json:string;version:number;updated_at:number};
type ShareRow={id:string;report_id:string;company_id:string;recipient_id:string;recipient_name:string;role:"viewer"|"editor";mode:"recipient"|"distribution";widget_ids_json:string;definition_json:string;filters_json:string;publisher_json:string;expires_at:number|null;revoked_at:number|null;version:number};
const missing=()=>new ApiError(404,"REPORT_NOT_FOUND","This report is unavailable or has not been shared with you.");
export async function reportAccess(db:SqlDatabase,a:WorkContext,id:string) {
  await currentAuthority(db,a);
  const r=await db.prepare("SELECT * FROM executive_reports WHERE id=? AND company_id=?").bind(id,a.companyId).first<ReportRow>();
  if(!r)throw missing();
  const owner=r.owner_id===a.userId;
  const share=owner?null:await db.prepare("SELECT * FROM executive_report_shares WHERE report_id=? AND company_id=? AND recipient_id=? AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at>?)").bind(id,a.companyId,a.userId,Date.now()).first<ShareRow>();
  if(owner)await requireAuthor(db,a);else if(!share)throw missing();
  if(share&&!await db.prepare("SELECT 1 FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=? AND p.user_id=? AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) LIMIT 1").bind(a.companyId,a.userId).first())throw missing();
  const definition=definitionSchema.parse(JSON.parse(share?.definition_json??r.definition_json));
  const filters=filtersSchema.parse(JSON.parse(share?.filters_json??r.filters_json));
  const canEdit=owner?hasPermission(a,"executive.report.edit"):share?.role==="editor"&&await canAuthor(db,a)&&hasPermission(a,"executive.report.edit");
  return {r,share,owner,definition,filters,canEdit:!!canEdit};
}
const shareView=(s:ShareRow):ShareView=>({id:s.id,recipientId:s.recipient_id,name:s.recipient_name,role:s.role,mode:s.mode,widgetIds:JSON.parse(s.widget_ids_json),expiresAt:s.expires_at,version:s.version});
export async function reportView(db:SqlDatabase,a:WorkContext,id:string):Promise<ReportView> {
  const x=await reportAccess(db,a,id);
  const shares=x.owner?(await db.prepare("SELECT * FROM executive_report_shares WHERE report_id=? AND revoked_at IS NULL ORDER BY created_at").bind(id).all<ShareRow>()).results.map(shareView):[];
  const revisions=x.owner?(await db.prepare("SELECT version,created_at FROM executive_report_revisions WHERE report_id=? ORDER BY version DESC LIMIT 30").bind(id).all<{version:number;created_at:number}>()).results.map(r=>({version:r.version,createdAt:r.created_at})):[];
  await currentAuthority(db,a);
  return {id,definitionVersion:1,owner:x.owner,canEdit:x.canEdit,canShare:x.owner&&hasPermission(a,"executive.report.share"),version:x.r.version,definition:x.definition,filters:x.filters,lockedFilters:!!x.share,updatedAt:x.r.updated_at,shares,revisions};
}
export async function reportLibrary(db:SqlDatabase,a:WorkContext) {
  await currentAuthority(db,a);
  const author=await canAuthor(db,a);
  const rows=(await db.prepare(`SELECT r.*,s.definition_json shared_definition FROM executive_reports r LEFT JOIN executive_report_shares s ON s.report_id=r.id AND s.recipient_id=? AND s.revoked_at IS NULL AND (s.expires_at IS NULL OR s.expires_at>?) WHERE r.company_id=? AND ((r.owner_id=? AND ?=1) OR s.id IS NOT NULL) ORDER BY r.updated_at DESC LIMIT 200`).bind(a.userId,Date.now(),a.companyId,a.userId,author?1:0).all<ReportRow&{shared_definition:string|null}>()).results;
  const reports:ReportSummary[]=rows.map(r=>{const d=definitionSchema.parse(JSON.parse(r.owner_id===a.userId?r.definition_json:r.shared_definition!));return {id:r.id,title:d.title,description:d.description,theme:d.theme,version:r.version,updatedAt:r.updated_at,owner:r.owner_id===a.userId,shared:!!r.shared_definition,widgets:d.widgets.length};});
  const accessSetup=hasPermission(a,"iam.manage")&&hasPermission(a,"people.read")&&!a.branchId&&!a.departmentId?(await db.prepare(`SELECT DISTINCT p.id,p.legal_name name,l.level_number level,
    EXISTS(SELECT 1 FROM assignments a WHERE a.company_id=p.company_id AND a.user_id=p.user_id AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>? AND iam_assignment_active(a.id,?) AND iam_role_allows(a.role,a.company_id,'executive.access') AND iam_role_allows(a.role,a.company_id,'executive.report.create')) ready
    FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id JOIN staff_levels l ON l.id=e.level_id AND l.company_id=e.company_id
    WHERE p.company_id=? AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND l.active=1 AND l.level_number IN (1,2)
    AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) ORDER BY l.level_number,p.legal_name LIMIT 100`).bind(Date.now(),Date.now(),Date.now(),a.companyId).all<{id:string;name:string;level:number;ready:boolean}>()).results:[];
  return {canCreate:author,reports,catalog:await metricCatalogFor(db,a),coverage:executiveCoverage(),eligible:await executiveEligible(db,a),accessSetup};
}
export async function createReport(db:SqlDatabase,a:WorkContext,key:string,title="Untitled report") {
  await requireAuthor(db,a);const definition=emptyDefinition(title),filters=defaultFilters(),id=`report-${key}`,now=Date.now();
  return executeCommand(db,a,key,"executive.report.create",{title},id,1,{id},[
    db.prepare("INSERT INTO executive_reports(id,company_id,owner_id,definition_json,filters_json,created_at,updated_at) VALUES(?,?,?,?,?,?,?)").bind(id,a.companyId,a.userId,JSON.stringify(definition),JSON.stringify(filters),now,now),
    db.prepare("INSERT INTO executive_report_revisions(report_id,version,definition_json,filters_json,author_id,created_at) VALUES(?,1,?,?,?,?)").bind(id,JSON.stringify(definition),JSON.stringify(filters),a.userId,now),
  ]);
}
export const saveSchema=z.object({expectedVersion:z.number().int().positive(),definition:definitionSchema,filters:filtersSchema}).strict();
function previousPeriod(filters:Filters):Filters {
  const from=Date.parse(`${filters.from}T00:00:00Z`),to=Date.parse(`${filters.to}T00:00:00Z`),days=(to-from)/86_400_000+1;
  const previousTo=from-86_400_000,previousFrom=previousTo-(days-1)*86_400_000;
  return filtersSchema.parse({from:new Date(previousFrom).toISOString().slice(0,10),to:new Date(previousTo).toISOString().slice(0,10),currency:filters.currency});
}
export async function saveReport(db:SqlDatabase,a:WorkContext,id:string,input:z.infer<typeof saveSchema>,key:string) {
  const x=await reportAccess(db,a,id);if(!x.canEdit)throw new ApiError(403,"REPORT_READ_ONLY","This report is shared as view only.");
  const replay=await replayCommand<{id:string;version:number}>(db,a,key,"executive.report.save",id,input);if(replay)return replay;
  if(!x.owner) {
    // Editors never gain the owner's hidden widgets or broaden an approved projection.
    if(JSON.stringify(definitionSchema.parse(JSON.parse(x.r.definition_json)))!==JSON.stringify(x.definition))throw new ApiError(409,"SHARED_REVISION_CHANGED","The owner has a newer design. Ask them to publish the current revision before editing.");
    const published=x.definition.widgets.map(w=>w.id).sort().join(",");
    const current=definitionSchema.parse(JSON.parse(x.r.definition_json)).widgets.map(w=>w.id).sort().join(",");
    if(published!==current||input.definition.widgets.map(w=>w.id).sort().join(",")!==published||JSON.stringify(input.filters)!==JSON.stringify(x.filters)||input.definition.widgets.some(w=>!x.definition.widgets.some(p=>p.id===w.id&&metricKey(p)===metricKey(w))))throw new ApiError(403,"SHARE_BOUNDS","Only the explicitly shared widgets can be edited. Ask the owner to add new data or change the approved filters.");
  }
  const allowed=(await metricCatalogFor(db,a)).filter(m=>m.available).map(m=>m.id);
  if(input.definition.widgets.some(w=>!allowed.includes(w.metric)||!metricBreakdowns(w.metric).includes(w.breakdown??"default")||!!w.comparePeriod&&!metricSupportsComparison(w.metric)))throw new ApiError(403,"METRIC_FORBIDDEN","A widget uses a metric or comparison unavailable to your selected role.");
  const version=input.expectedVersion+1,now=Date.now();
  return executeCommand(db,a,key,"executive.report.save",input,id,version,{id,version},[
    db.prepare("SELECT id FROM executive_reports WHERE id=? FOR UPDATE").bind(id),
    assertion(db,key,"report-version","EXISTS(SELECT 1 FROM executive_reports WHERE id=? AND version=?)",[id,input.expectedVersion]),
    ...(!x.owner?[assertion(db,key,"report-share","EXISTS(SELECT 1 FROM executive_report_shares WHERE id=? AND version=? AND revoked_at IS NULL AND (expires_at IS NULL OR expires_at>?))",[x.share!.id,x.share!.version,now])]:[]),
    db.prepare("UPDATE executive_reports SET definition_json=?,filters_json=?,version=?,updated_at=? WHERE id=?").bind(JSON.stringify(input.definition),JSON.stringify(input.filters),version,now,id),
    db.prepare("INSERT INTO executive_report_revisions(report_id,version,definition_json,filters_json,author_id,created_at) VALUES(?,?,?,?,?,?)").bind(id,version,JSON.stringify(input.definition),JSON.stringify(input.filters),a.userId,now),
    // An editor can continue their bounded design edits. Other recipients stay pinned.
    ...(!x.owner?[db.prepare("UPDATE executive_report_shares SET definition_json=?,version=version+1 WHERE id=?").bind(JSON.stringify(input.definition),x.share!.id)]:[]),
  ]);
}
export async function restoreReport(db:SqlDatabase,a:WorkContext,id:string,version:number,expectedVersion:number,key:string) {
  const x=await reportAccess(db,a,id);if(!x.owner)throw missing();
  const r=await db.prepare("SELECT definition_json,filters_json FROM executive_report_revisions WHERE report_id=? AND version=?").bind(id,version).first<{definition_json:string;filters_json:string}>();if(!r)throw missing();
  return saveReport(db,a,id,saveSchema.parse({expectedVersion,definition:JSON.parse(r.definition_json),filters:JSON.parse(r.filters_json)}),key);
}
export async function recipients(db:SqlDatabase,a:WorkContext,id:string,q:string) {
  const x=await reportAccess(db,a,id);if(!x.owner||!hasPermission(a,"executive.report.share"))throw missing();
  return (await db.prepare(`SELECT DISTINCT p.user_id id,p.legal_name name FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.company_id=? AND p.user_id IS NOT NULL AND p.user_id<>? AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) AND strpos(lower(p.legal_name),lower(?))>0 AND iam_principal_active(p.user_id,p.company_id,?) AND EXISTS(SELECT 1 FROM assignments a WHERE a.company_id=p.company_id AND a.user_id=p.user_id AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>? AND iam_assignment_active(a.id,?)) ORDER BY p.legal_name LIMIT 40`).bind(a.companyId,a.userId,q,Date.now(),Date.now(),Date.now(),Date.now()).all<{id:string;name:string}>()).results;
}
async function activeShareRecipient(db:SqlDatabase,a:WorkContext,recipientId:string) {
  const now=Date.now();
  return db.prepare(`SELECT p.legal_name FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
    WHERE p.company_id=? AND p.user_id=? AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active'
      AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))
      AND EXISTS(SELECT 1 FROM assignments r WHERE r.company_id=p.company_id AND r.user_id=p.user_id AND r.revoked_at IS NULL AND r.valid_from<=? AND r.valid_until>?
        AND iam_assignment_active(r.id,?) AND iam_principal_active(r.user_id,r.company_id,?)) LIMIT 1`).bind(a.companyId,recipientId,now,now,now,now).first<{legal_name:string}>();
}
async function validateShareCandidate(db:SqlDatabase,a:WorkContext,id:string,input:z.infer<typeof shareSchema>,validateDistributionReads=true) {
  await requireAuthor(db,a,"executive.report.share");const x=await reportAccess(db,a,id);if(!x.owner)throw missing();
  const person=await activeShareRecipient(db,a,input.recipientId);
  const target={...a,userId:input.recipientId,role:"employee",iam:undefined};
  if(!person||input.recipientId===a.userId)throw new ApiError(422,"RECIPIENT_INVALID","Choose an active employee in this company.");
  if(input.role==="editor"&&(!await executiveEligible(db,target)||input.mode==="distribution"))throw new ApiError(422,"EDITOR_INELIGIBLE","Editors must be eligible executives using their own source access.");
  if(input.expiresAt!==null&&input.expiresAt<=Date.now())throw new ApiError(422,"SHARE_EXPIRED","Choose a future expiry.");
  const widgets=x.definition.widgets.filter(w=>input.widgetIds.includes(w.id));
  if(widgets.length!==new Set(input.widgetIds).size)throw new ApiError(422,"WIDGET_INVALID","Choose widgets from the saved report.");
  if(input.role==="editor"&&widgets.length!==x.definition.widgets.length)throw new ApiError(422,"EDITOR_SCOPE","Editors need the complete report. Use viewer access for selected widgets.");
  if(input.mode==="distribution") {
    if(!hasPermission(a,"executive.data.distribute"))throw new ApiError(403,"DISTRIBUTION_FORBIDDEN","Your role cannot distribute report data.");
    if(widgets.some(w=>["my-tasks","projects","overdue-project-milestones","project-dependency-blockers","project-actual-hours","project-posted-cost","routines","approved-leave-duration","authorized-overtime-duration","open-operations-exceptions","exception-resolution-time","operations-escalations"].includes(w.metric)))throw new ApiError(422,"PERSONAL_METRIC","Personal work metrics can only be shared using the recipient's own access.");
    if(widgets.some(w=>["available-stock","inventory-shortages","inventory-movements"].includes(w.metric)))throw new ApiError(422,"RECIPIENT_SOURCE_REQUIRED","Inventory metrics can reveal product, warehouse, reorder threshold or source movement details; share them only when each recipient reads through their own Inventory access.");
    if(widgets.some(w=>w.metric==="hotel-occupancy"))throw new ApiError(422,"RECIPIENT_SOURCE_REQUIRED","Hotel occupancy aggregates remain recipient-scoped until aggregate disclosure and small-cohort rules are accepted.");
    if(widgets.some(w=>w.metric==="active-support-incidents"))throw new ApiError(422,"RECIPIENT_SOURCE_REQUIRED","Support incident counts remain recipient-scoped until aggregate disclosure and small-cohort rules are accepted.");
    if(widgets.some(w=>["active-subscriptions","overdue-invoices"].includes(w.metric)))throw new ApiError(422,"RECIPIENT_SOURCE_REQUIRED","Holding billing metrics are limited to Holding owners and cannot be distributed to report recipients.");
    if(widgets.some(w=>w.metric==="payroll-summary"))throw new ApiError(422,"RECIPIENT_SOURCE_REQUIRED","Payroll summaries must be read using each recipient's own Payroll permission.");
    if(validateDistributionReads)for(const widget of widgets){await queryMetric(db,a,widget.metric,input.filters,widget.breakdown);if(widget.comparePeriod)await queryMetric(db,a,widget.metric,previousPeriod(input.filters),widget.breakdown);}
  }
  const definition={...x.definition,widgets},shareId=`share-${id}-${input.recipientId}`;
  return {x,person,widgets,definition,shareId};
}
export async function saveShare(db:SqlDatabase,a:WorkContext,id:string,input:z.infer<typeof shareSchema>,key:string) {
  const {x,person,widgets,definition,shareId}=await validateShareCandidate(db,a,id,input);
  // Persist authority without browser/session credentials. Any authority revision change requires republishing.
  const publisher={...a,authority:undefined};
  return executeCommand(db,a,key,"executive.report.share",input,id,x.r.version,{id:shareId},[
    db.prepare("SELECT id FROM executive_reports WHERE id=? FOR UPDATE").bind(id),
    assertion(db,key,"share-report-version","EXISTS(SELECT 1 FROM executive_reports WHERE id=? AND owner_id=? AND version=?)",[id,a.userId,x.r.version]),
    assertion(db,key,"share-recipient","iam_principal_active(?,?,?) AND EXISTS(SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>? AND iam_assignment_active(id,?))",[input.recipientId,a.companyId,Date.now(),a.companyId,input.recipientId,Date.now(),Date.now(),Date.now()]),
    db.prepare(`INSERT INTO executive_report_shares(id,report_id,company_id,recipient_id,recipient_name,role,mode,widget_ids_json,definition_json,filters_json,publisher_json,expires_at,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(report_id,recipient_id) DO UPDATE SET recipient_name=excluded.recipient_name,role=excluded.role,mode=excluded.mode,widget_ids_json=excluded.widget_ids_json,definition_json=excluded.definition_json,filters_json=excluded.filters_json,publisher_json=excluded.publisher_json,expires_at=excluded.expires_at,revoked_at=NULL,version=executive_report_shares.version+1`).bind(shareId,id,a.companyId,input.recipientId,person.legal_name,input.role,input.mode,JSON.stringify(widgets.map(w=>w.id)),JSON.stringify(definition),JSON.stringify(input.filters),JSON.stringify(publisher),input.expiresAt,Date.now()),
  ]);
}
type PreviewAssignmentRow={id:string;role:string;modules_json:string;company_id:string;branch_id:string|null;version:number;valid_until:number;scope_kind:"holding"|"branch"|"department";scope_node_id:string|null;scope_name:string|null;scope_slug:string|null;scope_branch_id:string|null;scope_branch_name:string|null;scope_branch_slug:string|null;};
type PreviewAssignment={row:PreviewAssignmentRow;roleName:string;label:string;scope:string;current:boolean;permissions:string[]};
async function recipientPreviewAssignments(db:SqlDatabase,a:WorkContext,id:string,recipientId:string):Promise<PreviewAssignment[]> {
  const x=await reportAccess(db,a,id);if(!x.owner||!hasPermission(a,"executive.report.share"))throw missing();
  if(!await db.prepare(`SELECT 1 FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
    WHERE p.company_id=? AND p.user_id=? AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active'
      AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))
      AND iam_principal_active(p.user_id,p.company_id,?) LIMIT 1`).bind(a.companyId,recipientId,Date.now()).first())return [];
  const rows=(await db.prepare(`SELECT a.id,a.role,a.modules_json,a.company_id,a.branch_id,a.version,a.valid_until,a.scope_kind,a.scope_node_id,
      n.display_name scope_name,n.slug scope_slug,b.id scope_branch_id,b.display_name scope_branch_name,b.slug scope_branch_slug
    FROM assignments a JOIN organization_nodes c ON c.id=a.company_id AND c.kind='company' AND c.status='active'
    LEFT JOIN organization_nodes n ON n.id=a.scope_node_id AND n.company_id=a.company_id
    LEFT JOIN organization_nodes b ON b.id=CASE WHEN a.scope_kind='branch' THEN a.scope_node_id WHEN a.scope_kind='department' THEN COALESCE(a.branch_id,n.branch_id) ELSE NULL END AND b.kind='branch' AND b.company_id=a.company_id AND b.status='active'
    WHERE a.company_id=? AND a.user_id=? AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>? AND iam_assignment_active(a.id,?)
      AND iam_principal_active(a.user_id,a.company_id,?) AND (a.scope_kind='holding' OR (n.status='active' AND ((a.scope_kind='branch' AND n.kind='branch') OR (a.scope_kind='department' AND n.kind='department' AND (n.branch_id IS NULL OR n.branch_id=b.id)))))
    ORDER BY a.scope_kind,a.role,a.id`).bind(a.companyId,recipientId,Date.now(),Date.now(),Date.now(),Date.now()).all<PreviewAssignmentRow>()).results;
  const roles=await iamRoles(db,a.companyId,rows.map(row=>row.role));
  const currentRows=(await db.prepare("SELECT assignment_id FROM active_work_contexts WHERE company_id=? AND user_id=? AND expires_at>? AND assignment_id IS NOT NULL ORDER BY updated_at DESC").bind(a.companyId,recipientId,Date.now()).all<{assignment_id:string}>()).results;
  const currentIds=new Set(currentRows.map(row=>row.assignment_id));
  return rows.flatMap(row=>{
    const role=roles.get(row.role);if(!role||!role.scopes.includes(row.scope_kind))return [];
    const scope=row.scope_kind==="holding"?"Company-wide":row.scope_kind==="branch"?row.scope_name??"Branch scope":[row.scope_branch_name,row.scope_name].filter(Boolean).join(" · ")||"Department scope";
    return [{row,roleName:role.name,label:`${role.name} · ${scope}`,scope,current:currentIds.has(row.id),permissions:role.permissions}];
  });
}
export async function recipientPreviewContexts(db:SqlDatabase,a:WorkContext,id:string,recipientId:string) {
  const contexts=await recipientPreviewAssignments(db,a,id,recipientId);
  return contexts.map(context=>({id:context.row.id,label:context.label,scope:context.scope,role:context.roleName,current:context.current}));
}
async function recipientPreviewContext(db:SqlDatabase,a:WorkContext,id:string,recipientId:string,assignmentId:string):Promise<WorkContext> {
  const candidates=await recipientPreviewAssignments(db,a,id,recipientId);
  const candidate=candidates.find(item=>item.row.id===assignmentId);if(!candidate)throw new ApiError(422,"PREVIEW_CONTEXT_UNAVAILABLE","Choose one of the recipient's currently active work roles.");
  const row=candidate.row;
  const scope= row.scope_kind==="holding"
    ? {kind:"holding" as const,holdingId:a.holdingId,holdingSlug:a.holdingSlug,holdingName:a.holdingName,companyId:a.companyId}
    : row.scope_kind==="branch"
      ? {kind:"branch" as const,holdingId:a.holdingId,holdingSlug:a.holdingSlug,holdingName:a.holdingName,companyId:a.companyId,branchId:row.scope_node_id!,branchSlug:row.scope_slug!,branchName:row.scope_name!}
      : {kind:"department" as const,holdingId:a.holdingId,holdingSlug:a.holdingSlug,holdingName:a.holdingName,companyId:a.companyId,branchId:row.scope_branch_id,branchSlug:row.scope_branch_slug??undefined,branchName:row.scope_branch_name??undefined,departmentId:row.scope_node_id!,departmentSlug:row.scope_slug!,departmentName:row.scope_name!};
  const branchId=scope.kind==="holding"?null:scope.kind==="branch"?scope.branchId:scope.branchId;
  const departmentId=scope.kind==="department"?scope.departmentId:null;
  const assignment:WorkAssignment={id:row.id,role:row.role,companyId:row.company_id,branchId,modules:JSON.parse(row.modules_json) as string[],scopeKind:row.scope_kind,scopeNodeId:row.scope_node_id,version:Number(row.version),validUntil:Number(row.valid_until),label:candidate.label};
  const activeContext=await db.prepare("SELECT session_id,revision,assignment_version FROM active_work_contexts WHERE company_id=? AND user_id=? AND assignment_id=? AND expires_at>? ORDER BY updated_at DESC LIMIT 1").bind(a.companyId,recipientId,assignmentId,Date.now()).first<{session_id:string;revision:number;assignment_version:number}>();
  const loadedIam=await loadIamAuthority(db,a.companyId,recipientId,row.role,activeContext?.session_id);
  const iam={...loadedIam,queuedAssignmentVersion:Number(row.version)};
  const beta=await betaAccess(db,a.companyId,new Headers());
  const modules=beta?[]:row.role.startsWith("custom_")?[...new Set(iam.permissions.filter(permission=>permission.startsWith("module.")).map(permission=>permission.split(".")[1]))]:modulesForRole(row.role,assignment.modules);
  const recipient=await db.prepare("SELECT legal_name FROM hr_people WHERE company_id=? AND user_id=? AND status='active' AND merged_into_id IS NULL").bind(a.companyId,recipientId).first<{legal_name:string}>();
  if(!recipient)throw new ApiError(422,"RECIPIENT_INVALID","Choose an active employee in this company.");
  const context:WorkContext={...a,iam,userId:recipientId,name:recipient.legal_name,email:"",assignmentId:row.id,role:row.role,companyId:row.company_id,branchId,departmentId,scope,assignmentScope:row.scope_kind,inherited:false,beta,modules,assignments:[assignment],availableAssignments:[assignment],authority:activeContext?{sessionId:activeContext.session_id,revision:Number(activeContext.revision),assignmentVersion:Number(activeContext.assignment_version)}:undefined,workspaces:[]};
  await currentAuthority(db,context);
  return context;
}
export async function previewShare(db:SqlDatabase,a:WorkContext,id:string,input:z.infer<typeof sharePreviewSchema>) {
  const candidate=await validateShareCandidate(db,a,id,shareSchema.parse({recipientId:input.recipientId,mode:input.mode,role:"viewer",widgetIds:input.widgetIds,expiresAt:null,filters:input.filters}),false);
  const reader=input.mode==="distribution"?a:await recipientPreviewContext(db,a,id,input.recipientId,input.assignmentId??"");
  const metrics:Record<string,MetricData>={},comparisons:Record<string,MetricData>={};
  for(const widget of candidate.widgets) {
    const key=metricKey(widget);
    if(!metrics[key])try{metrics[key]=await queryMetric(db,reader,widget.metric,input.filters,widget.breakdown);}catch(error){const denied=error instanceof ApiError;metrics[key]=metricUnavailable(widget.metric,denied?error.message:"This metric could not be previewed.",denied?"unavailable":"failed",input.filters.currency);}
    if(widget.comparePeriod)try{if(!metricSupportsComparison(widget.metric))throw new ApiError(403,"COMPARISON_UNAVAILABLE","This metric does not support period comparison.");comparisons[widget.id]=await queryMetric(db,reader,widget.metric,previousPeriod(input.filters),widget.breakdown);}catch(error){const denied=error instanceof ApiError;comparisons[widget.id]=metricUnavailable(widget.metric,denied?error.message:"The comparison period could not be previewed.",denied?"unavailable":"failed",input.filters.currency);}
  }
  await currentAuthority(db,a);if(reader!==a)await currentAuthority(db,reader);
  if(reader===a&&!await activeShareRecipient(db,a,input.recipientId))throw new ApiError(403,"RECIPIENT_ACCESS_CHANGED","The recipient's active membership or work role changed. Refresh the share preview.");
  const fresh=await reportAccess(db,a,id);if(fresh.r.version!==candidate.x.r.version)throw new ApiError(409,"REPORT_CHANGED","The report changed while its share preview was loading.");
  return {mode:input.mode,recipientContext:reader===a?null:{assignmentId:reader.assignmentId,role:reader.role,scope:reader.scope.kind==="holding"?"Company-wide":reader.scope.kind==="branch"?reader.scope.branchName: [reader.scope.branchName,reader.scope.departmentName].filter(Boolean).join(" · ")},widgets:candidate.widgets.map(widget=>({id:widget.id,title:widget.title,metric:widget.metric,data:metrics[metricKey(widget)],comparison:comparisons[widget.id]})),updatedAt:Date.now()};
}
export async function ownershipCandidates(db:SqlDatabase,a:WorkContext,id:string) {
  const x=await reportAccess(db,a,id);if(!x.owner||!hasPermission(a,"executive.report.share"))throw missing();
  const now=Date.now(),rows=(await db.prepare(`SELECT a.id,a.role,a.modules_json,a.company_id,a.branch_id,a.version,a.valid_until,a.scope_kind,a.scope_node_id,
      p.user_id,n.display_name scope_name,n.slug scope_slug,b.id scope_branch_id,b.display_name scope_branch_name,b.slug scope_branch_slug,p.legal_name
    FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
    JOIN staff_levels l ON l.id=e.level_id AND l.company_id=e.company_id AND l.active=1 AND l.level_number IN (1,2)
    JOIN assignments a ON a.company_id=p.company_id AND a.user_id=p.user_id
    JOIN organization_nodes c ON c.id=a.company_id AND c.kind='company' AND c.status='active'
    LEFT JOIN organization_nodes n ON n.id=a.scope_node_id AND n.company_id=a.company_id
    LEFT JOIN organization_nodes b ON b.id=CASE WHEN a.scope_kind='branch' THEN a.scope_node_id WHEN a.scope_kind='department' THEN COALESCE(a.branch_id,n.branch_id) ELSE NULL END AND b.kind='branch' AND b.company_id=a.company_id AND b.status='active'
    WHERE p.company_id=? AND p.user_id IS NOT NULL AND p.user_id<>? AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active'
      AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))
      AND a.revoked_at IS NULL AND a.valid_from<=? AND a.valid_until>? AND iam_assignment_active(a.id,?) AND iam_principal_active(a.user_id,a.company_id,?)
      AND EXISTS(SELECT 1 FROM iam_roles r JOIN iam_role_revisions rv ON rv.company_id=r.company_id AND rv.role_id=r.id AND rv.revision=r.published_revision
        WHERE r.id=a.role AND r.company_id IN ('*',a.company_id) AND r.status='active'
          AND rv.permissions_json @> '["executive.access","executive.report.create","executive.report.edit","executive.report.share"]'::jsonb)
      AND (a.scope_kind='holding' OR (n.status='active' AND ((a.scope_kind='branch' AND n.kind='branch') OR (a.scope_kind='department' AND n.kind='department' AND (n.branch_id IS NULL OR n.branch_id=b.id)))))
    ORDER BY p.legal_name,a.scope_kind,a.role,a.id LIMIT 500`).bind(a.companyId,a.userId,now,now,now,now).all<PreviewAssignmentRow&{user_id:string;legal_name:string}>()).results;
  const roles=await iamRoles(db,a.companyId,rows.map(row=>row.role));
  const activeContexts=rows.length?(await db.prepare(`SELECT assignment_id FROM active_work_contexts WHERE company_id=? AND expires_at>? AND assignment_id IN (${rows.map(()=>"?").join(",")})`).bind(a.companyId,now,...rows.map(row=>row.id)).all<{assignment_id:string}>()).results:[];
  const currentIds=new Set(activeContexts.map(row=>row.assignment_id)),people=new Map<string,{id:string;name:string;contexts:{id:string;label:string;role:string;scope:string;current:boolean}[]}>();
  for(const row of rows){
    const role=roles.get(row.role);if(!role||!role.scopes.includes(row.scope_kind))continue;
    const scope=row.scope_kind==="holding"?"Company-wide":row.scope_kind==="branch"?row.scope_name??"Branch scope":[row.scope_branch_name,row.scope_name].filter(Boolean).join(" · ")||"Department scope";
    const person=people.get(row.user_id)??{id:row.user_id,name:row.legal_name,contexts:[]};
    person.contexts.push({id:row.id,label:`${role.name} · ${scope}`,role:role.name,scope,current:currentIds.has(row.id)});people.set(row.user_id,person);
  }
  const shares=(await db.prepare("SELECT id,version FROM executive_report_shares WHERE report_id=? AND revoked_at IS NULL ORDER BY id").bind(id).all<{id:string;version:number}>()).results;
  if(shares.length>500)throw new ApiError(422,"TRANSFER_SHARE_LIMIT","Revoke some shares before transferring this report.");
  return {people:[...people.values()],shares};
}
export async function transferReportOwnership(db:SqlDatabase,a:WorkContext,id:string,input:z.infer<typeof transferOwnershipSchema>,key:string) {
  await requireAuthor(db,a,"executive.report.share");
  const replay=await replayCommand<{id:string;ownerId:string;version:number;revokedShares:number}>(db,a,key,"executive.report.transfer",id,input);if(replay)return replay;
  const x=await reportAccess(db,a,id);if(!x.owner)throw missing();
  if(input.expectedVersion!==x.r.version)throw new ApiError(409,"REPORT_CHANGED","The report changed. Reload it before transferring ownership.");
  const candidates=await ownershipCandidates(db,a,id),person=candidates.people.find(candidate=>candidate.id===input.ownerId),context=person?.contexts.find(candidate=>candidate.id===input.assignmentId);
  if(!person||!context)throw new ApiError(422,"TRANSFER_TARGET_INVALID","Choose an active Executive with an eligible work role.");
  const target=await recipientPreviewContext(db,a,id,input.ownerId,input.assignmentId);
  await requireAuthor(db,target,"executive.report.share");
  const expectedShares=[...input.expectedShares].sort((left,right)=>left.id.localeCompare(right.id)),currentShares=[...candidates.shares].sort((left,right)=>left.id.localeCompare(right.id));
  if(JSON.stringify(expectedShares)!==JSON.stringify(currentShares))throw new ApiError(409,"TRANSFER_SHARES_CHANGED","Report shares changed. Review the latest transfer preview.");
  await currentAuthority(db,target);
  const version=x.r.version+1,now=Date.now(),targetAssignment=target.assignments.find(assignment=>assignment.id===target.assignmentId)!;
  const targetGuard=`EXISTS(SELECT 1 FROM assignments g JOIN organization_nodes c ON c.id=g.company_id AND c.kind='company' AND c.status='active'
    JOIN hr_people p ON p.company_id=g.company_id AND p.user_id=g.user_id
    JOIN hr_engagements e ON e.company_id=p.company_id AND e.person_id=p.id JOIN staff_levels l ON l.company_id=e.company_id AND l.id=e.level_id
    WHERE g.id=? AND g.company_id=? AND g.user_id=? AND g.role=? AND g.version=? AND g.scope_kind=? AND g.scope_node_id IS NOT DISTINCT FROM ?
      AND g.revoked_at IS NULL AND g.valid_from<=? AND g.valid_until>? AND iam_assignment_active(g.id,?) AND iam_principal_active(g.user_id,g.company_id,?)
      AND (g.scope_kind='holding' OR EXISTS(SELECT 1 FROM organization_nodes n WHERE n.id=g.scope_node_id AND n.company_id=g.company_id AND n.status='active'
        AND (n.branch_id IS NULL OR EXISTS(SELECT 1 FROM organization_nodes b WHERE b.id=n.branch_id AND b.company_id=g.company_id AND b.status='active'))))
      AND l.active=1 AND l.level_number IN (1,2) AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')
      AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))
      AND COALESCE((SELECT m.version FROM iam_members m WHERE m.company_id=g.company_id AND m.user_id=g.user_id),0)=?
      AND EXISTS(SELECT 1 FROM iam_roles r JOIN iam_role_revisions rv ON rv.company_id=r.company_id AND rv.role_id=r.id AND rv.revision=r.published_revision
        WHERE r.id=g.role AND r.company_id IN ('*',g.company_id) AND r.status='active' AND r.published_revision=? AND rv.scopes_json @> jsonb_build_array(?::text)
          AND rv.permissions_json @> '["executive.access","executive.report.create","executive.report.edit","executive.report.share"]'::jsonb))`;
  return executeCommand(db,a,key,"executive.report.transfer",input,id,version,{id,ownerId:input.ownerId,version,revokedShares:currentShares.length},[
    db.prepare("SELECT id FROM executive_reports WHERE id=? FOR UPDATE").bind(id),
    assertion(db,key,"transfer-report-version","EXISTS(SELECT 1 FROM executive_reports WHERE id=? AND owner_id=? AND version=?)",[id,a.userId,input.expectedVersion]),
    assertion(db,key,"transfer-target",targetGuard,[targetAssignment.id,target.companyId,target.userId,target.role,targetAssignment.version??0,target.assignmentScope,targetAssignment.scopeNodeId,now,now,now,now,target.iam?.identityVersion??0,target.iam?.roleRevision??0,target.assignmentScope]),
    assertion(db,key,"transfer-share-set","COALESCE((SELECT jsonb_agg(jsonb_build_object('id',id,'version',version) ORDER BY id) FROM executive_report_shares WHERE report_id=? AND revoked_at IS NULL),'[]'::jsonb)=?::jsonb",[id,JSON.stringify(currentShares)]),
    db.prepare("UPDATE executive_reports SET owner_id=?,version=?,updated_at=? WHERE id=?").bind(input.ownerId,version,now,id),
    db.prepare("INSERT INTO executive_report_revisions(report_id,version,definition_json,filters_json,author_id,created_at) VALUES(?,?,?,?,?,?)").bind(id,version,x.r.definition_json,x.r.filters_json,a.userId,now),
    db.prepare("UPDATE executive_report_shares SET revoked_at=?,version=version+1 WHERE report_id=? AND revoked_at IS NULL").bind(now,id),
    db.prepare("UPDATE executive_generation_jobs SET state='cancelled',error='Report ownership changed. Start a new generation with the current owner.',updated_at=? WHERE report_id=? AND state IN ('queued','running')").bind(now,id),
  ]);
}
export async function revokeShare(db:SqlDatabase,a:WorkContext,id:string,shareId:string,key:string) {
  await requireAuthor(db,a,"executive.report.share");const x=await reportAccess(db,a,id);if(!x.owner)throw missing();
  return executeCommand(db,a,key,"executive.report.revoke",{shareId},id,x.r.version,{id:shareId},[
    db.prepare("SELECT id FROM executive_reports WHERE id=? FOR UPDATE").bind(id),
    assertion(db,key,"revoke-report-version","EXISTS(SELECT 1 FROM executive_reports WHERE id=? AND owner_id=? AND version=?)",[id,a.userId,x.r.version]),
    db.prepare("UPDATE executive_report_shares SET revoked_at=?,version=version+1 WHERE id=? AND report_id=? AND revoked_at IS NULL").bind(Date.now(),shareId,id),
  ]);
}
export async function reportData(db:SqlDatabase,a:WorkContext,id:string,filters?:Filters) {
  const x=await reportAccess(db,a,id);
  let f=x.share?x.filters:filters??x.filters;
  if(x.share&&filters) {
    if(filters.currency===x.filters.currency&&filters.from>=x.filters.from&&filters.to<=x.filters.to)f=filters;
  }
  let reader=a;
  if(x.share?.mode==="distribution") {
    reader=JSON.parse(x.share.publisher_json) as WorkContext;
    await requireAuthor(db,reader,"executive.data.distribute");
  }
  const metrics=[...new Map(x.definition.widgets.map(w=>[metricKey(w),w])).values()];
  const results:Record<string,MetricData>={};
  for(const widget of metrics)try{results[metricKey(widget)]=await queryMetric(db,reader,widget.metric,f,widget.breakdown);}catch(e){
    const denied=e instanceof ApiError;
    results[metricKey(widget)]=metricUnavailable(widget.metric,denied?e.message:"This metric could not be refreshed. Try again.",denied?"unavailable":"failed",f.currency);
  }
  const comparisons:Record<string,MetricData>={};
  for(const widget of x.definition.widgets.filter(widget=>widget.comparePeriod))try{
    if(!metricSupportsComparison(widget.metric))throw new ApiError(403,"COMPARISON_UNAVAILABLE","This metric does not support period comparison.");
    comparisons[widget.id]=await queryMetric(db,reader,widget.metric,previousPeriod(f),widget.breakdown);
  }catch(e){
    const denied=e instanceof ApiError;
    comparisons[widget.id]=metricUnavailable(widget.metric,denied?e.message:"The comparison period could not be refreshed. Try again.",denied?"unavailable":"failed",f.currency);
  }
  // Recheck after reads, including publisher and share version, before returning any payload.
  await currentAuthority(db,a);if(reader!==a)await requireAuthor(db,reader,"executive.data.distribute");
  const fresh=await reportAccess(db,a,id);
  if(fresh.share?.version!==x.share?.version||fresh.r.version!==x.r.version)throw new ApiError(409,"REPORT_CHANGED","Report access or design changed. Refresh to continue.");
  const documents=Object.fromEntries(x.definition.widgets.filter(w=>w.kind==="html").map(w=>[w.id,htmlWidgetDocument(w,results[metricKey(w)],x.definition.accent)]));
  return {metrics:results,comparisons,documents,updatedAt:Date.now(),version:x.r.version,shareVersion:x.share?.version??0,mode:x.share?.mode??"owner"};
}
