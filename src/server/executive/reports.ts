import "server-only";
import { z } from "zod";
import { hasPermission } from "@/lib/permissions";
import { metricKey, definitionSchema, filtersSchema, emptyDefinition, defaultFilters, shareSchema, type Filters, type ReportView, type ReportSummary, type ShareView, type MetricData } from "@/lib/executive/canvas";
import type { SqlDatabase } from "../runtime/types";
import type { WorkContext } from "../authorization";
import { ApiError } from "../http";
import { assertion, executeCommand, replayCommand } from "../commands";
import { currentAuthority, canAuthor, requireAuthor, executiveEligible } from "./access";
import { metricBreakdowns, metricCatalog, queryMetric } from "./metrics";
import { htmlWidgetDocument } from "./html-widget";

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
  return {id,owner:x.owner,canEdit:x.canEdit,canShare:x.owner&&hasPermission(a,"executive.report.share"),version:x.r.version,definition:x.definition,filters:x.filters,lockedFilters:!!x.share,updatedAt:x.r.updated_at,shares,revisions};
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
  return {canCreate:author,reports,catalog:metricCatalog(a),eligible:await executiveEligible(db,a),accessSetup};
}
export async function createReport(db:SqlDatabase,a:WorkContext,key:string,title="Untitled report") {
  await requireAuthor(db,a);const definition=emptyDefinition(title),filters=defaultFilters(),id=`report-${key}`,now=Date.now();
  return executeCommand(db,a,key,"executive.report.create",{title},id,1,{id},[
    db.prepare("INSERT INTO executive_reports(id,company_id,owner_id,definition_json,filters_json,created_at,updated_at) VALUES(?,?,?,?,?,?,?)").bind(id,a.companyId,a.userId,JSON.stringify(definition),JSON.stringify(filters),now,now),
    db.prepare("INSERT INTO executive_report_revisions(report_id,version,definition_json,filters_json,author_id,created_at) VALUES(?,1,?,?,?,?)").bind(id,JSON.stringify(definition),JSON.stringify(filters),a.userId,now),
  ]);
}
export const saveSchema=z.object({expectedVersion:z.number().int().positive(),definition:definitionSchema,filters:filtersSchema}).strict();
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
  const allowed=metricCatalog(a).filter(m=>m.available).map(m=>m.id);
  if(input.definition.widgets.some(w=>!allowed.includes(w.metric)||!metricBreakdowns(w.metric).includes(w.breakdown??"default")))throw new ApiError(403,"METRIC_FORBIDDEN","A widget uses a metric unavailable to your selected role.");
  const version=input.expectedVersion+1,now=Date.now();
  return executeCommand(db,a,key,"executive.report.save",input,id,version,{id,version},[
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
export async function saveShare(db:SqlDatabase,a:WorkContext,id:string,input:z.infer<typeof shareSchema>,key:string) {
  await requireAuthor(db,a,"executive.report.share");const x=await reportAccess(db,a,id);if(!x.owner)throw missing();
  const person=await db.prepare("SELECT legal_name FROM hr_people WHERE company_id=? AND user_id=? AND status='active' AND merged_into_id IS NULL").bind(a.companyId,input.recipientId).first<{legal_name:string}>();
  const target={...a,userId:input.recipientId,role:"employee",iam:undefined};
  if(!person||input.recipientId===a.userId)throw new ApiError(422,"RECIPIENT_INVALID","Choose an active employee in this company.");
  if(input.role==="editor"&&(!await executiveEligible(db,target)||input.mode==="distribution"))throw new ApiError(422,"EDITOR_INELIGIBLE","Editors must be eligible executives using their own source access.");
  if(input.expiresAt!==null&&input.expiresAt<=Date.now())throw new ApiError(422,"SHARE_EXPIRED","Choose a future expiry.");
  const widgets=x.definition.widgets.filter(w=>input.widgetIds.includes(w.id));
  if(widgets.length!==new Set(input.widgetIds).size)throw new ApiError(422,"WIDGET_INVALID","Choose widgets from the saved report.");
  if(input.role==="editor"&&widgets.length!==x.definition.widgets.length)throw new ApiError(422,"EDITOR_SCOPE","Editors need the complete report. Use viewer access for selected widgets.");
  if(input.mode==="distribution") {
    if(!hasPermission(a,"executive.data.distribute"))throw new ApiError(403,"DISTRIBUTION_FORBIDDEN","Your role cannot distribute report data.");
    if(widgets.some(w=>["my-tasks","projects","routines"].includes(w.metric)))throw new ApiError(422,"PERSONAL_METRIC","Personal work metrics can only be shared using the recipient's own access.");
    for(const widget of widgets)await queryMetric(db,a,widget.metric,input.filters,widget.breakdown);
  }
  const definition={...x.definition,widgets},shareId=`share-${id}-${input.recipientId}`;
  // Persist authority without browser/session credentials. Any authority revision change requires republishing.
  const publisher={...a,authority:undefined};
  return executeCommand(db,a,key,"executive.report.share",input,id,x.r.version,{id:shareId},[
    assertion(db,key,"share-report-version","EXISTS(SELECT 1 FROM executive_reports WHERE id=? AND owner_id=? AND version=?)",[id,a.userId,x.r.version]),
    assertion(db,key,"share-recipient","iam_principal_active(?,?,?) AND EXISTS(SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>? AND iam_assignment_active(id,?))",[input.recipientId,a.companyId,Date.now(),a.companyId,input.recipientId,Date.now(),Date.now(),Date.now()]),
    db.prepare(`INSERT INTO executive_report_shares(id,report_id,company_id,recipient_id,recipient_name,role,mode,widget_ids_json,definition_json,filters_json,publisher_json,expires_at,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?) ON CONFLICT(report_id,recipient_id) DO UPDATE SET recipient_name=excluded.recipient_name,role=excluded.role,mode=excluded.mode,widget_ids_json=excluded.widget_ids_json,definition_json=excluded.definition_json,filters_json=excluded.filters_json,publisher_json=excluded.publisher_json,expires_at=excluded.expires_at,revoked_at=NULL,version=executive_report_shares.version+1`).bind(shareId,id,a.companyId,input.recipientId,person.legal_name,input.role,input.mode,JSON.stringify(widgets.map(w=>w.id)),JSON.stringify(definition),JSON.stringify(input.filters),JSON.stringify(publisher),input.expiresAt,Date.now()),
  ]);
}
export async function revokeShare(db:SqlDatabase,a:WorkContext,id:string,shareId:string,key:string) {
  await requireAuthor(db,a,"executive.report.share");const x=await reportAccess(db,a,id);if(!x.owner)throw missing();
  return executeCommand(db,a,key,"executive.report.revoke",{shareId},id,x.r.version,{id:shareId},[db.prepare("UPDATE executive_report_shares SET revoked_at=?,version=version+1 WHERE id=? AND report_id=?").bind(Date.now(),shareId,id)]);
}
export async function reportData(db:SqlDatabase,a:WorkContext,id:string,filters?:Filters) {
  const x=await reportAccess(db,a,id);const f=x.share?x.filters:filters??x.filters;
  let reader=a;
  if(x.share?.mode==="distribution") {
    reader=JSON.parse(x.share.publisher_json) as WorkContext;
    await requireAuthor(db,reader,"executive.data.distribute");
  }
  const metrics=[...new Map(x.definition.widgets.map(w=>[metricKey(w),w])).values()];
  const results:Record<string,MetricData>={};
  for(const widget of metrics)try{results[metricKey(widget)]=await queryMetric(db,reader,widget.metric,f,widget.breakdown);}catch(e){if(!(e instanceof ApiError))throw e;results[metricKey(widget)]={metric:widget.metric,total:0,rows:[],unit:"count",currency:null,definition:"",scope:"Unavailable",updatedAt:Date.now(),revision:"unavailable",error:e.message};}
  // Recheck after reads, including publisher and share version, before returning any payload.
  await currentAuthority(db,a);if(reader!==a)await requireAuthor(db,reader,"executive.data.distribute");
  const fresh=await reportAccess(db,a,id);
  if(fresh.share?.version!==x.share?.version||fresh.r.version!==x.r.version)throw new ApiError(409,"REPORT_CHANGED","Report access or design changed. Refresh to continue.");
  const documents=Object.fromEntries(x.definition.widgets.filter(w=>w.kind==="html").map(w=>[w.id,htmlWidgetDocument(w,results[metricKey(w)],x.definition.accent)]));
  return {metrics:results,documents,updatedAt:Date.now(),version:x.r.version,shareVersion:x.share?.version??0,mode:x.share?.mode??"owner"};
}
