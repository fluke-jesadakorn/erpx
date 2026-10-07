import type {SqlDatabase} from "../src/server/runtime/types";
import type {WorkContext} from "../src/server/authorization";
import type {MetricId,Filters,Breakdown,MetricData} from "../src/lib/executive/canvas";
import type {MetricSourceGroup,MetricSourcePage} from "../src/server/executive/metric-sources";
import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { env, applyMigrations } from "./runtime-env";
import { expenseDb as db, expenseFixture, key } from "./helpers/expense-fixture";
import { createReport, reportView, reportLibrary, reportData, saveReport, saveShare, revokeShare, saveSchema, recipientPreviewContexts, previewShare, ownershipCandidates, transferReportOwnership } from "../src/server/executive/reports";
import { queueGeneration, drainExecutiveGeneration, generationHistory, cancelGeneration } from "../src/server/executive/generation";
import { canAuthor, currentAuthority } from "../src/server/executive/access";
import { metricCatalog, metricCatalogFor, queryMetric as ownerQueryMetric } from "../src/server/executive/metrics";
import { defaultFilters, definitionSchema, emptyDefinition, filtersForMonth, generateSchema, metricIds, monthBounds, sharePreviewSchema, shareSchema, transferOwnershipSchema } from "../src/lib/executive/canvas";
import { executiveCoverage } from "../src/lib/executive/coverage";
import { readExecutiveEvidence, readFinanceEvidence, searchExecutiveEvidence, searchFinanceEvidence } from "../src/server/executive/evidence";
import { saveMaster } from "../src/server/commerce";
import { formatMetric } from "../src/lib/executive/numbers";
import {defaultConfig} from "../src/lib/pms-model";
import { dailyRoomReport } from "../src/lib/pms-read";
import { listInventoryReorderPolicies, saveInventoryReorderPolicy, transitionInventoryReorderPolicy } from "../src/server/inventory-reorder-policies";
vi.mock("../src/server/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
// Exercise every populated metric fixture through its source query as well as its aggregate.
function queryMetric(db:SqlDatabase,actor:WorkContext,id:MetricId,filters:Filters,breakdown?:Breakdown):Promise<MetricData>;
function queryMetric(db:SqlDatabase,actor:WorkContext,id:MetricId,filters:Filters,breakdown:Breakdown,source:MetricSourceGroup):Promise<MetricSourcePage>;
async function queryMetric(db:SqlDatabase,actor:WorkContext,id:MetricId,filters:Filters,breakdown:Breakdown='default',source?:MetricSourceGroup):Promise<MetricData|MetricSourcePage>{
 if(source)return ownerQueryMetric(db,actor,id,filters,breakdown,source);
 const result=await ownerQueryMetric(db,actor,id,filters,breakdown),first=result.rows[0];
 if(first){const sources=await ownerQueryMetric(db,actor,id,filters,breakdown,{label:first.label,unit:first.unit,page:1});
  expect(sources.row.name).toBe(first.label);
  expect(sources.rows.every(row=>typeof row.id==='string'&&typeof row.facts==='object')).toBe(true);
  if(first.value!=='0')expect(sources.rows.length).toBeGreaterThan(0);
 }
 return result;
}
const aiEnv={AI_BASE_URL:"https://opencode.ai/zen/go/v1",AI_MODEL:"muse-spark-1.3-contributor",AI_API_KEY:"synthetic-key"};
let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await Promise.all([applyMigrations(db,env.STAFF_TEST_MIGRATIONS),applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS)]);});
beforeEach(async()=>{f=await expenseFixture();});
async function report(metric:"headcount"|"my-tasks"="headcount") {
  const r=await createReport(db,f.owner,key(),"Operating overview");
  const d=definitionSchema.parse({...emptyDefinition("Operating overview"),widgets:[{id:"people",title:"Active people",metric,kind:"kpi",width:"half"}]});
  await saveReport(db,f.owner,r.id,saveSchema.parse({expectedVersion:1,definition:d,filters:defaultFilters()}),key());return r.id;
}
it("rejects scripts, arbitrary URLs, unknown metrics and untranslated product labels",()=>{
  expect(()=>definitionSchema.parse({...emptyDefinition(),script:"fetch('/secret')"})).toThrow();
  expect(()=>definitionSchema.parse({...emptyDefinition(),title:"รายงาน"})).toThrow();
  expect(()=>definitionSchema.parse({...emptyDefinition(),widgets:[{id:"x",title:"Secrets",metric:"payroll-secrets"}]})).toThrow();
});
it("narrows monthly cross-filters to valid dates inside the current report interval",()=>{
  expect(monthBounds("2024-02")).toEqual({from:"2024-02-01",to:"2024-02-29"});
  expect(monthBounds("2025-02")).toEqual({from:"2025-02-01",to:"2025-02-28"});
  expect(monthBounds("2026-13")).toBeNull();
  expect(monthBounds("2026-2")).toBeNull();
  const filters={from:"2026-01-15",to:"2026-04-10",currency:"USD" as const};
  expect(filtersForMonth(filters,"2026-01")).toEqual({...filters,to:"2026-01-31"});
  expect(filtersForMonth(filters,"2026-04")).toEqual({...filters,from:"2026-04-01"});
  expect(filtersForMonth(filters,"2025-12")).toBeNull();
});
it("lists all 32 modules and keeps engineering coverage separate from acceptance and availability",()=>{
  const modules=executiveCoverage();
  expect(modules).toHaveLength(32);
  expect(modules.reduce((count,module)=>count+module.requiredMetrics,0)).toBe(96);
  expect(modules.reduce((count,module)=>count+module.implementedMetrics,0)).toBe(96);
  expect(modules.filter(module=>module.implemented)).toHaveLength(32);
  expect(modules.every(module=>!module.verified&&!module.productionAccepted&&!module.businessAccepted&&!module.available)).toBe(true);
  expect(modules.flatMap(module=>module.metrics).filter(metric=>metric.implemented).every(metric=>metric.implementation==="platform-executive"||!!metric.implementedMetric&&metricIds.includes(metric.implementedMetric))).toBe(true);
  expect(modules.find(module=>module.moduleId==="operations")).toMatchObject({requiredMetrics:3,implementedMetrics:3,implemented:true,verified:false,available:false});
  expect(modules.find(module=>module.moduleId==="people_payroll")).toMatchObject({requiredMetrics:4,implementedMetrics:4,status:"implemented",verified:false,available:false});
  expect(modules.find(module=>module.moduleId==="pms")).toMatchObject({requiredMetrics:6,implementedMetrics:6,status:"implemented",verified:false,available:false});
  expect(modules.find(module=>module.moduleId==="inventory")).toMatchObject({requiredMetrics:4,implementedMetrics:4,implemented:true,verified:false,available:false});
  expect(modules.find(module=>module.moduleId==="projects")).toMatchObject({requiredMetrics:5,implementedMetrics:5,implemented:true,verified:false,available:false});
});
it("reconciles Operations overdue tasks, overdue resolution time and current-version escalations with row-scoped evidence",async()=>{
  const operationsActor={...f.owner,modules:[...f.owner.modules,"operations"]},now=Date.now(),filters={from:"2026-09-01",to:"2026-09-30",currency:"THB"},
    overdueId=key(),staleAlertId=key(),futureId=key(),resolvedOneId=key(),resolvedTwoId=key(),privateId=key(),escalatedId=key();
  const task=async(id:string,title:string,state:string,dueAt:number,updatedAt:number,version=1,creator=f.owner.userId,assignee=f.claimant.userId)=>db.prepare(`INSERT INTO staff_tasks
    (id,company_id,branch_id,department_id,title,description,creator_user_id,assignee_user_id,due_at,state,version,created_at,updated_at)
    VALUES(?,?,?,?,?,'English-only private task description',?,?,?,?,?,?,?)`)
    .bind(id,f.company,f.branch,f.department,title,creator,assignee,dueAt,state,version,dueAt-7*86400000,updatedAt).run();
  await task(overdueId,"Executive Operations overdue fixture task","in_progress",now-3*86400000,now-1000);
  await task(staleAlertId,"Executive Operations stale alert fixture task","ready",now-2*86400000,now,2);
  await task(futureId,"Executive Operations future fixture task","ready",now+86400000,now);
  const dueOne=Date.UTC(2026,8,3),dueTwo=Date.UTC(2026,8,6);
  await task(resolvedOneId,"Executive Operations resolved fixture one","completed",dueOne,dueOne+86400000);
  await task(resolvedTwoId,"Executive Operations resolved fixture two","completed",dueTwo,dueTwo+2*86400000);
  await task(privateId,"Executive Operations private fixture task","in_progress",now-86400000,now,1,f.claimant.userId,f.claimant.userId);
  await task(escalatedId,"Executive Operations escalation fixture task","ready",now-5*86400000,now);
  await db.batch([
    db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
      VALUES(?,?,?,'staff_task_alert',?,1,'English-only stale fixture alert',?)`).bind(`staff-task-alert-overdue-${staleAlertId}-1-${f.owner.userId}`,f.company,f.owner.userId,staleAlertId,now-500),
    db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
      VALUES(?,?,?,'staff_task_alert',?,1,'English-only fixture escalation',?)`).bind(`staff-task-alert-overdue-${escalatedId}-1-${f.owner.userId}`,f.company,f.owner.userId,escalatedId,now-300),
    db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
      VALUES(?,?,?,'staff_task_alert',?,1,'English-only other-recipient alert',?)`).bind(`staff-task-alert-overdue-${overdueId}-1-${f.claimant.userId}`,f.company,f.claimant.userId,overdueId,now-200),
  ]);

  const open=await queryMetric(db,operationsActor,"open-operations-exceptions",filters);
  expect(open).toMatchObject({total:"3",unit:"count",status:"ready",scopeBoundary:{level:"personal"}});
  expect((await queryMetric(db,operationsActor,"open-operations-exceptions",filters,"status")).rows).toEqual([{label:"in_progress",value:"1"},{label:"ready",value:"2"}]);
  expect((await queryMetric(db,operationsActor,"open-operations-exceptions",filters,"age-band")).rows.reduce((sum,row)=>sum+Number(row.value),0)).toBe(3);
  const resolution=await queryMetric(db,operationsActor,"exception-resolution-time",filters);
  expect(resolution).toMatchObject({total:"129600000",unit:"duration",status:"ready"});
  expect((await queryMetric(db,operationsActor,"exception-resolution-time",filters,"month")).rows).toEqual([{label:"2026-09",value:"129600000"}]);
  expect((await queryMetric(db,operationsActor,"operations-escalations",filters)).total).toBe("1");
  expect((await queryMetric(db,operationsActor,"operations-escalations",filters,"branch")).rows).toEqual([{label:"Branch",value:"1"}]);

  const first=await searchExecutiveEvidence(db,operationsActor,{metric:"open-operations-exceptions",filters,query:"Executive Operations",limit:1});
  const second=await searchExecutiveEvidence(db,operationsActor,{metric:"open-operations-exceptions",filters,query:"Executive Operations",limit:1,cursor:first.nextCursor!});
  expect(first.provider).toBe("operations-staff-tasks");expect(first.items).toHaveLength(1);expect(second.items).toHaveLength(1);expect(first.items[0].id).not.toBe(second.items[0].id);
  const detail=await readExecutiveEvidence(db,operationsActor,"open-operations-exceptions",first.items[0].id,filters);
  expect(requiredField(detail.record,'version')).toBeGreaterThan(0);expect(detail.record.href).toMatch(/^\/staff-tasks\/[A-Za-z0-9_-]+$/);
  expect(detail.fields).toEqual(["title","state","due_at","created_at","updated_at","version","branch_id","department_id"]);
  expect(detail.record).not.toHaveProperty("description");expect(detail.record).not.toHaveProperty("completion_note");
  const resolutionEvidence=await searchExecutiveEvidence(db,operationsActor,{metric:"exception-resolution-time",filters,query:"Executive Operations resolved fixture",limit:1});
  const resolutionDetail=await readExecutiveEvidence(db,operationsActor,"exception-resolution-time",resolutionEvidence.items[0].id,filters);
  expect(resolutionDetail.record).toHaveProperty("exception_duration_ms");
  const escalationEvidence=await searchExecutiveEvidence(db,operationsActor,{metric:"operations-escalations",filters,query:"Executive Operations escalation fixture",limit:10});
  expect(escalationEvidence.items).toHaveLength(1);
  const escalationDetail=await readExecutiveEvidence(db,operationsActor,"operations-escalations",escalationEvidence.items[0].id,filters);
  expect(escalationDetail.record).toHaveProperty("alert_created_at");
  await expect(queryMetric(db,{...operationsActor,modules:[]},"open-operations-exceptions",filters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,{...operationsActor,modules:[]},{metric:"operations-escalations",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});

  const reportId=(await createReport(db,operationsActor,key(),"Operations personal metric" )).id;
  const definition=definitionSchema.parse({...emptyDefinition("Operations personal metric"),widgets:[{id:"exceptions",title:"Open exceptions",metric:"open-operations-exceptions",kind:"kpi",width:"half"}]});
  await saveReport(db,operationsActor,reportId,saveSchema.parse({expectedVersion:1,definition,filters}),key());
  await expect(saveShare(db,f.owner,reportId,shareSchema.parse({recipientId:f.claimant.userId,mode:"distribution",widgetIds:["exceptions"],expiresAt:null,filters}),key())).rejects.toMatchObject({code:"PERSONAL_METRIC"});
  const audit=(await db.prepare(`SELECT provider_id,metric_id,action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events
    WHERE company_id=? AND actor_id=? AND provider_id='operations-staff-tasks' ORDER BY created_at,id`).bind(f.company,operationsActor.userId).all<{provider_id:string;metric_id:string;action:string;record_id:string;record_version:number;fields:string}>()).results;
  expect(audit.some(row=>row.metric_id==="open-operations-exceptions"&&row.action==="open"&&row.record_version>0)).toBe(true);
  expect(audit.every(row=>JSON.parse(row.fields).every((field:string)=>!field.includes("description")&&!field.includes("completion_note")&&!field.includes("message")))).toBe(true);
});
it("counts only overdue milestones in projects visible to the reader and audits milestone evidence",async()=>{
  const actor={...f.owner,branchId:f.branch,departmentId:null,modules:[...f.owner.modules,"projects"]},filters={from:"2026-01-01",to:"2026-12-31",currency:"THB"},now=Date.now();
  const visibleProject=key(),sharedProject=key(),hiddenProject=key(),oldId=key(),recentId=key(),completedId=key(),cancelledId=key(),futureId=key(),hiddenId=key();
  await db.batch([
    db.prepare("INSERT INTO staff_projects(id,company_id,branch_id,department_id,title,description,owner_user_id,created_at,updated_at) VALUES(?,?,?,NULL,'Executive Project owned','',?,?,?)")
      .bind(visibleProject,f.company,f.branch,f.owner.userId,now,now),
    db.prepare("INSERT INTO staff_projects(id,company_id,branch_id,department_id,title,description,owner_user_id,created_at,updated_at) VALUES(?,?,?,NULL,'Executive Project shared','',?,?,?)")
      .bind(sharedProject,f.company,f.branch,f.claimant.userId,now,now),
    db.prepare("INSERT INTO staff_projects(id,company_id,branch_id,department_id,title,description,owner_user_id,created_at,updated_at) VALUES(?,?,?,NULL,'Executive Project private','',?,?,?)")
      .bind(hiddenProject,f.company,f.branch,f.claimant.userId,now,now),
    db.prepare("INSERT INTO staff_project_members(project_id,user_id,added_by,created_at) VALUES(?,?,?,?)")
      .bind(sharedProject,f.owner.userId,f.claimant.userId,now),
    ...[
      [oldId,visibleProject,"Overdue milestone older than thirty days",now-40*86400000,f.owner.userId,"planned"],
      [recentId,sharedProject,"Overdue milestone within one week",now-5*86400000,f.claimant.userId,"in_progress"],
      [completedId,visibleProject,"Completed milestone excluded",now-20*86400000,f.owner.userId,"completed"],
      [cancelledId,visibleProject,"Cancelled milestone excluded",now-10*86400000,f.owner.userId,"cancelled"],
      [futureId,visibleProject,"Future milestone excluded",now+2*86400000,f.owner.userId,"planned"],
      [hiddenId,hiddenProject,"Private project milestone excluded",now-3*86400000,f.claimant.userId,"planned"],
    ].map(([id,projectId,title,dueAt,ownerId,state])=>db.prepare(`INSERT INTO staff_project_milestones
      (id,company_id,project_id,title,description,due_at,owner_user_id,state,version,created_by,completed_at,created_at,updated_at)
      VALUES(?,?,?,?,'English-only fixture detail',?,?,?,1,?,?,?,?)`)
      .bind(id,f.company,projectId,title,dueAt,ownerId,state,actor.userId,state==="completed"?now:null,now,now)),
  ]);
  const metric=await queryMetric(db,actor,"overdue-project-milestones",filters);
  expect(metric).toMatchObject({total:"2",unit:"count",status:"ready",scopeBoundary:{level:"personal"}});
  expect((await queryMetric(db,actor,"overdue-project-milestones",filters,"project")).rows)
    .toEqual([{label:"Executive Project owned",value:"1"},{label:"Executive Project shared",value:"1"}]);
  expect((await queryMetric(db,actor,"overdue-project-milestones",filters,"age-band")).rows)
    .toEqual([{label:"0-7 days",value:"1"},{label:"Over 30 days",value:"1"}]);
  const first=await searchExecutiveEvidence(db,actor,{metric:"overdue-project-milestones",filters,query:"Overdue milestone",limit:1});
  const second=await searchExecutiveEvidence(db,actor,{metric:"overdue-project-milestones",filters,query:"Overdue milestone",limit:1,cursor:first.nextCursor!});
  expect(first.provider).toBe("projects-overdue-milestones");
  expect(first.items).toHaveLength(1);expect(second.items).toHaveLength(1);expect(first.items[0].id).not.toBe(second.items[0].id);
  const detail=await readExecutiveEvidence(db,actor,"overdue-project-milestones",first.items[0].id,filters);
  expect(detail.record).toMatchObject({project_id:visibleProject,project_title:"Executive Project owned",version:1,href:`/projects/${visibleProject}`});
  expect(detail.record).not.toHaveProperty("description");
  await expect(searchExecutiveEvidence(db,{...actor,modules:[]},{metric:"overdue-project-milestones",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  const reportId=(await createReport(db,actor,key(),"Project milestone overview")).id;
  const definition=definitionSchema.parse({...emptyDefinition("Project milestone overview"),widgets:[{id:"milestones",title:"Overdue milestones",metric:"overdue-project-milestones",kind:"table",width:"full"}]});
  await saveReport(db,actor,reportId,saveSchema.parse({expectedVersion:1,definition,filters}),key());
  await expect(saveShare(db,actor,reportId,shareSchema.parse({recipientId:f.claimant.userId,mode:"distribution",widgetIds:["milestones"],expiresAt:null,filters}),key()))
    .rejects.toMatchObject({code:"PERSONAL_METRIC"});
});
it("keeps private reports undiscoverable even to another senior role",async()=>{
  const id=await report();expect((await reportLibrary(db,f.claimant)).reports).toHaveLength(0);
  await expect(reportView(db,f.claimant,id)).rejects.toMatchObject({code:"REPORT_NOT_FOUND"});
  await expect(reportData(db,f.claimant,id)).rejects.toMatchObject({code:"REPORT_NOT_FOUND"});
  expect((await reportView(db,f.owner,id)).owner).toBe(true);
});
it("requires executive eligibility and an explicit effective grant",async()=>{
  expect(await canAuthor(db,f.claimant)).toBe(false);
  await expect(createReport(db,f.claimant,key())).rejects.toMatchObject({code:"EXECUTIVE_AUTHOR_REQUIRED"});
  expect(await canAuthor(db,f.owner)).toBe(true);
});
it("rejects concurrent report edits and keeps immutable revisions",async()=>{
  const id=await report(),input=saveSchema.parse({expectedVersion:2,definition:emptyDefinition("Revised view"),filters:defaultFilters()});
  const command=key();await saveReport(db,f.owner,id,input,command);await saveReport(db,f.owner,id,input,command);
  await expect(saveReport(db,f.owner,id,input,key())).rejects.toMatchObject({status:409});
  expect((await reportView(db,f.owner,id)).revisions.map(r=>r.version)).toEqual([3,2,1]);
});
it("uses exact aggregates beyond list-page limits and excludes unrelated users",async()=>{
  const now=Date.now(),due=Date.parse(defaultFilters().from)+86400000;
  await db.prepare(`INSERT INTO staff_tasks(id,company_id,branch_id,department_id,title,creator_user_id,assignee_user_id,due_at,state,created_at,updated_at) SELECT ?||n,?,?,?,'Test task',?,CASE WHEN n=351 THEN ? ELSE ? END,?,'ready',?,? FROM generate_series(1,351) n`).bind(key(),f.company,f.branch,f.department,f.owner.userId,f.owner.userId,f.claimant.userId,due,now,now).run();
  const result=await queryMetric(db,f.claimant,"my-tasks",defaultFilters());expect(result.total).toBe("350");expect(result.rows).toEqual([{label:"ready",value:"350"}]);
});
it("counts active Support incidents by current scope and opens bounded, audited metadata evidence",async()=>{
  const now=Date.now(),incidentIds=[key(),key(),key()];
  const incidents=[
    {id:incidentIds[0],department:f.department,title:"Executive test incident installation outage",severity:"critical",status:"investigating",closed:false},
    {id:incidentIds[1],department:null,title:"Executive test incident warehouse alarm",severity:"high",status:"open",closed:false},
    {id:incidentIds[2],department:f.department,title:"Executive test incident resolved outage",severity:"normal",status:"closed",closed:true},
  ];
  await db.batch(incidents.map((incident,index)=>db.prepare(`INSERT INTO support_incidents
    (id,company_id,branch_id,department_id,title,impact,severity,owner_id,due_date,status,root_cause,mitigation,mitigated_by,version,created_by,created_at,updated_at)
    VALUES(?,?,?,?,?,'English-only test impact',?,?,? ,?,?,?, ?,1,?,?,?)`).bind(
      incident.id,f.company,f.branch,incident.department,incident.title,incident.severity,f.owner.userId,"2026-10-15",incident.status,
      incident.closed?"English-only test cause":null,incident.closed?"English-only test mitigation":null,incident.closed?f.owner.userId:null,
      f.owner.userId,now-index,now-index,
    )));
  const filters={from:"2026-01-01",to:"2026-01-31",currency:"THB"};
  const total=await queryMetric(db,f.owner,"active-support-incidents",filters);
  expect(total).toMatchObject({total:"2",unit:"count",status:"ready",rows:[{label:"Active support incidents",value:"2"}]});
  expect((await queryMetric(db,f.owner,"active-support-incidents",filters,"status")).rows).toEqual([{label:"investigating",value:"1"},{label:"open",value:"1"}]);
  expect((await queryMetric(db,f.owner,"active-support-incidents",filters,"severity")).rows).toEqual([{label:"critical",value:"1"},{label:"high",value:"1"}]);
  const departmentActor={...f.owner,branchId:f.branch,departmentId:f.department};
  expect((await queryMetric(db,departmentActor,"active-support-incidents",filters)).total).toBe("1");
  await expect(queryMetric(db,f.claimant,"active-support-incidents",filters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});

  const first=await searchExecutiveEvidence(db,f.owner,{metric:"active-support-incidents",filters,query:"Executive test incident",limit:1});
  const second=await searchExecutiveEvidence(db,f.owner,{metric:"active-support-incidents",filters,query:"Executive test incident",limit:1,cursor:first.nextCursor!});
  expect(first.provider).toBe("support-incidents");expect(first.items).toHaveLength(1);expect(second.items).toHaveLength(1);
  expect(first.items[0].id).not.toBe(second.items[0].id);expect(first.items[0].summary).not.toContain("impact");
  const detail=await readExecutiveEvidence(db,f.owner,"active-support-incidents",first.items[0].id,filters);
  expect(detail.record).toMatchObject({status:"investigating",href:`/support/incidents/${incidentIds[0]}`});
  expect(detail.fields).toEqual(["title","severity","status","due_date","created_at","updated_at","version"]);
  expect(detail.record).not.toHaveProperty("impact");expect(detail.record).not.toHaveProperty("root_cause");expect(detail.record).not.toHaveProperty("mitigation");
  await expect(searchExecutiveEvidence(db,f.claimant,{metric:"active-support-incidents",filters,query:"Executive test incident",limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  const audit=(await db.prepare(`SELECT provider_id,action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events
    WHERE company_id=? AND actor_id=? AND metric_id='active-support-incidents' ORDER BY created_at,id`).bind(f.company,f.owner.userId).all<{provider_id:string;action:string;record_id:string|null;record_version:number|null;fields:string}>()).results;
  expect(audit.map(row=>row.action).sort()).toEqual(["open","search","search"]);
  expect(audit.every(row=>row.provider_id==="support-incidents"&&row.record_id&&row.record_version===1)).toBe(true);
  expect(JSON.parse(audit[0].fields)).toEqual(["title","severity","status","due_date","created_at","updated_at","version"]);

  const id=(await createReport(db,f.owner,key(),"Support incident pulse")).id;
  const definition=definitionSchema.parse({...emptyDefinition("Support incident pulse"),widgets:[{id:"incidents",title:"Active incidents",metric:"active-support-incidents",kind:"kpi",width:"half"}]});
  await saveReport(db,f.owner,id,saveSchema.parse({expectedVersion:1,definition,filters:defaultFilters()}),key());
  await expect(saveShare(db,f.owner,id,shareSchema.parse({recipientId:f.claimant.userId,mode:"distribution",widgetIds:["incidents"],expiresAt:null,filters:defaultFilters()}),key()))
    .rejects.toMatchObject({code:"RECIPIENT_SOURCE_REQUIRED"});
  expect(required((await db.prepare("SELECT count(*) n FROM executive_report_shares WHERE report_id=?").bind(id).first<{n:number}>()))?.n).toBe(0);
});
it("reconciles current Support ticket and overdue SLA metrics with bounded metadata-only evidence",async()=>{
  const now=Date.now(),warehouse=key(),customer=key(),order=key(),policy=key();
  await db.prepare("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status) VALUES(?,?,?,?,?,'active')")
    .bind(warehouse,f.company,f.branch,`EX-${warehouse.slice(0,7)}`,"Executive support test warehouse").run();
  await db.prepare(`INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at)
    VALUES(?,?,'customer',?,?,'THB',?,?)`).bind(customer,f.company,`EX-${customer.slice(0,8)}`,"Executive support test customer",now,now).run();
  await db.prepare(`INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,state,subtotal_minor,tax_minor,created_by,approved_by,created_at,updated_at)
    VALUES(?,?,?,?, 'sales',?,?,?,?,? ,?,'confirmed',10000,700,?,?,?,?)`).bind(order,f.company,f.branch,f.department,`EX-SUPPORT-${order.slice(0,7)}`,customer,warehouse,"THB","2026-01-01","2026-01-15",f.owner.userId,f.owner.userId,now,now).run();
  await db.prepare(`INSERT INTO support_sla_policies(id,company_id,name,policy_json,status,version,created_by,created_at,updated_at)
    VALUES(?,?,?,'{}','draft',1,?,?,?)`).bind(policy,f.company,"Executive support test policy",f.owner.userId,now,now).run();
  const overdueId=key(),waitingId=key(),resolvedId=key(),cancelledId=key();
  const ticket=async(id:string,title:string,status:"in_progress"|"waiting_customer"|"resolved"|"cancelled",department:string|null,updatedAt:number,clock:{state:string;dueAt:number|null}|null=null)=>db.prepare(`INSERT INTO support_tickets(
      id,company_id,branch_id,department_id,order_id,customer_id,title,issue,owner_id,due_date,status,resolution,sla_policy_id,sla_policy_version,sla_clock_json,version,created_by,created_at,updated_at)
    VALUES(?,?,?,?,?,?,?,?,?,? ,?,?,?, ?,?::jsonb,1,?,?,?)`).bind(id,f.company,f.branch,department,order,customer,title,"English-only private issue narrative",f.owner.userId,"2026-01-15",status,status==="resolved"?"English-only private resolution":null,clock?policy:null,clock?1:null,clock?JSON.stringify(clock):null,f.owner.userId,now,updatedAt).run();
  await ticket(overdueId,"Executive test ticket SLA breach","in_progress",f.department,now,{state:"running",dueAt:now-60000});
  await ticket(waitingId,"Executive test ticket awaiting customer","waiting_customer",null,now-1000);
  await ticket(resolvedId,"Executive test ticket proposed resolution","resolved",f.department,now-2000);
  await ticket(cancelledId,"Executive test ticket cancelled","cancelled",f.department,now-3000);

  const filters={from:"2020-01-01",to:"2020-01-31",currency:"THB"};
  const open=await queryMetric(db,f.owner,"open-support-tickets",filters);
  const byStatus=await queryMetric(db,f.owner,"open-support-tickets",filters,"status");
  const overdue=await queryMetric(db,f.owner,"overdue-support-sla",filters);
  expect(open).toMatchObject({total:"3",unit:"count",status:"ready"});
  expect(byStatus.rows).toEqual([{label:"in_progress",value:"1"},{label:"resolved",value:"1"},{label:"waiting_customer",value:"1"}]);
  expect(overdue).toMatchObject({total:"1",unit:"count",status:"ready"});
  const departmentActor={...f.owner,branchId:f.branch,departmentId:f.department};
  expect((await queryMetric(db,departmentActor,"open-support-tickets",filters)).total).toBe("2");
  expect((await queryMetric(db,departmentActor,"overdue-support-sla",filters)).total).toBe("1");
  await expect(queryMetric(db,f.claimant,"open-support-tickets",filters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});

  const first=await searchExecutiveEvidence(db,f.owner,{metric:"open-support-tickets",filters,query:"Executive test ticket",limit:1});
  const second=await searchExecutiveEvidence(db,f.owner,{metric:"open-support-tickets",filters,query:"Executive test ticket",limit:1,cursor:first.nextCursor!});
  const third=await searchExecutiveEvidence(db,f.owner,{metric:"open-support-tickets",filters,query:"Executive test ticket",limit:1,cursor:second.nextCursor!});
  expect(first.provider).toBe("support-tickets");expect(first.items).toHaveLength(1);expect(second.items).toHaveLength(1);expect(third.items).toHaveLength(1);
  expect(first.items[0].href).toBe(`/support/${overdueId}`);expect(first.items[0].summary).toContain("in_progress");
  expect(JSON.stringify([first,second,third])).not.toContain("private issue narrative");
  const detail=await readExecutiveEvidence(db,f.owner,"open-support-tickets",first.items[0].id,filters);
  expect(detail.record).toMatchObject({title:"Executive test ticket SLA breach",status:"in_progress",due_date:"2026-01-15",href:`/support/${overdueId}`});
  expect(detail.fields).toEqual(["title","status","due_date","updated_at","version"]);
  expect(JSON.stringify(detail)).not.toContain("private issue narrative");expect(JSON.stringify(detail)).not.toContain("customer");

  const overdueEvidence=await searchExecutiveEvidence(db,f.owner,{metric:"overdue-support-sla",filters,query:"Executive test ticket",limit:10});
  expect(overdueEvidence.items.map(item=>item.href)).toEqual([`/support/${overdueId}`]);
  expect(overdueEvidence.items[0].summary).toContain("Overdue SLA");
  const overdueDetail=await readExecutiveEvidence(db,f.owner,"overdue-support-sla",overdueEvidence.items[0].id,filters);
  expect(overdueDetail.record).toMatchObject({sla_state:"running",sla_due_at:now-60000});
  expect(overdueDetail.fields).toEqual(["title","status","due_date","updated_at","version","sla_clock_json.state","sla_clock_json.dueAt"]);
  await expect(searchExecutiveEvidence(db,f.claimant,{metric:"open-support-tickets",filters,query:"Executive test ticket",limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  await expect(readExecutiveEvidence(db,departmentActor,"open-support-tickets",second.items[0].id,filters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});
  const audit=(await db.prepare(`SELECT provider_id,metric_id,action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events
    WHERE company_id=? AND actor_id=? AND provider_id='support-tickets' ORDER BY created_at,id`).bind(f.company,f.owner.userId).all<{provider_id:string;metric_id:string;action:string;record_id:string|null;record_version:number|null;fields:string}>()).results;
  expect(audit.map(row=>row.action).sort()).toEqual(["open","open","search","search","search","search"]);
  expect(audit.every(row=>row.provider_id==="support-tickets"&&row.record_id&&row.record_version===1)).toBe(true);
  expect(audit.filter(row=>row.metric_id==="open-support-tickets").every(row=>JSON.parse(row.fields).every((field:string)=>!field.includes("issue")&&!field.includes("customer")))).toBe(true);
  expect(JSON.parse(audit.find(row=>row.metric_id==="overdue-support-sla")!.fields)).toContain("sla_clock_json.dueAt");
});
it("counts pending customer duplicate reviews and exposes only authorized pair metadata",async()=>{
  const now=Date.now(),suffix=key(),pairs=[
    {left:`executive-review-left-${suffix}`,right:`executive-review-right-${suffix}`,leftName:"Executive duplicate review customer Alpha",rightName:"Executive duplicate review customer Beta",state:"pending",updatedAt:now},
    {left:`executive-review-left2-${suffix}`,right:`executive-review-right2-${suffix}`,leftName:"Executive duplicate review customer Gamma",rightName:"Executive duplicate review customer Delta",state:"pending",updatedAt:now-1000},
    {left:`executive-review-left3-${suffix}`,right:`executive-review-right3-${suffix}`,leftName:"Executive resolved review customer Epsilon",rightName:"Executive resolved review customer Zeta",state:"distinct",updatedAt:now-2000},
  ];
  const parties=pairs.flatMap((pair,index)=>[
    {id:pair.left,code:`EX-${suffix.slice(0,6)}-L${index}`,name:pair.leftName},
    {id:pair.right,code:`EX-${suffix.slice(0,6)}-R${index}`,name:pair.rightName},
  ]);
  await db.batch(parties.map(party=>db.prepare(`INSERT INTO finance_parties(id,company_id,kind,code,name,currency,tax_registration,status,version,created_at,updated_at)
    VALUES(?,?,'customer',?,?,'THB','ENGLISH-ONLY-PRIVATE-TAX-REFERENCE','active',1,?,?)`).bind(party.id,f.company,party.code,party.name,now,now)));
  const reviewIds=pairs.map(()=>key());
  await db.batch(pairs.map((pair,index)=>db.prepare(`INSERT INTO customer_duplicate_reviews(id,company_id,left_customer_id,right_customer_id,status,evidence,created_by,version,created_at,updated_at)
    VALUES(?,?,?,?,?,'English-only private reviewer evidence',?,1,?,?)`).bind(reviewIds[index],f.company,pair.left,pair.right,pair.state,f.owner.userId,now-3000+index, pair.updatedAt)));
  const filters={from:"2020-01-01",to:"2020-01-31",currency:"THB"};
  const total=await queryMetric(db,f.owner,"duplicate-reviews-pending",filters);
  expect(total).toMatchObject({total:"2",unit:"count",status:"ready",rows:[{label:"Pending reviews",value:"2"}]});
  await expect(searchExecutiveEvidence(db,{...f.owner,branchId:f.branch,departmentId:f.department},{metric:"duplicate-reviews-pending",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  await expect(queryMetric(db,f.claimant,"duplicate-reviews-pending",filters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});

  const first=await searchExecutiveEvidence(db,f.owner,{metric:"duplicate-reviews-pending",filters,query:"Executive duplicate review customer",limit:1});
  const second=await searchExecutiveEvidence(db,f.owner,{metric:"duplicate-reviews-pending",filters,query:"Executive duplicate review customer",limit:1,cursor:first.nextCursor!});
  expect(first.provider).toBe("customer-duplicate-reviews");expect(first.items).toHaveLength(1);expect(second.items).toHaveLength(1);
  expect(first.items[0].href).toBe(`/customer-duplicates/${reviewIds[0]}`);expect(first.items[0].label).toContain("Executive duplicate review customer Alpha");
  const detail=await readExecutiveEvidence(db,f.owner,"duplicate-reviews-pending",first.items[0].id,filters);
  expect(detail.record).toMatchObject({left_name:"Executive duplicate review customer Alpha",right_name:"Executive duplicate review customer Beta",status:"pending",version:1,href:`/customer-duplicates/${reviewIds[0]}`});
  expect(detail.fields).toEqual(["left_customer.name","left_customer.code","right_customer.name","right_customer.code","status","created_at","updated_at","version"]);
  expect(JSON.stringify([first,second,detail])).not.toContain("private reviewer evidence");
  expect(JSON.stringify(detail)).not.toContain("PRIVATE-TAX-REFERENCE");
  await expect(searchExecutiveEvidence(db,f.claimant,{metric:"duplicate-reviews-pending",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  const audit=(await db.prepare(`SELECT provider_id,action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events
    WHERE company_id=? AND actor_id=? AND provider_id='customer-duplicate-reviews' ORDER BY created_at,id`).bind(f.company,f.owner.userId).all<{provider_id:string;action:string;record_id:string|null;record_version:number|null;fields:string}>()).results;
  expect(audit.map(row=>row.action).sort()).toEqual(["open","search","search"]);
  expect(audit.every(row=>row.provider_id==="customer-duplicate-reviews"&&row.record_id&&row.record_version===1)).toBe(true);
  expect(audit.every(row=>JSON.parse(row.fields).every((field:string)=>!field.includes("evidence")&&!field.includes("tax_registration")))).toBe(true);
});
it("opens only the latest unresolved close blockers for the selected accounting periods",async()=>{
  const now=Date.now(),periods=[
    {id:key(),name:"Executive period April",start:"2026-04-01",end:"2026-04-30",state:"open"},
    {id:key(),name:"Executive period June",start:"2026-06-01",end:"2026-06-30",state:"closing"},
    {id:key(),name:"Executive period May closed",start:"2026-05-01",end:"2026-05-31",state:"closed"},
    {id:key(),name:"Executive period August",start:"2026-08-01",end:"2026-08-31",state:"open"},
  ];
  await db.batch(periods.map(period=>db.prepare(`INSERT INTO finance_periods(id,company_id,name,start_date,end_date,state,close_version,created_at)
    VALUES(?,?,?,?,?,?,1,?)`).bind(period.id,f.company,period.name,period.start,period.end,period.state,now)));
  const check=async(periodId:string,checkKey:string,sourceVersion:string,status:"block"|"pass",count:number,checkedAt:number,resolvedAt:number|null=null)=>db.prepare(`INSERT INTO finance_close_checks(
      id,company_id,period_id,check_key,source_version,status,count_value,amount_minor,detail_json,evidence_json,checked_at,checked_by,resolved_at,resolved_by)
    VALUES(?,?,?,?,?,?,?,999999,'{"note":"English-only private finance check detail"}','{"note":"English-only private source evidence"}',?,?,?,?)`).bind(
      key(),f.company,periodId,checkKey,sourceVersion,status,count,checkedAt,f.owner.userId,resolvedAt,resolvedAt?f.owner.userId:null,
    ).run();
  await check(periods[0].id,"draft-journals","source-april-old","block",4,now-3000);
  await check(periods[0].id,"draft-journals","source-april-current","pass",0,now-1000);
  await check(periods[0].id,"draft-invoices","source-april-invoices","block",3,now-2000);
  await check(periods[1].id,"unmatched-bank","source-june-bank","block",2,now);
  await check(periods[1].id,"open-manufacturing","source-june-resolved","block",1,now-500,now-250);
  await check(periods[2].id,"draft-journals","source-may-closed","block",1,now);
  await check(periods[3].id,"draft-journals","source-august-outside","block",1,now);

  const filters={from:"2026-04-01",to:"2026-06-30",currency:"THB"};
  const total=await queryMetric(db,f.owner,"close-blockers",filters);
  expect(total).toMatchObject({total:"2",unit:"count",status:"ready",rows:[{label:"Open close blockers",value:"2"}]});
  expect((await queryMetric(db,f.owner,"close-blockers",filters,"status")).rows).toEqual([{label:"draft-invoices",value:"1"},{label:"unmatched-bank",value:"1"}]);
  await expect(searchExecutiveEvidence(db,{...f.owner,branchId:f.branch},{metric:"close-blockers",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});

  const first=await searchExecutiveEvidence(db,f.owner,{metric:"close-blockers",filters,query:"Executive period",limit:1});
  const second=await searchExecutiveEvidence(db,f.owner,{metric:"close-blockers",filters,query:"Executive period",limit:1,cursor:first.nextCursor!});
  expect(first.provider).toBe("finance-close-checks");expect(first.items).toHaveLength(1);expect(second.items).toHaveLength(1);
  expect(first.items[0].label).toContain("Executive period June");expect(first.items[0].summary).toContain("2 current items");
  const detail=await readExecutiveEvidence(db,f.owner,"close-blockers",first.items[0].id,filters);
  expect(detail.record).toMatchObject({period_name:"Executive period June",period_end_date:"2026-06-30",period_state:"closing",check_key:"unmatched-bank",source_version:"source-june-bank",status:"block",count_value:"2"});
  expect(detail.fields).toEqual(["check_key","count_value","status","checked_at","source_version","period.name","period.end_date","period.state"]);
  expect(JSON.stringify([first,second,detail])).not.toContain("private finance check detail");
  expect(JSON.stringify(detail)).not.toContain("private source evidence");expect(detail.record).not.toHaveProperty("amount_minor");
  await expect(readExecutiveEvidence(db,f.owner,"close-blockers",first.items[0].id,{...filters,to:"2026-04-30"})).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});
  await expect(searchExecutiveEvidence(db,f.claimant,{metric:"close-blockers",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  const audit=(await db.prepare(`SELECT provider_id,action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events
    WHERE company_id=? AND actor_id=? AND provider_id='finance-close-checks' ORDER BY created_at,id`).bind(f.company,f.owner.userId).all<{provider_id:string;action:string;record_id:string|null;record_version:number|null;fields:string}>()).results;
  expect(audit.map(row=>row.action).sort()).toEqual(["open","search","search"]);
  expect(audit.every(row=>row.provider_id==="finance-close-checks"&&row.record_id&&row.record_version===1)).toBe(true);
  expect(audit.every(row=>JSON.parse(row.fields).includes("source_version")&&!JSON.parse(row.fields).includes("evidence_json"))).toBe(true);
});
it("keeps available inventory exact by product unit and gates audited stock evidence",async()=>{
  const warehouse=key(),kgProduct=key(),eachProduct=key(),heldProduct=key(),now=Date.now();
  await db.prepare("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status) VALUES(?,?,?,?,?,'active')")
    .bind(warehouse,f.company,f.branch,`EX-${warehouse.slice(0,7)}`,"Executive inventory test warehouse").run();
  await db.prepare(`INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status)
    VALUES(?,?,?,'Executive inventory test material','kg',100,0,'active'),(?,?,?,'Executive inventory test item','each',100,0,'active'),(?,?,?,'Executive quarantined item','each',100,0,'active')`)
    .bind(kgProduct,f.company,"EX-AVAILABLE-KG",eachProduct,f.company,"EX-AVAILABLE-EACH",heldProduct,f.company,"EX-HELD-EACH").run();
  await db.prepare("INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,10001,500,1),(?,?,2500,200,1),(?,?,5000,400,1)")
    .bind(warehouse,kgProduct,warehouse,eachProduct,warehouse,heldProduct).run();
  const movementWarehouse=key();
  await db.prepare("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status) VALUES(?,?,?,?,?,'active')")
    .bind(movementWarehouse,f.company,f.branch,"EX-MOVE-TO","Executive inventory movement warehouse").run();
  const transferId=key(),transferNumber=`EX-MOVEMENT-${transferId.slice(0,7)}`;
  await db.prepare(`INSERT INTO inventory_transfers(id,company_id,source_warehouse_id,target_warehouse_id,source_branch_id,target_branch_id,source_warehouse_version,target_warehouse_version,number,transfer_date,reason,status,version,total_value_minor,created_by,prepared_by,approved_by,submitted_at,posted_at,created_at,updated_at)
    VALUES(?,?,?,?,?,?,1,1,?,'2026-02-10','English executive inventory transfer','posted',1,70,?,?,?, ?,?,?,?)`)
    .bind(transferId,f.company,warehouse,movementWarehouse,f.branch,f.branch,transferNumber,f.owner.userId,f.owner.userId,f.owner.userId,now,now,now,now).run();
  const movementEvents=[
    {product:kgProduct,kind:"sale",quantity:-1001,value:-100,date:"2026-01-05",number:"EX-MOVEMENT-KG-SALE"},
    {product:kgProduct,kind:"return",quantity:1,value:1,date:"2026-01-20",number:"EX-MOVEMENT-KG-RETURN"},
    {product:eachProduct,kind:"sale",quantity:-2000,value:-200,date:"2026-01-08",number:"EX-MOVEMENT-EACH-SALE"},
    {product:eachProduct,kind:"return",quantity:500,value:50,date:"2026-01-22",number:"EX-MOVEMENT-EACH-RETURN"},
  ];
  await db.batch(movementEvents.map(event=>db.prepare(`INSERT INTO pos_stock_moves(id,company_id,warehouse_id,product_id,fulfillment_id,source_id,source_number,kind,quantity_milli,value_minor,movement_date,created_at)
    VALUES(?,?,?, ?,NULL,?,?, ?,?,?,?,?)`).bind(key(),f.company,warehouse,event.product,key(),event.number,event.kind,event.quantity,event.value,event.date,now)));
  await db.batch([
    db.prepare("INSERT INTO inventory_transfer_moves(id,transfer_id,warehouse_id,product_id,quantity_milli,value_minor,movement_date,created_at) VALUES(?,?,?,?, -700,-70,'2026-02-10',?)").bind(`${transferId}-out`,transferId,warehouse,eachProduct,now),
    db.prepare("INSERT INTO inventory_transfer_moves(id,transfer_id,warehouse_id,product_id,quantity_milli,value_minor,movement_date,created_at) VALUES(?,?,?,?, 700,70,'2026-02-10',?)").bind(`${transferId}-in`,transferId,movementWarehouse,eachProduct,now),
  ]);
  const party=key(),orderId=key(),orderLine=key();
  await db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'customer',?,'English executive inventory party','THB',?,?)")
    .bind(party,f.company,`EX-${party.slice(0,7)}`,now,now).run();
  await db.prepare(`INSERT INTO commerce_orders(id,company_id,branch_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,subtotal_minor,tax_minor,created_by,created_at,updated_at)
    VALUES(?,?,?,'sales',?,?,?,'THB','2026-01-01','2026-01-02',300,0,?,?,?)`)
    .bind(orderId,f.company,f.branch,`EX-${orderId.slice(0,7)}`,party,warehouse,f.owner.userId,now,now).run();
  await db.prepare(`INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal)
    VALUES(?,?,?,'English executive inventory reservation',3000,0,100,0,300,0,1)`)
    .bind(orderLine,orderId,kgProduct).run();
  await db.prepare(`INSERT INTO inventory_order_reservations(id,order_line_id,warehouse_id,product_id,quantity_milli,expires_at,created_by,reason,created_at)
    VALUES(?,?,?,?,?,?,?,'English executive inventory reservation',?)`)
    .bind(key(),orderLine,warehouse,kgProduct,3000,now+86400000,f.owner.userId,now).run();
  await db.prepare(`INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,created_by,created_at,updated_at)
    VALUES(?,?,?,?,?,'English executive quarantine hold','2026-12-31',?,?,?)`)
    .bind(key(),f.company,f.branch,warehouse,heldProduct,f.owner.userId,now,now).run();

  const actor={...f.owner,branchId:f.branch,departmentId:null,modules:["inventory"]},filters={from:"2026-01-01",to:"2026-01-31",currency:"THB"};
  const companyInventoryActor={...actor,branchId:null,departmentId:null};
  const stockedLocations=await queryMetric(db,companyInventoryActor,"inventory",filters);
  const inventorySourceCount=await db.prepare("SELECT count(*)::text count FROM commerce_stock s JOIN commerce_warehouses w ON w.id=s.warehouse_id WHERE w.company_id=? AND s.quantity_milli>0").bind(f.company).first<{count:string}>();
  expect(stockedLocations.total).toBe(inventorySourceCount?.count);
  const heldStock=await searchExecutiveEvidence(db,companyInventoryActor,{metric:"inventory",filters,query:"EX-HELD-EACH",limit:10});
  expect(heldStock.provider).toBe("inventory-available-stock");expect(heldStock.metric).toBe("inventory");expect(heldStock.items).toHaveLength(1);
  const heldStockDetail=await readExecutiveEvidence(db,companyInventoryActor,"inventory",heldStock.items[0].id,filters);
  expect(heldStockDetail.record).toMatchObject({sku:"EX-HELD-EACH",on_hand_milli:"5000",available_milli:"0",href:"/commerce?view=stock"});
  const locationFirst=await searchExecutiveEvidence(db,companyInventoryActor,{metric:"inventory",filters,query:"EX-AVAILABLE",limit:1});
  const locationSecond=await searchExecutiveEvidence(db,companyInventoryActor,{metric:"inventory",filters,query:"EX-AVAILABLE",limit:1,cursor:locationFirst.nextCursor!});
  expect(locationFirst.hasMore).toBe(true);expect(locationFirst.items).toHaveLength(1);expect(locationSecond.items).toHaveLength(1);expect(locationFirst.items[0].id).not.toBe(locationSecond.items[0].id);
  await expect(queryMetric(db,actor,"inventory",filters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,actor,{metric:"inventory",filters,query:"EX-HELD-EACH",limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  const stock=await queryMetric(db,actor,"available-stock",filters);
  expect(stock).toMatchObject({total:null,unit:"quantity",valueScale:3,status:"ready"});
  expect(stock.rows).toEqual([
    {label:"EX-AVAILABLE-EACH · Executive inventory test item · Executive inventory test warehouse",value:"2500",unit:"each"},
    {label:"EX-AVAILABLE-KG · Executive inventory test material · Executive inventory test warehouse",value:"7001",unit:"kg"},
  ]);
  expect((await queryMetric(db,actor,"available-stock",{...filters,from:"2026-09-01",to:"2026-09-01"})).rows).toEqual(stock.rows);
  expect(formatMetric("7001",{unit:"quantity",currency:null,valueScale:3})).toBe("7.001");
  const byWarehouse=await queryMetric(db,actor,"available-stock",filters,"warehouse");
  const kgSources=await queryMetric(db,actor,"available-stock",filters,"warehouse",{label:"Executive inventory test warehouse",unit:"kg",page:1});
  expect(kgSources.rows).toHaveLength(1);expect(kgSources.rows[0].facts).toMatchObject({unit:'kg',available_milli:7001});
  expect(kgSources.rows[0].owner_args).toMatchObject({warehouseId:warehouse});
  expect(byWarehouse.total).toBeNull();
  expect(byWarehouse.rows).toEqual([{label:"Executive inventory test warehouse",value:"2500",unit:"each"},{label:"Executive inventory test warehouse",value:"7001",unit:"kg"}]);

  const movementFilters={from:"2026-01-01",to:"2026-02-28",currency:"THB"};
  const movements=await queryMetric(db,actor,"inventory-movements",movementFilters);
  expect(movements).toMatchObject({total:null,unit:"quantity",valueScale:3,status:"ready"});
  expect(movements.rows).toEqual([
    {label:"2026-01",value:"-1500",unit:"each"},
    {label:"2026-01",value:"-1000",unit:"kg"},
    {label:"2026-02",value:"0",unit:"each"},
  ]);
  expect((await queryMetric(db,actor,"inventory-movements",{...movementFilters,to:"2026-01-31"})).rows).toEqual(movements.rows.slice(0,2));
  expect((await queryMetric(db,actor,"inventory-movements",movementFilters,"movement-type")).rows).toEqual([
    {label:"pos_return",value:"500",unit:"each"},{label:"pos_return",value:"1",unit:"kg"},
    {label:"pos_sale",value:"-2000",unit:"each"},{label:"pos_sale",value:"-1001",unit:"kg"},
    {label:"transfer",value:"0",unit:"each"},
  ]);
  expect((await queryMetric(db,actor,"inventory-movements",movementFilters,"warehouse")).rows).toEqual([
    {label:"Executive inventory movement warehouse",value:"700",unit:"each"},
    {label:"Executive inventory test warehouse",value:"-2200",unit:"each"},
    {label:"Executive inventory test warehouse",value:"-1000",unit:"kg"},
  ]);

  const noModule={...actor,modules:[]};
  await expect(queryMetric(db,{...noModule,branchId:null},"inventory",filters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,{...noModule,branchId:null},{metric:"inventory",filters,query:"EX-HELD-EACH",limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  await expect(queryMetric(db,noModule,"available-stock",filters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});
  await expect(queryMetric(db,noModule,"inventory-movements",movementFilters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,noModule,{metric:"available-stock",filters,query:"EX-AVAILABLE",limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,noModule,{metric:"inventory-movements",filters:movementFilters,query:"EX-MOVEMENT",limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  const first=await searchExecutiveEvidence(db,actor,{metric:"available-stock",filters,query:"EX-AVAILABLE",limit:1});
  const second=await searchExecutiveEvidence(db,actor,{metric:"available-stock",filters,query:"EX-AVAILABLE",limit:1,cursor:first.nextCursor!});
  expect(first.provider).toBe("inventory-available-stock");
  expect(first.items).toHaveLength(1);expect(second.items).toHaveLength(1);
  expect(first.items[0].id).not.toBe(second.items[0].id);
  const kgEvidence=[...first.items,...second.items].find(item=>item.summary.includes("7.001 kg"));
  expect(kgEvidence).toBeDefined();
  const detail=await readExecutiveEvidence(db,actor,"available-stock",kgEvidence!.id,filters);
  expect(detail.record).toMatchObject({sku:"EX-AVAILABLE-KG",unit:"kg",on_hand_milli:"10001",reserved_milli:"3000",quarantined_milli:"0",available_milli:"7001",href:"/commerce?view=stock"});
  const held=await searchExecutiveEvidence(db,actor,{metric:"available-stock",filters,query:"EX-HELD-EACH",limit:10});
  expect(held.items).toHaveLength(0);
  const deniedDepartment={...actor,departmentId:f.department};
  await expect(searchExecutiveEvidence(db,deniedDepartment,{metric:"available-stock",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,deniedDepartment,{metric:"inventory-movements",filters:movementFilters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  const movementFirst=await searchExecutiveEvidence(db,actor,{metric:"inventory-movements",filters:movementFilters,query:"EX-MOVEMENT",limit:1});
  const movementSecond=await searchExecutiveEvidence(db,actor,{metric:"inventory-movements",filters:movementFilters,query:"EX-MOVEMENT",limit:1,cursor:movementFirst.nextCursor!});
  expect(movementFirst.provider).toBe("inventory-movements");expect(movementFirst.items).toHaveLength(1);expect(movementSecond.items).toHaveLength(1);
  expect(movementFirst.items[0].id).not.toBe(movementSecond.items[0].id);
  const movementDetail=await readExecutiveEvidence(db,actor,"inventory-movements",movementFirst.items[0].id,movementFilters);
  expect(movementDetail.record).toMatchObject({source_type:"transfer",source_id:transferId,source_number:transferNumber,sku:"EX-AVAILABLE-EACH",unit:"each",quantity_milli:expect.stringMatching(/^-?700$/),href:`/stock-transfers/${transferId}`});
  const audit=(await db.prepare(`SELECT provider_id,action,record_version,field_ids::text fields FROM executive_evidence_access_events
    WHERE company_id=? AND actor_id=? AND metric_id='available-stock' ORDER BY created_at,id`).bind(f.company,f.owner.userId).all<{provider_id:string;action:string;record_version:number;fields:string}>()).results;
  expect(audit.map(row=>row.action).sort()).toEqual(["open","search","search"]);
  expect(audit.every(row=>row.provider_id==="inventory-available-stock"&&row.record_version>0)).toBe(true);
  expect(JSON.parse(audit[0].fields)).toEqual(["sku","product_name","unit","warehouse_name","on_hand_milli","reserved_milli","quarantined_milli","available_milli","stock_version"]);
  const movementAudit=(await db.prepare(`SELECT provider_id,action,record_version,field_ids::text fields FROM executive_evidence_access_events
    WHERE company_id=? AND actor_id=? AND metric_id='inventory-movements' ORDER BY created_at,id`).bind(f.company,f.owner.userId).all<{provider_id:string;action:string;record_version:number|null;fields:string}>()).results;
  expect(movementAudit.map(row=>row.action).sort()).toEqual(["open","search","search"]);
  expect(movementAudit.every(row=>row.provider_id==="inventory-movements"&&row.record_version===1)).toBe(true);
  expect(JSON.parse(movementAudit[0].fields)).toEqual(["movement_date","source_type","source_number","sku","product_name","unit","warehouse_name","quantity_milli"]);

  const inventoryReport=(await createReport(db,actor,key(),"Inventory unit summary")).id;
  const inventoryDefinition=definitionSchema.parse({...emptyDefinition("Inventory unit summary"),widgets:[{id:"available",title:"Available stock",metric:"available-stock",kind:"table",width:"full"}]});
  await saveReport(db,actor,inventoryReport,saveSchema.parse({expectedVersion:1,definition:inventoryDefinition,filters:defaultFilters()}),key());
  await expect(saveShare(db,actor,inventoryReport,shareSchema.parse({recipientId:f.claimant.userId,mode:"distribution",widgetIds:["available"],expiresAt:null,filters:defaultFilters()}),key()))
    .rejects.toMatchObject({code:"RECIPIENT_SOURCE_REQUIRED"});
  expect((await reportView(db,actor,inventoryReport)).shares).toEqual([]);
  const movementReport=(await createReport(db,actor,key(),"Inventory movement summary")).id;
  const movementDefinition=definitionSchema.parse({...emptyDefinition("Inventory movement summary"),widgets:[{id:"movements",title:"Inventory movements",metric:"inventory-movements",kind:"table",width:"full"}]});
  await saveReport(db,actor,movementReport,saveSchema.parse({expectedVersion:1,definition:movementDefinition,filters:defaultFilters()}),key());
  await expect(saveShare(db,actor,movementReport,shareSchema.parse({recipientId:f.claimant.userId,mode:"distribution",widgetIds:["movements"],expiresAt:null,filters:defaultFilters()}),key()))
    .rejects.toMatchObject({code:"RECIPIENT_SOURCE_REQUIRED"});
  expect((await reportView(db,actor,movementReport)).shares).toEqual([]);
  const shortageReport=(await createReport(db,actor,key(),"Inventory shortage summary")).id;
  const shortageDefinition=definitionSchema.parse({...emptyDefinition("Inventory shortage summary"),widgets:[{id:"shortages",title:"Inventory shortages",metric:"inventory-shortages",kind:"table",width:"full"}]});
  await saveReport(db,actor,shortageReport,saveSchema.parse({expectedVersion:1,definition:shortageDefinition,filters:defaultFilters()}),key());
  await expect(saveShare(db,actor,shortageReport,shareSchema.parse({recipientId:f.claimant.userId,mode:"distribution",widgetIds:["shortages"],expiresAt:null,filters:defaultFilters()}),key()))
    .rejects.toMatchObject({code:"RECIPIENT_SOURCE_REQUIRED"});
  expect((await reportView(db,actor,shortageReport)).shares).toEqual([]);
  const mutableStock=await searchExecutiveEvidence(db,companyInventoryActor,{metric:"inventory",filters,query:"EX-AVAILABLE-EACH",limit:10});
  expect(mutableStock.items).toHaveLength(1);
  await db.prepare("UPDATE commerce_stock SET quantity_milli=0,version=version+1 WHERE warehouse_id=? AND product_id=?").bind(warehouse,eachProduct).run();
  await expect(readExecutiveEvidence(db,companyInventoryActor,"inventory",mutableStock.items[0].id,filters)).rejects.toMatchObject({code:"EVIDENCE_NOT_FOUND"});
  const locationAudit=(await db.prepare(`SELECT provider_id,action,record_version,field_ids::text fields FROM executive_evidence_access_events WHERE company_id=? AND actor_id=? AND metric_id='inventory' ORDER BY created_at,id`)
    .bind(f.company,f.owner.userId).all<{provider_id:string;action:string;record_version:number;fields:string}>()).results;
  expect(locationAudit.map(row=>row.action).sort()).toEqual(["open","search","search","search","search"]);
  expect(locationAudit.every(row=>row.provider_id==="inventory-available-stock"&&row.record_version>0)).toBe(true);
  expect(JSON.parse(locationAudit[0].fields)).toEqual(["sku","product_name","unit","warehouse_name","on_hand_milli","reserved_milli","quarantined_milli","available_milli","stock_version"]);
});
it("reconciles inventory shortages to independently approved, versioned reorder points and auditable evidence",async()=>{
  const warehouse=key(),stockedProduct=key(),emptyProduct=key(),actor={...f.owner,modules:[...f.owner.modules,"inventory"]},approver={...f.manager,modules:[...f.manager.modules,"inventory"]},filters=defaultFilters();
  await db.prepare("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status) VALUES(?,?,?,?,'Executive reorder warehouse','active')")
    .bind(warehouse,f.company,f.branch,`EX-${warehouse.slice(0,7)}`).run();
  await db.prepare(`INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status)
    VALUES(?,?,?,'Executive reorder stocked item','each',100,0,'active'),(?,?,?,'Executive reorder zero-stock item','kg',100,0,'active')`)
    .bind(stockedProduct,f.company,"EX-REORDER-STOCKED",emptyProduct,f.company,"EX-REORDER-EMPTY").run();
  await db.prepare("INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,2500,200,1)")
    .bind(warehouse,stockedProduct).run();

  const propose=(productId:string,reorderPoint:string,expectedVersion?:number)=>saveInventoryReorderPolicy(actor,{
    warehouseId:warehouse,productId,reorderPoint,reason:"English-only reviewed stocking target",...(expectedVersion?{expectedVersion}:{})
  },key());
  const decide=(row:{id:string;version:number},action:"approve"|"reject"|"retire",reviewer=approver)=>transitionInventoryReorderPolicy(reviewer,row.id,{
    action,expectedVersion:Number(row.version),reason:`English-only ${action} decision with source review`
  },key());
  const noPolicy=await queryMetric(db,actor,"inventory-shortages",filters);
  expect(noPolicy).toMatchObject({total:"0",status:"empty",rows:[]});

  const first=await propose(stockedProduct,"3");
  expect(first).toMatchObject({status:"pending",version:1});
  await expect(decide({id:stringField(first,'id'),version:1},"approve",actor)).rejects.toMatchObject({code:"INDEPENDENT_APPROVER_REQUIRED"});
  expect((await queryMetric(db,actor,"inventory-shortages",filters)).status).toBe("empty");
  const approved=await decide({id:stringField(first,'id'),version:1},"approve");
  expect(approved).toMatchObject({status:"approved",version:2});
  let metric=await queryMetric(db,actor,"inventory-shortages",filters);
  expect(metric).toMatchObject({total:"1",unit:"count",status:"ready",scopeBoundary:{level:"company"}});
  expect(metric.rows).toEqual([{label:"Inventory shortages",value:"1"}]);
  expect((await queryMetric(db,actor,"inventory-shortages",filters,"product")).rows).toEqual([{label:"EX-REORDER-STOCKED · Executive reorder stocked item",value:"1"}]);
  const firstEvidence=await searchExecutiveEvidence(db,actor,{metric:"inventory-shortages",filters,query:"EX-REORDER-STOCKED",limit:10});
  expect(firstEvidence).toMatchObject({provider:"inventory-reorder-shortages",metric:"inventory-shortages"});
  expect(firstEvidence.items).toHaveLength(1);
  const detail=await readExecutiveEvidence(db,actor,"inventory-shortages",firstEvidence.items[0].id,filters);
  expect(detail.record).toMatchObject({sku:"EX-REORDER-STOCKED",unit:"each",on_hand_milli:"2500",available_milli:"2500",approved_reorder_point_milli:"3000",policy_version:2,href:"/commerce?view=stock"});

  const pendingChange=await propose(stockedProduct,"2",2);
  expect(pendingChange.version).toBe(3);
  expect((await queryMetric(db,actor,"inventory-shortages",filters)).total).toBe("1");
  const rejected=await decide({id:stringField(first,'id'),version:3},"reject");
  expect(rejected).toMatchObject({status:"approved",version:4});
  expect((await queryMetric(db,actor,"inventory-shortages",filters)).total).toBe("1");
  const replacement=await propose(stockedProduct,"2",4);
  await decide({id:stringField(first,'id'),version:5},"approve");
  metric=await queryMetric(db,actor,"inventory-shortages",filters);
  expect(metric).toMatchObject({total:"0",status:"ready"});
  expect(metric.rows).toEqual([{label:"Inventory shortages",value:"0"}]);

  const missingStock=await propose(emptyProduct,"1");
  await decide({id:stringField(missingStock,'id'),version:1},"approve");
  metric=await queryMetric(db,actor,"inventory-shortages",filters);
  expect(metric.total).toBe("1");
  const missingEvidence=await searchExecutiveEvidence(db,actor,{metric:"inventory-shortages",filters,query:"EX-REORDER-EMPTY",limit:10});
  expect(missingEvidence.items).toHaveLength(1);
  const missingDetail=await readExecutiveEvidence(db,actor,"inventory-shortages",missingEvidence.items[0].id,filters);
  expect(missingDetail.record).toMatchObject({sku:"EX-REORDER-EMPTY",unit:"kg",on_hand_milli:"0",available_milli:"0",approved_reorder_point_milli:"1000"});

  const policies=await listInventoryReorderPolicies(actor),stocked=policies.find(row=>row.product_id===stockedProduct)!,empty=policies.find(row=>row.product_id===emptyProduct)!;
  expect(stocked).toMatchObject({current_reorder_point_milli:"2000",proposed_reorder_point_milli:null,last_decision:"approved",version:6});
  expect(empty).toMatchObject({current_reorder_point_milli:"1000",retired_at:null});
  await decide(empty,"retire",actor);
  expect((await queryMetric(db,actor,"inventory-shortages",filters)).total).toBe("0");
  const retireStocked=await listInventoryReorderPolicies(actor).then(rows=>rows.find(row=>row.product_id===stockedProduct)!);
  await decide(retireStocked,"retire",actor);
  expect((await queryMetric(db,actor,"inventory-shortages",filters)).status).toBe("empty");

  await expect(queryMetric(db,{...actor,modules:[]},"inventory-shortages",filters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,{...actor,departmentId:f.department},{metric:"inventory-shortages",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  const audit=(await db.prepare(`SELECT provider_id,action,record_version,field_ids::text fields FROM executive_evidence_access_events
    WHERE company_id=? AND actor_id=? AND metric_id='inventory-shortages' ORDER BY created_at,id`).bind(f.company,actor.userId).all<{provider_id:string;action:string;record_version:number;fields:string}>()).results;
  expect(audit.map(row=>row.action).sort()).toEqual(["open","open","search","search"]);
  expect(audit.every(row=>row.provider_id==="inventory-reorder-shortages"&&row.record_version===2)).toBe(true);
  expect(JSON.parse(audit[0].fields)).toEqual(["sku","product_name","unit","warehouse_name","on_hand_milli","reserved_milli","quarantined_milli","available_milli","approved_reorder_point_milli","policy_version","stock_version"]);
  const immutable=await db.prepare("SELECT id FROM inventory_reorder_policy_events WHERE policy_id=? ORDER BY policy_version").bind(first.id).all<{id:string}>();
  expect(immutable.results.map(row=>row.id)).toEqual(Array.from({length:7},(_,index)=>`${first.id}:${index+1}`));
  await expect(db.prepare("UPDATE inventory_reorder_policy_events SET reason='changed' WHERE id=?").bind(immutable.results[0].id).run()).rejects.toThrow();
  expect(replacement.version).toBe(5);
});

it("allows a bounded published live view without granting source-module access",async()=>{
  const id=await report(),base=defaultFilters(),month=filtersForMonth(base,`${base.from.slice(0,4)}-02`)!;await saveShare(db,f.owner,id,shareSchema.parse({recipientId:f.claimant.userId,mode:"distribution",widgetIds:["people"],expiresAt:null,filters:base}),key());
  const view=await reportView(db,f.claimant,id);expect(view.canEdit).toBe(false);expect(view.lockedFilters).toBe(true);expect(view.shares).toEqual([]);expect(view.revisions).toEqual([]);
  expect((await reportData(db,f.claimant,id)).metrics.headcount.error).toBeUndefined();
  expect((await reportData(db,f.claimant,id,month)).metrics.headcount.error).toBeUndefined();
  expect((await reportData(db,f.claimant,id,{...month,from:"2000-01-01"})).metrics.headcount.error).toBeUndefined();
  expect((await reportData(db,f.claimant,id,{...month,currency:"USD"})).metrics.headcount.error).toBeUndefined();
  await expect(saveReport(db,f.claimant,id,saveSchema.parse({expectedVersion:2,definition:emptyDefinition(),filters:defaultFilters()}),key())).rejects.toMatchObject({code:"REPORT_READ_ONLY"});
  expect(await generationHistory(db,f.claimant,id)).toEqual([]);
});
it("uses recipient source access instead of the publisher's authority by default",async()=>{
  const id=await report();await saveShare(db,f.owner,id,shareSchema.parse({recipientId:f.claimant.userId,mode:"recipient",widgetIds:["people"],expiresAt:null,filters:defaultFilters()}),key());
  expect((await reportData(db,f.claimant,id)).metrics.headcount.error).toBeTruthy();
});
it("previews exact numerical values using the recipient's selected active role and the publisher's distribution scope",async()=>{
  const id=await report(),filters=defaultFilters(),contexts=await recipientPreviewContexts(db,f.owner,id,f.claimant.userId);
  expect(contexts).toHaveLength(1);expect(contexts[0]).toMatchObject({role:"employee",scope:"Company-wide"});
  const recipient=await previewShare(db,f.owner,id,sharePreviewSchema.parse({recipientId:f.claimant.userId,mode:"recipient",widgetIds:["people"],filters,assignmentId:contexts[0].id}));
  expect(recipient.recipientContext).toMatchObject({assignmentId:contexts[0].id,role:"employee",scope:"Company-wide"});
  expect(recipient.widgets[0].data).toMatchObject({schemaVersion:2,metric:"headcount",unit:"count",status:"unavailable"});
  expect(recipient.widgets[0].data.error).toContain("unavailable");
  const distribution=await previewShare(db,f.owner,id,sharePreviewSchema.parse({recipientId:f.claimant.userId,mode:"distribution",widgetIds:["people"],filters,assignmentId:null}));
  expect(distribution.recipientContext).toBeNull();expect(distribution.widgets[0].data).toMatchObject({schemaVersion:2,metric:"headcount",unit:"count",status:"ready"});
  expect(BigInt(distribution.widgets[0].data.total!)).toBeGreaterThan(0n);
  expect((await reportView(db,f.owner,id)).shares).toEqual([]);
  await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(),f.claimant.assignmentId).run();
  await expect(previewShare(db,f.owner,id,sharePreviewSchema.parse({recipientId:f.claimant.userId,mode:"distribution",widgetIds:["people"],filters,assignmentId:null}))).rejects.toMatchObject({code:"RECIPIENT_INVALID"});
});
it("transfers ownership to an active Executive and atomically revokes the existing shares",async()=>{
  const id=await report(),version=(await reportView(db,f.owner,id)).version;
  expect((await ownershipCandidates(db,f.owner,id)).people).toEqual([]);
  await db.prepare("UPDATE hr_engagements SET level_id=? WHERE person_id=?").bind(`org-level-${f.company}-2`,`person-${f.claimant.userId}`).run();
  const assignmentId=await f.grant(f.claimant,"executive_author");
  const job=await queueGeneration(db,f.owner,aiEnv,id,generateSchema.parse({question:"Create a current report proposal"}),key());
  const share=await saveShare(db,f.owner,id,shareSchema.parse({recipientId:f.reviewer.userId,mode:"distribution",widgetIds:["people"],expiresAt:null,filters:defaultFilters()}),key());
  const savedShare=await db.prepare("SELECT id,version FROM executive_report_shares WHERE id=?").bind(share.id).first<{id:string;version:number}>();
  const preview=await ownershipCandidates(db,f.owner,id),candidate=preview.people.find(person=>person.id===f.claimant.userId)!;
  expect(candidate.contexts).toContainEqual(expect.objectContaining({id:assignmentId,scope:"Company-wide"}));
  expect(preview.shares).toEqual([savedShare]);
  await saveShare(db,f.owner,id,shareSchema.parse({recipientId:f.head.userId,mode:"distribution",widgetIds:["people"],expiresAt:null,filters:defaultFilters()}),key());
  await expect(transferReportOwnership(db,f.owner,id,transferOwnershipSchema.parse({ownerId:f.claimant.userId,assignmentId,expectedVersion:version,expectedShares:preview.shares}),key())).rejects.toMatchObject({code:"TRANSFER_SHARES_CHANGED"});
  expect(required((await db.prepare("SELECT owner_id FROM executive_reports WHERE id=?").bind(id).first<{owner_id:string}>())).owner_id).toBe(f.owner.userId);
  const currentPreview=await ownershipCandidates(db,f.owner,id),transferKey=key(),result=await transferReportOwnership(db,f.owner,id,transferOwnershipSchema.parse({ownerId:f.claimant.userId,assignmentId,expectedVersion:version,expectedShares:currentPreview.shares}),transferKey);
  expect(result).toMatchObject({id,ownerId:f.claimant.userId,version:version+1,revokedShares:2});
  expect(await db.prepare("SELECT owner_id,version FROM executive_reports WHERE id=?").bind(id).first()).toEqual({owner_id:f.claimant.userId,version:version+1});
  expect(required((await db.prepare("SELECT count(*)::int count FROM executive_report_shares WHERE report_id=? AND revoked_at IS NOT NULL").bind(id).first<{count:number}>())).count).toBe(2);
  expect(required((await db.prepare("SELECT state FROM executive_generation_jobs WHERE id=?").bind(job.id).first<{state:string}>())).state).toBe("cancelled");
  await expect(reportView(db,f.owner,id)).rejects.toMatchObject({code:"REPORT_NOT_FOUND"});
  const audit=await db.prepare("SELECT action,record_version FROM audit_events WHERE command_id=?").bind(transferKey).first<{action:string;record_version:number}>();
  expect(audit).toMatchObject({action:"executive.report.transfer",record_version:version+1});
  await expect(transferReportOwnership(db,f.owner,id,transferOwnershipSchema.parse({ownerId:f.claimant.userId,assignmentId,expectedVersion:version,expectedShares:preview.shares}),key())).rejects.toMatchObject({code:"REPORT_NOT_FOUND"});
});
it("pins shared definitions so later widgets do not expand a grant",async()=>{
  const id=await report();await saveShare(db,f.owner,id,shareSchema.parse({recipientId:f.claimant.userId,mode:"distribution",widgetIds:["people"],expiresAt:null,filters:defaultFilters()}),key());
  const inventoryOwner={...f.owner,modules:[...f.owner.modules,"inventory"]},r=await reportView(db,inventoryOwner,id);await saveReport(db,inventoryOwner,id,saveSchema.parse({expectedVersion:r.version,definition:{...r.definition,widgets:[...r.definition.widgets,{id:"stock",title:"Stock",metric:"inventory"}]},filters:r.filters}),key());
  expect((await reportView(db,f.claimant,id)).definition.widgets.map(w=>w.id)).toEqual(["people"]);
  expect(Object.keys((await reportData(db,f.claimant,id)).metrics)).toEqual(["headcount"]);
});
it("revocation immediately removes the shared view and library metadata",async()=>{
  const id=await report();const s=await saveShare(db,f.owner,id,shareSchema.parse({recipientId:f.claimant.userId,mode:"distribution",widgetIds:["people"],expiresAt:null,filters:defaultFilters()}),key());
  await revokeShare(db,f.owner,id,s.id,key());await expect(reportData(db,f.claimant,id)).rejects.toMatchObject({code:"REPORT_NOT_FOUND"});expect((await reportLibrary(db,f.claimant)).reports).toHaveLength(0);
});
it("stops published reads after the publisher assignment is revoked",async()=>{
  const id=await report();await saveShare(db,f.owner,id,shareSchema.parse({recipientId:f.claimant.userId,mode:"distribution",widgetIds:["people"],expiresAt:null,filters:defaultFilters()}),key());
  await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(),f.owner.assignmentId).run();await expect(reportData(db,f.claimant,id)).rejects.toMatchObject({code:"EXECUTIVE_ACCESS_CHANGED"});
});
it("does not grant staff editor rights or publish a person's private work metrics",async()=>{
  const id=await report("my-tasks");
  await expect(saveShare(db,f.owner,id,shareSchema.parse({recipientId:f.claimant.userId,mode:"recipient",role:"editor",widgetIds:["people"],expiresAt:null,filters:defaultFilters()}),key())).rejects.toMatchObject({code:"EDITOR_INELIGIBLE"});
  await expect(saveShare(db,f.owner,id,shareSchema.parse({recipientId:f.claimant.userId,mode:"distribution",widgetIds:["people"],expiresAt:null,filters:defaultFilters()}),key())).rejects.toMatchObject({code:"PERSONAL_METRIC"});
});
it("durably generates an English proposal once without changing the report",async()=>{
  const id=await report(),k=key(),input=generateSchema.parse({question:"สร้างรายงานผู้บริหาร"});
  await queueGeneration(db,f.owner,aiEnv,id,input,k);await queueGeneration(db,f.owner,aiEnv,id,input,k);
  const generate=vi.fn(async()=>({result:{message:"Review this design.",definition:emptyDefinition("Executive view"),followUps:[]},inputTokens:12,outputTokens:12}));
  await drainExecutiveGeneration(db,aiEnv,generate);expect(generate).toHaveBeenCalledTimes(1);expect(generate.mock.calls[0]).toBeDefined();
  expect((await generationHistory(db,f.owner,id))[0].state).toBe("ready");expect((await reportView(db,f.owner,id)).version).toBe(2);
});
it("cancelled jobs never call the provider",async()=>{
  const id=await report(),k=key();await queueGeneration(db,f.owner,aiEnv,id,generateSchema.parse({question:"Create a report"}),k);await cancelGeneration(db,f.owner,id,k,key());const generate=vi.fn();await drainExecutiveGeneration(db,aiEnv,generate);expect(generate).not.toHaveBeenCalled();
});
it("reconciles money by currency, period and posting state without list truncation",async()=>{
  const party=key(),now=Date.now();
  await db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'customer',?,'Test customer','THB',?,?)").bind(party,f.company,party,now,now).run();
  await db.prepare(`INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,status,created_by,created_at,updated_at)
    SELECT ?||n,?,?,'receivable',?||n,'2026-03-01','2026-03-31',CASE WHEN n=303 THEN 'USD' ELSE 'THB' END,1001,1001,501,?,?,CASE WHEN n=302 THEN 'draft' WHEN n=301 THEN 'voided' ELSE 'posted' END,?,?,? FROM generate_series(1,303) n`).bind(key(),f.company,party,key(),f.accounts.cash,f.accounts.revenue,f.owner.userId,now,now).run();
  const revenue=await queryMetric(db,f.owner,"invoiced-sales",{from:"2026-01-01",to:"2026-12-31",currency:"THB"});
  const ar=await queryMetric(db,f.owner,"receivables",{from:"2026-01-01",to:"2026-12-31",currency:"THB"});
  expect(revenue.total).toBe("300300");expect(ar.total).toBe("150300");expect(revenue.rows).toEqual([{label:"2026-03",value:"300300"}]);
  const sourceFilters={from:"2026-01-01",to:"2026-12-31",currency:"THB"};
  const sourceFirst=await queryMetric(db,f.owner,"invoiced-sales",sourceFilters,"default",{label:"2026-03",page:1});
  const sourceLast=await queryMetric(db,f.owner,"invoiced-sales",sourceFilters,"default",{label:"2026-03",page:10});
  expect(sourceFirst.rows).toHaveLength(31);expect(sourceFirst.hasNext).toBe(true);expect(sourceLast.rows).toHaveLength(30);expect(sourceLast.hasNext).toBe(false);
  expect(sourceFirst.rows.every(row=>row.owner_kind==='invoice'&&row.facts.status==='posted'&&row.facts.currency==='THB')).toBe(true);
  expect(sourceLast.rows.some(row=>sourceFirst.rows.slice(0,30).some(first=>first.id===row.id))).toBe(false);
  const direct=await queryMetric(db,f.owner,"invoiced-sales",sourceFilters,"default",{label:"2026-03",page:1,recordId:sourceLast.rows[29].id});
  expect(direct.rows[0].id).toBe(sourceLast.rows[29].id);
  await expect(queryMetric(db,f.owner,"invoiced-sales",sourceFilters,"default",{label:"2026-04",page:1})).rejects.toMatchObject({code:'METRIC_GROUP_NOT_FOUND'});
  await expect(queryMetric(db,f.claimant,"invoiced-sales",sourceFilters,"default",{label:"2026-03",page:1})).rejects.toMatchObject({code:'METRIC_FORBIDDEN'});
  expect((await queryMetric(db,f.owner,"invoiced-sales",{from:"2026-01-01",to:"2026-12-31",currency:"USD"})).total).toBe("1001");
  expect((await queryMetric(db,f.owner,"invoiced-sales",{from:"2026-04-01",to:"2026-04-30",currency:"THB"})).total).toBe("0");
});
it("reconciles posted net payroll to posted Finance journals and keeps evidence company-scoped and audited",async()=>{
  const filters={from:"2026-01-01",to:"2026-12-31",currency:"THB"},payGroup=`executive-pay-group-${key()}`,engagements=[f.owner,f.claimant,f.head,f.reviewer].map(person=>`eng-${person.userId}`),now=Date.now();
  await db.prepare("INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,created_by,created_at,updated_at) VALUES(?,?,?,'TH','THB','monthly',?,?,?)")
    .bind(payGroup,f.company,"Executive payroll fixture",f.owner.userId,now,now).run();
  async function addRun(month:string,options:{status?:string;currency?:string;journalStatus?:string;posting?:boolean;amounts?:number[]}={}) {
    const id=`executive-payroll-${month}-${key()}`,currency=options.currency??"THB",status=options.status??"posted",amounts=options.amounts??[12345],start=`${month}-01`,end=new Date(Date.UTC(Number(month.slice(0,4)),Number(month.slice(5,7)),0)).toISOString().slice(0,10),payDate=end;
    await db.prepare(`INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,period_start,period_end,pay_date,currency,mode,status,totals_json,version,created_by,created_at,updated_at)
      VALUES(?,?,?,?,?,?,?,'production',?,'{}',1,?,?,?)`).bind(id,f.company,payGroup,start,end,payDate,currency,status,f.owner.userId,now,now).run();
    for(const [index,amount] of amounts.entries()) await db.prepare(`INSERT INTO hr_payroll_items(id,run_id,engagement_id,worker_type,input_snapshot_json,result_json,gross_minor,deductions_minor,employer_cost_minor,net_minor,currency,status,created_at)
      VALUES(?,?,?,'employee','{}','{}',?,?,?, ?,?,'posted',?)`).bind(key(),id,engagements[index%engagements.length],amount+1000,1000,0,amount,currency,now).run();
    if(options.posting!==false) {
      const journal=`executive-payroll-journal-${key()}`;
      await db.prepare(`INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,source_id,status,version,created_by,posted_by,posted_at,created_at,updated_at)
        VALUES(?,?,?,?,?,?,?,'payroll',?,?,1,?,?,?,?,?)`).bind(journal,f.company,f.chart,f.period,`PAYROLL-${key()}`,payDate,currency,id,options.journalStatus??"posted",f.owner.userId,f.owner.userId,now,now,now).run();
      await db.prepare("INSERT INTO hr_payroll_postings(id,run_id,journal_id,created_at) VALUES(?,?,?,?)").bind(key(),id,journal,now).run();
    }
    return id;
  }
  const january=await addRun("2026-01",{amounts:[12345,6]}),march=await addRun("2026-03",{amounts:[6789]}),preview=await addRun("2026-04",{status:"preview",amounts:[9000]}),unposted=await addRun("2026-05",{posting:false,amounts:[12000]}),unpostedJournal=await addRun("2026-06",{journalStatus:"in_review",amounts:[15000]}),wrongCurrency=await addRun("2026-07",{currency:"USD",amounts:[20000]});
  expect([january,march,preview,unposted,unpostedJournal,wrongCurrency]).toHaveLength(6);
  const result=await queryMetric(db,f.owner,"payroll-summary",filters),byMonth=await queryMetric(db,f.owner,"payroll-summary",filters,"month");
  expect(result).toMatchObject({total:"19140",unit:"money",currency:"THB",status:"ready",scopeBoundary:{level:"company"}});
  expect(byMonth.rows).toEqual([{label:"2026-01",value:"12351"},{label:"2026-03",value:"6789"}]);

  const first=await searchExecutiveEvidence(db,f.owner,{metric:"payroll-summary",filters,limit:1});
  const second=await searchExecutiveEvidence(db,f.owner,{metric:"payroll-summary",filters,limit:1,cursor:first.nextCursor!});
  expect(first.provider).toBe("people-payroll-posted-runs");expect(first.items).toHaveLength(1);expect(first.items[0]).toMatchObject({id:march,amount:"6789",currency:"THB",href:`/people/payroll/runs/${march}`});
  expect(second.items).toHaveLength(1);expect(second.items[0].id).toBe(january);
  const detail=await readExecutiveEvidence(db,f.owner,"payroll-summary",january,filters);
  expect(detail.record).toMatchObject({id:january,status:"posted",currency:"THB",net_minor:"12351",href:`/people/payroll/runs/${january}`});
  expect(detail.record).not.toHaveProperty("employee_name");expect(detail.record).not.toHaveProperty("input_snapshot_json");
  await expect(readExecutiveEvidence(db,f.owner,"payroll-summary",preview,filters)).rejects.toMatchObject({code:"EVIDENCE_NOT_FOUND"});
  await expect(readExecutiveEvidence(db,f.owner,"payroll-summary",unposted,filters)).rejects.toMatchObject({code:"EVIDENCE_NOT_FOUND"});
  await expect(readExecutiveEvidence(db,f.owner,"payroll-summary",unpostedJournal,filters)).rejects.toMatchObject({code:"EVIDENCE_NOT_FOUND"});
  await expect(readExecutiveEvidence(db,f.owner,"payroll-summary",wrongCurrency,filters)).rejects.toMatchObject({code:"EVIDENCE_NOT_FOUND"});
  await expect(queryMetric(db,f.claimant,"payroll-summary",filters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,f.claimant,{metric:"payroll-summary",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  const branchOwner={...f.owner,branchId:f.branch};
  await expect(queryMetric(db,branchOwner,"payroll-summary",filters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,branchOwner,{metric:"payroll-summary",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});

  const audit=(await db.prepare(`SELECT provider_id,action,record_id,record_version,field_ids::text fields,query_hash FROM executive_evidence_access_events
    WHERE company_id=? AND actor_id=? AND metric_id='payroll-summary' ORDER BY created_at,id`).bind(f.company,f.owner.userId).all<{provider_id:string;action:string;record_id:string|null;record_version:number|null;fields:string;query_hash:string}>()).results;
  expect(audit.some(row=>row.provider_id==="people-payroll-posted-runs"&&row.action==="search"&&row.record_id===january&&row.record_version===1)).toBe(true);
  expect(audit.some(row=>row.action==="open"&&row.record_id===january&&row.record_version===1)).toBe(true);
  expect(audit.every(row=>JSON.parse(row.fields).every((field:string)=>!field.includes("employee")&&!field.includes("name")&&!field.includes("snapshot")))).toBe(true);

  const reportId=(await createReport(db,f.owner,key(),"Payroll summary access")).id,definition=definitionSchema.parse({...emptyDefinition("Payroll summary access"),widgets:[{id:"payroll",title:"Posted net payroll",metric:"payroll-summary",kind:"line",breakdown:"month",width:"full"}]});
  await saveReport(db,f.owner,reportId,saveSchema.parse({expectedVersion:1,definition,filters}),key());
  await expect(saveShare(db,f.owner,reportId,shareSchema.parse({recipientId:f.claimant.userId,mode:"distribution",widgetIds:["payroll"],expiresAt:null,filters}),key())).rejects.toMatchObject({code:"RECIPIENT_SOURCE_REQUIRED"});
  await saveShare(db,f.owner,reportId,shareSchema.parse({recipientId:f.claimant.userId,mode:"recipient",widgetIds:["payroll"],expiresAt:null,filters}),key());
  expect((await reportData(db,f.claimant,reportId)).metrics["payroll-summary:month"].status).toBe("unavailable");
});
it("reconciles approved leave days and authorized overtime separately to reader-visible requests",async()=>{
  const filters={from:"2026-01-01",to:"2026-12-31",currency:"THB"},now=Date.now();
  async function addLeave(id:string,creator:string,date:string,units:string,state="approved") {
    const personId=`person-${creator}`,employeeId=`executive-leave-employee-${creator}`;
    await db.prepare(`INSERT INTO employees(id,person_id,company_id,branch_id,department_id,display_name,status,created_at)
      VALUES(?,?,?,?,?,?,'active',?) ON CONFLICT(id) DO NOTHING`).bind(employeeId,personId,f.company,f.branch,f.department,`English fixture ${creator}`,now).run();
    await db.prepare(`INSERT INTO leave_requests(id,number,employee_id,company_id,branch_id,start_date,end_date,days,calculated_units,
      handover_note,state,version,created_by,decided_by,created_at,updated_at,department_id,person_id,engagement_id)
      VALUES(?,?,?,?,?,?,?,?,?,'English-only private handover fixture',?,1,?,?,?,?,?,?,?)`)
      .bind(id,`LR-${id}`,employeeId,f.company,f.branch,date,date,Math.max(1,Math.ceil(Number(units))),units,state,creator,state==="approved"?creator:null,now,now,f.department,personId,`eng-${creator}`).run();
  }
  async function addOvertime(id:string,creator:string,date:string,minutes:number,approved=true) {
    const start=Date.parse(`${date}T18:00:00Z`),end=start+minutes*60000;
    await db.prepare(`INSERT INTO overtime_requests(id,company_id,engagement_id,person_id,branch_id,department_id,created_by,mode,state,
      timezone,start_local,end_local,starts_at,ends_at,break_minutes,minutes,reason,version,created_at,updated_at)
      VALUES(?,?,?,?,?,?,?,'retrospective',?,'UTC',?,?,?,?,?,?,'English-only private overtime reason',1,?,?)`)
      .bind(id,f.company,`eng-${creator}`,`person-${creator}`,f.branch,f.department,creator,approved?"approved":"pending_verification",`${date}T18:00:00`,new Date(end).toISOString().slice(0,19),start,end,0,minutes,now,now).run();
    if(approved) await db.prepare(`INSERT INTO overtime_payroll_handoffs(id,request_id,company_id,engagement_id,request_version,work_date,minutes,timezone,approved_by,created_at)
      VALUES(?,?,?,?,1,?,?, 'UTC',?,?)`).bind(`executive-handoff-${id}`,id,f.company,`eng-${creator}`,date,minutes,creator,now).run();
  }

  const febLeave=`executive-leave-feb-${key()}`,junLeave=`executive-leave-jun-${key()}`,outsiderLeave=`executive-leave-private-${key()}`,pendingLeave=`executive-leave-pending-${key()}`;
  const febOvertime=`executive-overtime-feb-${key()}`,junOvertime=`executive-overtime-jun-${key()}`,outsiderOvertime=`executive-overtime-private-${key()}`,pendingOvertime=`executive-overtime-pending-${key()}`;
  await addLeave(febLeave,f.owner.userId,"2026-02-10","1.50");
  await addLeave(junLeave,f.owner.userId,"2026-06-12","0.75");
  await addLeave(outsiderLeave,f.claimant.userId,"2026-06-14","8.00");
  await addLeave(pendingLeave,f.owner.userId,"2026-07-01","3.00","pending_approval");
  await addOvertime(febOvertime,f.owner.userId,"2026-02-15",90);
  await addOvertime(junOvertime,f.owner.userId,"2026-06-18",30);
  await addOvertime(outsiderOvertime,f.claimant.userId,"2026-06-20",600);
  await addOvertime(pendingOvertime,f.owner.userId,"2026-07-03",240,false);

  const leave=await queryMetric(db,f.owner,"approved-leave-duration",filters),leaveMonths=await queryMetric(db,f.owner,"approved-leave-duration",filters,"month");
  expect(leave).toMatchObject({total:"2250",unit:"quantity",valueScale:3,status:"ready",scopeBoundary:{level:"personal"}});
  expect(leaveMonths.rows).toEqual([{label:"2026-02",value:"1500",unit:"days"},{label:"2026-06",value:"750",unit:"days"}]);
  expect((await queryMetric(db,{...f.owner,branchId:f.branch},"approved-leave-duration",filters)).total).toBe("2250");
  const overtime=await queryMetric(db,f.owner,"authorized-overtime-duration",filters),overtimeMonths=await queryMetric(db,f.owner,"authorized-overtime-duration",filters,"month");
  expect(overtime).toMatchObject({total:"7200000",unit:"duration",status:"ready",scopeBoundary:{level:"personal"}});
  expect(overtimeMonths.rows).toEqual([{label:"2026-02",value:"5400000"},{label:"2026-06",value:"1800000"}]);

  const leaveFirst=await searchExecutiveEvidence(db,f.owner,{metric:"approved-leave-duration",filters,limit:1}),
    leaveSecond=await searchExecutiveEvidence(db,f.owner,{metric:"approved-leave-duration",filters,limit:1,cursor:leaveFirst.nextCursor!});
  expect(leaveFirst.provider).toBe("people-leave-overtime-requests");expect(leaveFirst.items).toHaveLength(1);
  expect(leaveFirst.items[0]).toMatchObject({id:junLeave,label:`LR-${junLeave}`,status:"approved",href:`/tasks/people_payroll/${junLeave}`});
  expect(leaveSecond.items[0].id).toBe(febLeave);
  const leaveDetail=await readExecutiveEvidence(db,f.owner,"approved-leave-duration",junLeave,filters);
  expect(leaveDetail.record).toMatchObject({id:junLeave,units:"0.75",state:"approved",href:`/tasks/people_payroll/${junLeave}`});
  expect(leaveDetail.record).not.toHaveProperty("person_id");expect(leaveDetail.record).not.toHaveProperty("handover_note");
  await expect(readExecutiveEvidence(db,f.owner,"approved-leave-duration",outsiderLeave,filters)).rejects.toMatchObject({code:"NOT_FOUND"});
  await expect(readExecutiveEvidence(db,f.owner,"approved-leave-duration",pendingLeave,filters)).rejects.toMatchObject({code:"EVIDENCE_NOT_FOUND"});

  const overtimeFirst=await searchExecutiveEvidence(db,f.owner,{metric:"authorized-overtime-duration",filters,limit:1}),
    overtimeSecond=await searchExecutiveEvidence(db,f.owner,{metric:"authorized-overtime-duration",filters,limit:1,cursor:overtimeFirst.nextCursor!});
  expect(overtimeFirst.items[0]).toMatchObject({id:junOvertime,status:"approved",href:`/overtime/${junOvertime}`});
  expect(overtimeSecond.items[0].id).toBe(febOvertime);
  const overtimeDetail=await readExecutiveEvidence(db,f.owner,"authorized-overtime-duration",junOvertime,filters);
  expect(overtimeDetail.record).toMatchObject({id:junOvertime,work_date:"2026-06-18",minutes:"30",request_version:1,href:`/overtime/${junOvertime}`});
  expect(overtimeDetail.record).not.toHaveProperty("reason");
  await expect(readExecutiveEvidence(db,f.owner,"authorized-overtime-duration",outsiderOvertime,filters)).rejects.toMatchObject({code:"NOT_FOUND"});
  await expect(searchExecutiveEvidence(db,f.owner,{metric:"authorized-overtime-duration",filters,query:pendingOvertime,limit:10})).resolves.toMatchObject({items:[]});
  await expect(queryMetric(db,f.claimant,"approved-leave-duration",filters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,f.claimant,{metric:"authorized-overtime-duration",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});

  const reportId=(await createReport(db,f.owner,key(),"People time summary")).id;
  const definition=definitionSchema.parse({...emptyDefinition("People time summary"),widgets:[{id:"leave",title:"Approved leave days",metric:"approved-leave-duration",kind:"table",width:"half"},{id:"overtime",title:"Authorized overtime",metric:"authorized-overtime-duration",kind:"line",breakdown:"month",width:"half"}]});
  await saveReport(db,f.owner,reportId,saveSchema.parse({expectedVersion:1,definition,filters}),key());
  await expect(saveShare(db,f.owner,reportId,shareSchema.parse({recipientId:f.claimant.userId,mode:"distribution",widgetIds:["leave"],expiresAt:null,filters}),key())).rejects.toMatchObject({code:"PERSONAL_METRIC"});
  const audit=(await db.prepare(`SELECT metric_id,provider_id,action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events
    WHERE company_id=? AND actor_id=? AND provider_id='people-leave-overtime-requests' ORDER BY created_at,id`).bind(f.company,f.owner.userId).all<{metric_id:string;provider_id:string;action:string;record_id:string;record_version:number;fields:string}>()).results;
  expect(audit.some(row=>row.metric_id==="approved-leave-duration"&&row.action==="open"&&row.record_id===junLeave&&row.record_version===1)).toBe(true);
  expect(audit.some(row=>row.metric_id==="authorized-overtime-duration"&&row.action==="open"&&row.record_id===junOvertime&&row.record_version===1)).toBe(true);
  expect(audit.every(row=>JSON.parse(row.fields).every((field:string)=>!field.includes("person")&&!field.includes("employee")&&!field.includes("reason")&&!field.includes("handover")))).toBe(true);
});
it("reconciles hotel occupancy to completed Night Audit snapshots with current PMS report access",async()=>{
  const filters={from:"2026-04-01",to:"2026-05-31",currency:"THB"},now=Date.now(),
    east=`executive-pms-east-${key()}`,west=`executive-pms-west-${key()}`,otherBranch=`${f.company}-west-branch`;
  await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'West Branch',?,?,?)")
    .bind(otherBranch,otherBranch,f.company,f.company,otherBranch).run();
  for(const [id,branch,name] of [[east,f.branch,"East Inn"],[west,otherBranch,"West Inn"]] as const)
    await db.prepare("INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,config_json,created_by,updated_at) VALUES(?,?,?,?,'2026-05-31','{}',?,?)")
      .bind(id,f.company,branch,name,f.owner.userId,now).run();
  async function addNight(propertyId:string,date:string,state:string,capacity:number|null,sold:number|null) {
    const nightId=`executive-night-${key()}`;
    const data={date,state,...(capacity===null||sold===null?{}:{roomMetrics:{capacity,sold}})};
    await db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,'night',?,1,?)")
      .bind(nightId,propertyId,JSON.stringify(data),now).run();
    return nightId;
  }
  const eastOne=await addNight(east,"2026-04-01","completed",10,5),eastTwo=await addNight(east,"2026-04-02","completed",10,10),
    eastMay=await addNight(east,"2026-05-01","completed",20,10),westApr=await addNight(west,"2026-04-01","completed",10,9),
    malformed=await addNight(east,"2026-04-03","completed",null,null),pending=await addNight(east,"2026-04-03","pending",10,4);

  const total=await queryMetric(db,f.owner,"hotel-occupancy",filters),months=await queryMetric(db,f.owner,"hotel-occupancy",filters,"month"),
    branches=await queryMetric(db,f.owner,"hotel-occupancy",filters,"branch"),properties=await queryMetric(db,f.owner,"hotel-occupancy",filters,"property");
  expect(total).toMatchObject({total:"6800",unit:"percent",status:"partial",scopeBoundary:{level:"company"}});
  expect(total.rows).toEqual([{label:"Hotel occupancy",value:"6800"}]);
  expect(months.rows).toEqual([{label:"2026-04",value:"8000"},{label:"2026-05",value:"5000"}]);
  expect(branches.rows).toEqual([{label:"Branch",value:"6250"},{label:"West Branch",value:"9000"}]);
  expect(properties.rows).toEqual([{label:"East Inn",value:"6250"},{label:"West Inn",value:"9000"}]);
  expect((await queryMetric(db,{...f.owner,branchId:f.branch},"hotel-occupancy",filters)).total).toBe("6250");
  await expect(queryMetric(db,{...f.owner,branchId:f.branch,departmentId:f.department},"hotel-occupancy",filters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});
  await expect(queryMetric(db,f.claimant,"hotel-occupancy",filters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});

  const first=await searchExecutiveEvidence(db,f.owner,{metric:"hotel-occupancy",filters,query:"East Inn",limit:2}),
    second=await searchExecutiveEvidence(db,f.owner,{metric:"hotel-occupancy",filters,query:"East Inn",limit:2,cursor:first.nextCursor!});
  expect(first.provider).toBe("pms-night-audit");expect(first.items.map(item=>item.id)).toEqual([eastMay,eastTwo]);
  expect(first.items[0]).toMatchObject({label:"East Inn · 2026-05-01",summary:"Completed Night Audit · 10 sold / 20 sellable room nights",href:`/pms/reports?property=${east}`});
  expect(second.items.map(item=>item.id)).toEqual([eastOne]);
  expect(first.items.some(item=>item.id===malformed||item.id===pending||item.id===westApr)).toBe(false);
  const detail=await readExecutiveEvidence(db,f.owner,"hotel-occupancy",eastTwo,filters);
  expect(detail.record).toMatchObject({id:eastTwo,property_name:"East Inn",business_date:"2026-04-02",state:"completed",sold:"10",capacity:"10",href:`/pms/reports?property=${east}`});
  expect(detail.record).not.toHaveProperty("guest_name");expect(detail.record).not.toHaveProperty("folio_balance");
  await expect(readExecutiveEvidence(db,f.owner,"hotel-occupancy",pending,filters)).rejects.toMatchObject({code:"EVIDENCE_NOT_FOUND"});
  await expect(readExecutiveEvidence(db,f.owner,"hotel-occupancy",malformed,filters)).rejects.toMatchObject({code:"EVIDENCE_NOT_FOUND"});
  await expect(searchExecutiveEvidence(db,f.claimant,{metric:"hotel-occupancy",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,{...f.owner,branchId:f.branch,departmentId:f.department},{metric:"hotel-occupancy",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  const audit=(await db.prepare("SELECT provider_id,action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events WHERE company_id=? AND actor_id=? AND metric_id='hotel-occupancy' ORDER BY created_at,id")
    .bind(f.company,f.owner.userId).all<{provider_id:string;action:string;record_id:string|null;record_version:number|null;fields:string}>()).results;
  expect(audit.some(row=>row.provider_id==="pms-night-audit"&&row.action==="search"&&row.record_id===eastTwo&&row.record_version===1)).toBe(true);
  expect(audit.some(row=>row.action==="open"&&row.record_id===eastTwo&&row.record_version===1)).toBe(true);
  expect(audit.every(row=>JSON.parse(row.fields).every((field:string)=>!field.includes("guest")&&!field.includes("folio")&&!field.includes("entry")))).toBe(true);

  const reportId=(await createReport(db,f.owner,key(),"Hotel occupancy access")).id,
    definition=definitionSchema.parse({...emptyDefinition("Hotel occupancy access"),widgets:[{id:"occupancy",title:"Hotel occupancy",metric:"hotel-occupancy",kind:"line",breakdown:"month",width:"full"}]});
  await saveReport(db,f.owner,reportId,saveSchema.parse({expectedVersion:1,definition,filters}),key());
  await expect(saveShare(db,f.owner,reportId,shareSchema.parse({recipientId:f.claimant.userId,mode:"distribution",widgetIds:["occupancy"],expiresAt:null,filters}),key())).rejects.toMatchObject({code:"RECIPIENT_SOURCE_REQUIRED"});
  await saveShare(db,f.owner,reportId,shareSchema.parse({recipientId:f.claimant.userId,mode:"recipient",widgetIds:["occupancy"],expiresAt:null,filters}),key());
  expect((await reportData(db,f.claimant,reportId)).metrics["hotel-occupancy:month"].status).toBe("unavailable");
});
it("reconciles PMS ADR and RevPAR to completed Night Audits and linked room revenue",async()=>{
  const filters={from:"2026-04-01",to:"2026-04-30",currency:"THB"},now=Date.now(),
    east=`executive-adr-east-${key()}`,west=`executive-adr-west-${key()}`,otherBranch=`${f.company}-adr-west-branch`;
  await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'West Branch',?,?,?)")
    .bind(otherBranch,otherBranch,f.company,f.company,otherBranch).run();
  for(const [id,branch,name,currency] of [[east,f.branch,"East Inn","THB"],[west,otherBranch,"West Inn","THB"]] as const)
    await db.prepare("INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,config_json,created_by,updated_at) VALUES(?,?,?,?,'2026-04-30',?,?,?)")
      .bind(id,f.company,branch,name,JSON.stringify({currency}),f.owner.userId,now).run();
  async function addNight(propertyId:string,date:string,capacity:number|string,sold:number|string,state="completed") {
    const nightId=`executive-adr-night-${key()}`;
    await db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,'night',?,1,?)")
      .bind(nightId,propertyId,JSON.stringify({date,state,roomMetrics:{capacity,sold}}),now).run();
    return nightId;
  }
  async function addEntry(propertyId:string,date:string,kind:string,net:number|string,sourceKey:string,originalId="") {
    const id=`executive-adr-entry-${key()}`;
    await db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,'entry',?,1,?)")
      .bind(id,propertyId,JSON.stringify({date,kind,net,sourceKey,originalId}),now).run();
    return id;
  }
  const eastApr1=await addNight(east,"2026-04-01",20,10),eastApr2=await addNight(east,"2026-04-02",10,5),
    westApr1=await addNight(west,"2026-04-01",5,5),pending=await addNight(east,"2026-04-03",10,5,"pending"),
    malformed=await addNight(east,"2026-04-04","bad",1),eastApr5=await addNight(east,"2026-04-05",4,2);
  await addNight(east,"2026-03-15",10,5);
  const nightCharge=await addEntry(east,"2026-04-01","charge",1200,"night:east-stay:2026-04-01");
  await addEntry(east,"2026-04-02","charge",600,"night:east-stay:2026-04-02");
  await addEntry(east,"2026-04-02","adjustment",-100,"adjustment:room-reversal",nightCharge);
  await addEntry(west,"2026-04-01","charge",2000,"night:west-stay:2026-04-01");
  await addEntry(east,"2026-04-01","charge",9000,"manual:unrelated-entry");
  await addEntry(east,"2026-04-03","charge",9000,"night:pending-stay:2026-04-03");
  await addEntry(east,"2026-04-05","adjustment","bad","night:bad-room-value");
  await addEntry(east,"2026-03-15","charge",1000,"night:previous-period-stay:2026-03-15");

  const nightlyRevenue=async(propertyId:string)=>{
    const property=await db.prepare("SELECT id,company_id,branch_id,name,business_date FROM pms_properties WHERE id=?").bind(propertyId).first<{id:string;company_id:string;branch_id:string;name:string;business_date:string}>();
    if(!property)throw new Error("PMS test property missing");
    const rows=(await db.prepare("SELECT id,kind,data_json,version,updated_at FROM pms_records WHERE property_id=? AND kind IN ('night','entry') ORDER BY id").bind(propertyId).all<{id:string;kind:"night"|"entry";data_json:string;version:number;updated_at:number}>()).results;
    const snapshot={property:{id:property.id,companyId:property.company_id,branchId:property.branch_id,name:property.name,businessDate:property.business_date,state:"open" as const,version:1,config:defaultConfig},records:rows.map(row=>({id:row.id,kind:row.kind,data:JSON.parse(row.data_json) as never,version:Number(row.version),updatedAt:Number(row.updated_at)}))} as Parameters<typeof dailyRoomReport>[0];
    return {night1:dailyRoomReport(snapshot,"2026-04-01").roomRevenue,night2:dailyRoomReport(snapshot,"2026-04-02").roomRevenue};
  };
  expect(await nightlyRevenue(east)).toEqual({night1:1200,night2:500});

  const adr=await queryMetric(db,f.owner,"hotel-adr",filters),adrBranches=await queryMetric(db,f.owner,"hotel-adr",filters,"branch"),
    adrProperties=await queryMetric(db,f.owner,"hotel-adr",filters,"property"),revpar=await queryMetric(db,f.owner,"hotel-revpar",filters),
    revparMonths=await queryMetric(db,f.owner,"hotel-revpar",filters,"month");
  expect(adr).toMatchObject({total:"168",unit:"money",currency:"THB",status:"partial",scopeBoundary:{level:"company"}});
  expect(adr.rows).toEqual([{label:"Hotel ADR",value:"168"}]);
  expect(adrBranches.rows).toEqual([{label:"Branch",value:"100"},{label:"West Branch",value:"400"}]);
  expect(adrProperties.rows).toEqual([{label:"East Inn",value:"100"},{label:"West Inn",value:"400"}]);
  expect(revpar).toMatchObject({total:"95",unit:"money",currency:"THB",status:"partial"});
  expect(revpar.rows).toEqual([{label:"Hotel RevPAR",value:"95"}]);
  expect(revparMonths.rows).toEqual([{label:"2026-04",value:"95"}]);
  expect(metricCatalog(f.owner).find(metric=>metric.id==="hotel-adr")).toMatchObject({supportsComparison:true});
  expect((await queryMetric(db,{...f.owner,branchId:f.branch},"hotel-adr",filters)).total).toBe("100");
  expect((await queryMetric(db,f.owner,"hotel-revpar",{...filters,currency:"USD"})).status).toBe("empty");
  await expect(queryMetric(db,{...f.owner,departmentId:f.department},"hotel-adr",filters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});
  await expect(queryMetric(db,f.claimant,"hotel-revpar",filters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});

  const page=await searchExecutiveEvidence(db,f.owner,{metric:"hotel-adr",filters,query:"East Inn",limit:10});
  expect(page.provider).toBe("pms-night-audit");
  expect(page.items.map(item=>item.id)).toContain(eastApr1);expect(page.items.map(item=>item.id)).toContain(eastApr2);
  expect(page.items.map(item=>item.id)).not.toContain(pending);
  expect(page.items.map(item=>item.id)).not.toContain(malformed);
  expect((await searchExecutiveEvidence(db,f.owner,{metric:"hotel-revpar",filters,query:"West Inn",limit:10})).items.map(item=>item.id)).toContain(westApr1);
  const detail=await readExecutiveEvidence(db,f.owner,"hotel-adr",eastApr1,filters),badRevenueDetail=await readExecutiveEvidence(db,f.owner,"hotel-revpar",eastApr5,filters);
  expect(detail.record).toMatchObject({property_name:"East Inn",business_date:"2026-04-01",sold:"10",capacity:"20",room_revenue_minor:"1200",invalid_revenue_count:"0",currency:"THB"});
  expect(badRevenueDetail.record).toMatchObject({business_date:"2026-04-05",room_revenue_minor:"0",invalid_revenue_count:"1"});
  expect(detail.fields).toContain("entry.net");
  expect(JSON.stringify(detail)).not.toContain("guest");
  await expect(searchExecutiveEvidence(db,f.claimant,{metric:"hotel-revpar",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  const rateReportId=(await createReport(db,f.owner,key(),"Hotel room rates")).id,
    rateDefinition=definitionSchema.parse({...emptyDefinition("Hotel room rates"),widgets:[
      {id:"adr",title:"Hotel ADR",metric:"hotel-adr",kind:"line",breakdown:"month",comparePeriod:"previous-period",width:"half"},
      {id:"revpar",title:"Hotel RevPAR",metric:"hotel-revpar",kind:"line",breakdown:"month",comparePeriod:"previous-period",width:"half"},
    ]});
  await saveReport(db,f.owner,rateReportId,saveSchema.parse({expectedVersion:1,definition:rateDefinition,filters}),key());
  const rateReportData=await reportData(db,f.owner,rateReportId);
  expect(rateReportData.metrics["hotel-adr:month"].total).toBe("168");
  expect(rateReportData.comparisons?.adr.total).toBe("200");
  expect(rateReportData.comparisons?.revpar.total).toBe("100");
  const audit=(await db.prepare("SELECT provider_id,action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events WHERE company_id=? AND actor_id=? AND metric_id='hotel-adr' ORDER BY created_at,id")
    .bind(f.company,f.owner.userId).all<{provider_id:string;action:string;record_id:string|null;record_version:number|null;fields:string}>()).results;
  expect(audit.some(row=>row.action==="open"&&row.record_id===eastApr1&&row.record_version===1)).toBe(true);
  expect(audit.every(row=>JSON.parse(row.fields).every((field:string)=>!field.toLowerCase().includes("guest")&&!field.toLowerCase().includes("description")&&!field.toLowerCase().includes("folio")))).toBe(true);
});
it("reconciles current PMS folio balances to posted receivable journals without exposing guest fields",async()=>{
  const filters={from:"2020-01-01",to:"2020-01-31",currency:"THB"},now=Date.now(),
    east=`executive-folio-east-${key()}`,west=`executive-folio-west-${key()}`,otherBranch=`${f.company}-folio-west-branch`,
    receivable=`${f.company}-pms-receivable-${key()}`;
  await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'West Branch',?,?,?)")
    .bind(otherBranch,otherBranch,f.company,f.company,otherBranch).run();
  await db.prepare("INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,'asset','THB',?,?)")
    .bind(receivable,f.chart,f.company,receivable,"PMS receivable",now,now).run();
  for(const [id,branch,name] of [[east,f.branch,"East Inn"],[west,otherBranch,"West Inn"]] as const)
    await db.prepare("INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,config_json,created_by,updated_at) VALUES(?,?,?,?,'2026-09-24',?,?,?)")
      .bind(id,f.company,branch,name,JSON.stringify({currency:"THB",accounts:{receivable}}),f.owner.userId,now).run();
  async function addFolio(propertyId:string,id:string,state:string,version=1) {
    await db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,'folio',?, ?,?)")
      .bind(id,propertyId,JSON.stringify({name:"Private guest record",guestId:`guest-${id}`,reservationId:`reservation-${id}`,state,due:1250}),version,now).run();
  }
  async function addPosting(propertyId:string,folioId:string,amount:number,status:"posted"|"draft"|"reversed",suffix:string) {
    const entryId=`executive-entry-${key()}`,journalId=`executive-journal-${key()}`;
    await db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,'entry',?,1,?)")
      .bind(entryId,propertyId,JSON.stringify({folioId,kind:"charge",date:"2026-09-24",total:amount}),now).run();
    await db.prepare(`INSERT INTO finance_journals
      (id,company_id,chart_id,period_id,journal_number,entry_date,currency,source_type,source_id,status,created_by,posted_by,posted_at,created_at,updated_at)
      VALUES(?,?,?,?,?,'2026-09-24','THB','manual',?,?,?, ?,?,?,?)`)
      .bind(journalId,f.company,f.chart,f.period,journalId,entryId,"draft",f.owner.userId,null,null,now,now).run();
    const absolute=Math.abs(amount),receivableDebit=amount>0?absolute:0,receivableCredit=amount<0?absolute:0,
      revenueDebit=amount<0?absolute:0,revenueCredit=amount>0?absolute:0;
    await db.prepare("INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,ordinal) VALUES(?,?,?,?,?,0),(?,?,?,?,?,1)")
      .bind(`line-ar-${key()}`,journalId,receivable,receivableDebit,receivableCredit,`line-revenue-${key()}`,journalId,f.accounts.revenue,revenueDebit,revenueCredit).run();
    if(status!=="draft")await db.prepare("UPDATE finance_journals SET status=?,posted_by=?,posted_at=? WHERE id=?")
      .bind(status,f.owner.userId,now,journalId).run();
    await db.prepare("INSERT INTO pms_postings(id,property_id,source_key,record_id,journal_id,business_date) VALUES(?,?,?,?,?,'2026-09-24')")
      .bind(`pms-posting-${key()}`,propertyId,`folio-fixture:${suffix}`,entryId,journalId).run();
    return entryId;
  }
  const openId=`executive-folio-open-${key()}`,secondOpenId=`executive-folio-open-second-${key()}`,closedId=`executive-folio-closed-${key()}`,
    pendingId=`executive-folio-pending-${key()}`,westId=`executive-folio-west-open-${key()}`;
  await addFolio(east,openId,"open",3);await addFolio(east,secondOpenId,"open");await addFolio(east,closedId,"closed");await addFolio(east,pendingId,"open");await addFolio(west,westId,"open");
  await addPosting(east,openId,2000,"posted","east-charge");await addPosting(east,openId,-750,"posted","east-credit");
  await addPosting(east,secondOpenId,100,"posted","east-second-charge");
  await addPosting(east,closedId,900,"posted","closed-charge");await addPosting(east,pendingId,500,"draft","pending-charge");
  await addPosting(east,pendingId,300,"reversed","reversed-charge");await addPosting(west,westId,400,"posted","west-charge");

  const total=await queryMetric(db,f.owner,"hotel-folio-balance",filters),branches=await queryMetric(db,f.owner,"hotel-folio-balance",filters,"branch"),
    properties=await queryMetric(db,f.owner,"hotel-folio-balance",filters,"property");
  expect(total).toMatchObject({total:"1750",unit:"money",currency:"THB",status:"ready",scopeBoundary:{level:"company"}});
  expect(total.rows).toEqual([{label:"Open folio balances",value:"1750"}]);
  expect(total.definition).toContain("date filter does not apply");
  expect(branches.rows).toEqual([{label:"Branch",value:"1350"},{label:"West Branch",value:"400"}]);
  expect(properties.rows).toEqual([{label:"East Inn",value:"1350"},{label:"West Inn",value:"400"}]);
  expect((await queryMetric(db,{...f.owner,branchId:f.branch},"hotel-folio-balance",filters)).total).toBe("1350");
  expect((await queryMetric(db,f.owner,"hotel-folio-balance",{...filters,currency:"USD"})).status).toBe("empty");
  await expect(queryMetric(db,{...f.owner,departmentId:f.department},"hotel-folio-balance",filters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});
  await expect(queryMetric(db,f.claimant,"hotel-folio-balance",filters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});

  const search=await searchExecutiveEvidence(db,f.owner,{metric:"hotel-folio-balance",filters,query:"East Inn",limit:1}),
    next=await searchExecutiveEvidence(db,f.owner,{metric:"hotel-folio-balance",filters,query:"East Inn",limit:1,cursor:search.nextCursor!});
  expect(search.provider).toBe("pms-open-folios");expect(search.items).toHaveLength(1);expect(next.items).toHaveLength(1);
  expect(new Set([search.items[0].id,next.items[0].id])).toEqual(new Set([openId,secondOpenId]));
  expect(search.items[0]).toMatchObject({id:openId,label:"East Inn · Open folio",summary:"1250 THB posted balance · open",currency:"THB",amount:"1250",href:`/pms/folios?property=${east}&folio=${openId}`});
  expect(JSON.stringify(search)).not.toContain("Private guest record");
  const detail=await readExecutiveEvidence(db,f.owner,"hotel-folio-balance",openId,filters);
  expect(detail.record).toMatchObject({id:openId,property_name:"East Inn",currency:"THB",state:"open",posted_receivable_balance_minor:"1250",version:3,href:`/pms/folios?property=${east}&folio=${openId}`});
  expect(detail.record).not.toHaveProperty("guest_name");expect(detail.record).not.toHaveProperty("guestId");expect(detail.record).not.toHaveProperty("name");
  await expect(readExecutiveEvidence(db,f.owner,"hotel-folio-balance",closedId,filters)).rejects.toMatchObject({code:"EVIDENCE_NOT_FOUND"});
  await expect(searchExecutiveEvidence(db,f.claimant,{metric:"hotel-folio-balance",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  const audit=(await db.prepare("SELECT provider_id,action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events WHERE company_id=? AND actor_id=? AND metric_id='hotel-folio-balance' ORDER BY created_at,id")
    .bind(f.company,f.owner.userId).all<{provider_id:string;action:string;record_id:string|null;record_version:number|null;fields:string}>()).results;
  expect(audit.some(row=>row.provider_id==="pms-open-folios"&&row.action==="search"&&row.record_id===openId&&row.record_version===3)).toBe(true);
  expect(audit.some(row=>row.action==="open"&&row.record_id===openId&&row.record_version===3)).toBe(true);
  expect(audit.every(row=>JSON.parse(row.fields).every((field:string)=>!field.toLowerCase().includes("guest")&&!field.toLowerCase().includes("folio.name")&&!field.toLowerCase().includes("entry")))).toBe(true);
});
it("reconciles blocked Night Audit jobs with property scope and metadata-only evidence",async()=>{
  const filters={from:"2026-04-01",to:"2026-04-30",currency:"THB"},now=Date.now(),
    east=`executive-audit-blockers-east-${key()}`,west=`executive-audit-blockers-west-${key()}`,otherBranch=`${f.company}-audit-blockers-west`;
  await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'West Branch',?,?,?)")
    .bind(otherBranch,otherBranch,f.company,f.company,otherBranch).run();
  for(const [id,branch,name] of [[east,f.branch,"East Inn"],[west,otherBranch,"West Inn"]] as const)
    await db.prepare("INSERT INTO pms_properties(id,company_id,branch_id,name,business_date,config_json,created_by,updated_at) VALUES(?,?,?,?,'2026-04-30','{}',?,?)")
      .bind(id,f.company,branch,name,f.owner.userId,now).run();
  async function addJob(propertyId:string,date:string,state:"blocked"|"pending"|"completed",errorCode:string|null,version=1) {
    const id=`executive-night-audit-job-${key()}`;
    await db.prepare("INSERT INTO pms_records(id,property_id,kind,data_json,version,updated_at) VALUES(?,?,'night',?,?,?)")
      .bind(id,propertyId,JSON.stringify({date,state,message:"English-only private Night Audit worker message"}),version,now).run();
    await db.prepare("INSERT INTO pms_jobs(id,property_id,business_date,state,actor_json,error_code,updated_at) VALUES(?,?,?,?,?,?,?)")
      .bind(id,propertyId,date,state,"{}",errorCode,now).run();
    return id;
  }
  const eastEarly=await addJob(east,"2026-04-01","blocked","PMS_PERIOD_CLOSED",2),
    eastLate=await addJob(east,"2026-04-03","blocked","PMS_JOB_FAILED",3),
    westBlocked=await addJob(west,"2026-04-02","blocked","PMS_JOB_FAILED"),
    pending=await addJob(east,"2026-04-04","pending",null),completed=await addJob(east,"2026-04-05","completed",null),
    mismatched=await addJob(east,"2026-04-06","pending","PMS_JOB_FAILED");
  await db.prepare("UPDATE pms_jobs SET state='blocked' WHERE id=?").bind(mismatched).run();

  const total=await queryMetric(db,f.owner,"hotel-night-audit-blockers",filters),
    properties=await queryMetric(db,f.owner,"hotel-night-audit-blockers",filters,"property"),
    types=await queryMetric(db,f.owner,"hotel-night-audit-blockers",filters,"status"),
    month=await queryMetric(db,f.owner,"hotel-night-audit-blockers",filters,"month");
  expect(total).toMatchObject({total:"3",unit:"count",status:"ready",scopeBoundary:{level:"company"}});
  expect(total.rows).toEqual([{label:"Blocked Night Audit jobs",value:"3"}]);
  expect(properties.rows).toEqual([{label:"East Inn",value:"2"},{label:"West Inn",value:"1"}]);
  expect(types.rows).toEqual([{label:"PMS_JOB_FAILED",value:"2"},{label:"PMS_PERIOD_CLOSED",value:"1"}]);
  expect(month.rows).toEqual([{label:"2026-04",value:"3"}]);
  expect((await queryMetric(db,{...f.owner,branchId:f.branch},"hotel-night-audit-blockers",filters)).total).toBe("2");
  await expect(queryMetric(db,{...f.owner,departmentId:f.department},"hotel-night-audit-blockers",filters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});
  await expect(queryMetric(db,f.claimant,"hotel-night-audit-blockers",filters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});

  const first=await searchExecutiveEvidence(db,f.owner,{metric:"hotel-night-audit-blockers",filters,query:"East Inn",limit:1}),
    second=await searchExecutiveEvidence(db,f.owner,{metric:"hotel-night-audit-blockers",filters,query:"East Inn",limit:1,cursor:first.nextCursor!});
  expect(first.provider).toBe("pms-night-audit-blockers");
  expect(first.items).toMatchObject([{id:eastLate,label:"East Inn · 2026-04-03",summary:"Blocked Night Audit job · PMS_JOB_FAILED",status:"blocked",href:`/pms/night-audit?property=${east}`}]);
  expect(second.items.map(item=>item.id)).toEqual([eastEarly]);
  expect([...first.items,...second.items].some(item=>item.id===pending||item.id===completed||item.id===westBlocked||item.id===mismatched)).toBe(false);
  expect(total.sourceReferences[0].datasets).toContain("pms_records");
  const detail=await readExecutiveEvidence(db,f.owner,"hotel-night-audit-blockers",eastLate,filters);
  expect(detail.record).toMatchObject({id:eastLate,property_name:"East Inn",business_date:"2026-04-03",state:"blocked",error_code:"PMS_JOB_FAILED",version:3,href:`/pms/night-audit?property=${east}`});
  expect(JSON.stringify(detail)).not.toContain("private Night Audit worker message");
  await expect(readExecutiveEvidence(db,f.owner,"hotel-night-audit-blockers",pending,filters)).rejects.toMatchObject({code:"EVIDENCE_NOT_FOUND"});
  await expect(searchExecutiveEvidence(db,f.claimant,{metric:"hotel-night-audit-blockers",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,{...f.owner,departmentId:f.department},{metric:"hotel-night-audit-blockers",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  const audit=(await db.prepare("SELECT provider_id,action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events WHERE company_id=? AND actor_id=? AND metric_id='hotel-night-audit-blockers' ORDER BY created_at,id")
    .bind(f.company,f.owner.userId).all<{provider_id:string;action:string;record_id:string|null;record_version:number|null;fields:string}>()).results;
  expect(audit.some(row=>row.provider_id==="pms-night-audit-blockers"&&row.action==="search"&&row.record_id===eastLate&&row.record_version===3)).toBe(true);
  expect(audit.some(row=>row.action==="open"&&row.record_id===eastLate&&row.record_version===3)).toBe(true);
  expect(audit.every(row=>JSON.parse(row.fields).every((field:string)=>!field.includes("message")&&!field.includes("actor")))).toBe(true);
});
it("shows an exact previous equal-length period comparison from source invoices",async()=>{
  const party=key(),now=Date.now();
  await db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'customer',?,'Comparison customer','THB',?,?)").bind(party,f.company,party,now,now).run();
  for(const [month,total] of [["2026-01",100],["2026-02",175]] as const)await db.prepare(`INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,status,created_by,created_at,updated_at)
    VALUES(?,?,?,'receivable',?,?||'-15',?||'-28','THB',?,?,0,?,?,'posted',?,?,?)`).bind(key(),f.company,party,`comparison-${month}`,month,month,total,total,f.accounts.cash,f.accounts.revenue,f.owner.userId,now,now).run();
  const id=(await createReport(db,f.owner,key(),"Period comparison")).id;
  const definition=definitionSchema.parse({...emptyDefinition("Period comparison"),widgets:[{id:"invoice-month",title:"Invoiced sales",metric:"invoiced-sales",kind:"kpi",width:"half",comparePeriod:"previous-period"}]});
  const filters=defaultFilters();filters.from="2026-02-01";filters.to="2026-02-28";
  await saveReport(db,f.owner,id,saveSchema.parse({expectedVersion:1,definition,filters}),key());
  const data=await reportData(db,f.owner,id);
  expect(data.metrics["invoiced-sales"].total).toBe("175");
  expect(data.comparisons?.["invoice-month"].total).toBe("100");
});
it("gates production metric metadata and reads on the company pilot rollout",async()=>{
  vi.stubEnv("NODE_ENV","production");
  try {
    expect((await metricCatalogFor(db,f.owner)).find((metric)=>metric.id==="headcount")?.available).toBe(false);
    await expect(queryMetric(db,f.owner,"headcount",defaultFilters())).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});
    await db.prepare("INSERT INTO company_feature_rollouts(company_id,module_id,rollout_state,version,reason,changed_by,changed_at) VALUES(?,'people_payroll','pilot',1,'Executive pilot integration test',?,?)")
      .bind(f.company,f.owner.userId,Date.now()).run();
    expect((await metricCatalogFor(db,f.owner)).find((metric)=>metric.id==="headcount")?.available).toBe(true);
    expect((await queryMetric(db,f.owner,"headcount",defaultFilters())).status).toBe("ready");
    await db.prepare("INSERT INTO company_feature_rollouts(company_id,module_id,rollout_state,version,reason,changed_by,changed_at) VALUES(?,'pos','pilot',1,'Executive acceptance gate integration test',?,?)")
      .bind(f.company,f.owner.userId,Date.now()).run();
    const posActor={...f.owner,modules:[...new Set([...f.owner.modules,"pos"])]};
    expect((await metricCatalogFor(db,posActor)).find((metric)=>metric.id==="net-pos-sales")).toMatchObject({available:false,acceptancePending:true});
    await expect(queryMetric(db,posActor,"net-pos-sales",defaultFilters())).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});
  } finally {
    vi.unstubAllEnvs();
  }
});
it("drills through finance metrics with current permission checks and metadata-only audit",async()=>{
  const party=key(),invoice=key(),now=Date.now();
  const filters={from:"2026-01-01",to:"2026-12-31",currency:"THB"};
  await db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'customer',?,'Evidence customer','THB',?,?)")
    .bind(party,f.company,party,now,now).run();
  await db.prepare(`INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,status,created_by,created_at,updated_at)
    VALUES(?,?,?,'receivable','EVIDENCE-INV-01','2026-03-05','2026-04-04','THB',10000,700,10700,10700,?,?, 'posted',?,?,?)`)
    .bind(invoice,f.company,party,f.accounts.cash,f.accounts.revenue,f.owner.userId,now,now).run();
  // An audit write is the final gate for source release. A deferred failure must
  // roll back the whole event statement, then permit the same read to retry.
  await db.prepare("CREATE FUNCTION fail_executive_evidence_audit() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'EXECUTIVE_EVIDENCE_LATE_FAILURE'; END $$").run();
  await db.prepare("CREATE CONSTRAINT TRIGGER fail_executive_evidence_audit AFTER INSERT ON executive_evidence_access_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION fail_executive_evidence_audit()").run();
  try {
    await expect(searchFinanceEvidence(db,f.owner,{metric:"invoiced-sales",filters,query:"EVIDENCE-INV",limit:10})).rejects.toThrow('EXECUTIVE_EVIDENCE_LATE_FAILURE');
    expect(await db.prepare("SELECT count(*)::integer count FROM executive_evidence_access_events WHERE company_id=? AND record_id=?").bind(f.company,invoice).first<{count:number}>()).toEqual({count:0});
  } finally {
    await db.prepare("DROP TRIGGER fail_executive_evidence_audit ON executive_evidence_access_events").run();
    await db.prepare("DROP FUNCTION fail_executive_evidence_audit()").run();
  }
  const result=await searchFinanceEvidence(db,f.owner,{metric:"invoiced-sales",filters,query:"EVIDENCE-INV",limit:10});
  expect(result.items).toHaveLength(1);
  expect(result.items[0]).toMatchObject({id:invoice,invoice_number:"EVIDENCE-INV-01",version:1,href:`/finance/invoices/${invoice}`});
  const detail=await readFinanceEvidence(db,f.owner,"invoiced-sales",invoice,filters);
  expect(detail.record).toMatchObject({id:invoice,party_name:"Evidence customer",total_minor:"10700",outstanding_minor:"10700"});
  await expect(searchFinanceEvidence(db,f.claimant,{metric:"invoiced-sales",filters,query:"EVIDENCE-INV",limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  const audit=(await db.prepare("SELECT provider_id,metric_id,action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events WHERE company_id=? AND record_id=? ORDER BY created_at,id").bind(f.company,invoice).all<{provider_id:string;metric_id:string;action:string;record_id:string;record_version:number;fields:string}>()).results;
  expect(audit).toHaveLength(2);expect(audit.map(row=>row.action)).toEqual(["search","open"]);
  expect(audit.every(row=>row.provider_id==="finance-invoices"&&row.metric_id==="invoiced-sales"&&row.record_version===1)).toBe(true);
  expect(audit.every(row=>!JSON.stringify(row).includes("EVIDENCE-INV-01")&&!JSON.stringify(row).includes("Evidence customer"))).toBe(true);
  await expect(db.prepare("DELETE FROM executive_evidence_access_events WHERE record_id=?").bind(invoice).run()).rejects.toThrow("EXECUTIVE_EVIDENCE_AUDIT_IMMUTABLE");
});
it("reconciles Customer 360 activity to the scoped timeline and audits bounded source reads",async()=>{
  const customer=key(),lead=key(),opportunity=key(),activity=key(),now=Date.parse("2026-03-12T10:00:00Z"),filters={from:"2026-01-01",to:"2026-12-31",currency:"THB"};
  await db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'customer',?,'Executive timeline customer','THB',?,?)")
    .bind(customer,f.company,customer,now,now).run();
  await db.prepare(`INSERT INTO crm_leads(id,company_id,branch_id,department_id,title,contact_name,contact_detail,source,brief,owner_id,due_date,status,created_by,created_at,updated_at)
    VALUES(?,?,?,?,?,'Synthetic contact','Private contact detail','Referral','Private lead brief',?,'2026-03-31','qualified',?,?,?)`)
    .bind(lead,f.company,f.branch,f.department,"Executive activity lead",f.owner.userId,now,now,now).run();
  await db.prepare(`INSERT INTO crm_opportunities(id,lead_id,customer_id,expected_minor,currency,close_date,created_by,created_at,updated_at)
    VALUES(?,?,?,5000,'THB','2026-04-30',?,?,?)`).bind(opportunity,lead,customer,f.owner.userId,now,now).run();
  await db.prepare(`INSERT INTO crm_case_events(id,lead_id,opportunity_id,version,action,actor_id,reason,snapshot_json,created_at)
    VALUES(?,?,NULL,1,'qualify',?,'Private qualification reason','{"source":"isolated integration fixture"}'::jsonb,?)`).bind(`event-${key()}`,lead,f.owner.userId,now).run();
  await db.prepare(`INSERT INTO crm_lead_activities(id,company_id,lead_id,activity_type,subject,details,owner_id,due_date,status,version,created_by,created_at,updated_at)
    VALUES(?,?,?,?,?,'Private activity details',?,'2026-03-20','completed',2,?,?,?)`)
    .bind(activity,f.company,lead,"call","Private activity subject",f.owner.userId,f.owner.userId,now,now+86400000).run();

  const metric=await queryMetric(db,f.owner,"recent-customer-activity",filters);
  const byMonth=await queryMetric(db,f.owner,"recent-customer-activity",filters,"month");
  const byType=await queryMetric(db,f.owner,"recent-customer-activity",filters,"activity-type");
  expect(metric.total).toBe("2");
  expect(byMonth.rows).toEqual([{label:"2026-03",value:"2"}]);
  expect(byType.rows).toEqual([{label:"crm",value:"1"},{label:"crm_activity",value:"1"}]);
  expect((await queryMetric(db,f.owner,"recent-customer-activity",{...filters,from:"2026-04-01",to:"2026-04-30"})).total).toBe("0");
  expect((await queryMetric(db,{...f.owner,branchId:"out-of-scope"},"recent-customer-activity",filters)).total).toBe("0");
  const timeline=await (await import("../src/server/customer-timeline")).customerTimeline(f.owner,customer);
  expect(timeline.items.some(row=>row.id===`crm-activity:${activity}`)).toBe(true);
  expect(JSON.stringify(timeline)).not.toContain("Private activity details");

  const page=await searchExecutiveEvidence(db,f.owner,{metric:"recent-customer-activity",filters,query:"Executive timeline customer",limit:1});
  expect(page.items).toHaveLength(1);expect(page.hasMore).toBe(true);expect(page.nextCursor).toBeTruthy();
  const next=await searchExecutiveEvidence(db,f.owner,{metric:"recent-customer-activity",filters,query:"Executive timeline customer",limit:1,cursor:page.nextCursor!});
  expect(next.items).toHaveLength(1);expect(new Set([...page.items,...next.items].map(row=>row.id)).size).toBe(2);
  const detail=await readExecutiveEvidence(db,f.owner,"recent-customer-activity",`crm-activity:${activity}`,filters);
  expect(detail?.record).toMatchObject({id:`crm-activity:${activity}`,source_type:"crm_activity",customer_id:customer,customer_name:"Executive timeline customer",href:`/crm/${lead}#crm-activities`});
  expect(JSON.stringify(detail)).not.toContain("Private activity details");expect(JSON.stringify(detail)).not.toContain("Private activity subject");
  await expect(searchExecutiveEvidence(db,f.claimant,{metric:"recent-customer-activity",filters,query:"Executive timeline customer",limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  const audit=(await db.prepare("SELECT provider_id,metric_id,action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events WHERE company_id=? AND actor_id=? AND metric_id='recent-customer-activity' ORDER BY created_at,id").bind(f.company,f.owner.userId).all<{provider_id:string;metric_id:string;action:string;record_id:string;record_version:number;fields:string}>()).results;
  expect(audit.map(row=>row.action)).toEqual(["search","search","open"]);
  expect(audit.every(row=>row.provider_id==="customer-360-timeline"&&row.metric_id==="recent-customer-activity"&&row.record_id&&!JSON.stringify(row).includes("Executive timeline customer"))).toBe(true);
  expect(audit.every(row=>row.fields.includes("customer_name")&&row.fields.includes("source_type"))).toBe(true);
});
it("reconciles active Brand profile metrics with bounded, authorized profile evidence",async()=>{
  const foreignCompany=(await expenseFixture()).company;
  const actor={...f.owner,modules:[...new Set([...f.owner.modules,"brand"])]};
  const filters={from:"2026-09-01",to:"2026-09-30",currency:"THB"};
  const currentBranch=key(),olderBranch=key(),suspendedBranch=key(),foreignBranch=key();
  const september=Date.parse("2026-09-15T12:00:00Z"),august=Date.parse("2026-08-15T12:00:00Z");
  const branch=(id:string,name:string,company=f.company,status="active")=>db.prepare(`INSERT INTO organization_nodes
    (id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES(?,'branch',?,?,?, ?,?,?)`)
    .bind(id,`brand-${id.slice(0,8)}`,name,f.company,company,id,status);
  const profile=(scope:string,company:string|null,updatedAt:number,mode="branch")=>db.prepare(`INSERT INTO organization_brand_profiles
    (scope_kind,scope_id,company_id,document_mode,updated_by,updated_at) VALUES('branch',?,?,?,?,?)`)
    .bind(scope,company,mode,f.owner.userId,updatedAt);
  await db.batch([
    branch(currentBranch,"Executive Brand current branch"),
    branch(olderBranch,"Executive Brand older branch"),
    branch(suspendedBranch,"Executive Brand suspended branch",f.company,"suspended"),
    branch(foreignBranch,"Executive Brand foreign branch",foreignCompany),
    profile(f.branch,f.company,september,"both"),
    profile(currentBranch,f.company,september,"branch"),
    profile(olderBranch,f.company,august,"branch"),
    profile(suspendedBranch,f.company,september,"branch"),
    profile(foreignBranch,foreignCompany,september,"branch"),
  ]);

  const configured=await queryMetric(db,actor,"brand-profiles",filters);
  expect(configured).toMatchObject({total:"3",unit:"count",status:"ready"});
  expect((await queryMetric(db,actor,"brand-profiles",filters,"branch")).rows.reduce((total,row)=>total+BigInt(row.value),0n)).toBe(3n);
  const updates=await queryMetric(db,actor,"brand-profile-updates",filters);
  expect(updates).toMatchObject({total:"2",unit:"count",status:"ready"});
  expect((await queryMetric(db,actor,"brand-profile-updates",filters,"month")).rows).toEqual([{label:"2026-09",value:"2"}]);
  expect((await await metricCatalogFor(db,{...actor,branchId:f.branch})).find(metric=>metric.id==="brand-profiles")?.available).toBe(false);

  const first=await searchExecutiveEvidence(db,actor,{metric:"brand-profiles",filters,limit:1});
  const second=await searchExecutiveEvidence(db,actor,{metric:"brand-profiles",filters,limit:5,cursor:first.nextCursor!});
  expect(first.provider).toBe("brand-profile-records");expect(first.items).toHaveLength(1);expect(first.hasMore).toBe(true);
  expect(second.items).toHaveLength(2);expect(new Set([...first.items,...second.items].map(item=>item.id)).size).toBe(3);
  const detail=await readExecutiveEvidence(db,actor,"brand-profiles",first.items[0].id,filters);
  expect(detail.record).toMatchObject({document_mode:"both",primary_configured:false,inverse_configured:false,href:"/settings?section=organization"});
  expect(detail.record).not.toHaveProperty("primary_attachment_id");expect(detail.record).not.toHaveProperty("inverse_attachment_id");
  const updated=await searchExecutiveEvidence(db,actor,{metric:"brand-profile-updates",filters,query:"Executive Brand current",limit:10});
  expect(updated.items).toHaveLength(1);expect(updated.items[0].id).toBe(currentBranch);
  const updatedDetail=await readExecutiveEvidence(db,actor,"brand-profile-updates",updated.items[0].id,filters);
  expect(requiredField(updatedDetail.record,'branch_name')).toBe("Executive Brand current branch");
  await db.prepare("UPDATE organization_brand_profiles SET updated_at=? WHERE scope_kind='branch' AND scope_id=?").bind(august,currentBranch).run();
  await expect(readExecutiveEvidence(db,actor,"brand-profile-updates",currentBranch,filters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});
  await expect(readExecutiveEvidence(db,actor,"brand-profiles",suspendedBranch,filters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});
  await expect(searchExecutiveEvidence(db,{...actor,modules:[]},{metric:"brand-profiles",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,{...actor,branchId:f.branch},{metric:"brand-profiles",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});

  const audit=(await db.prepare(`SELECT provider_id,metric_id,action,record_id,record_version,field_ids::text fields,query_hash
    FROM executive_evidence_access_events WHERE company_id=? AND actor_id=? AND provider_id='brand-profile-records' ORDER BY created_at,id`)
    .bind(f.company,f.owner.userId).all<{provider_id:string;metric_id:string;action:string;record_id:string|null;record_version:number|null;fields:string;query_hash:string}>()).results;
  expect(audit.map(row=>row.action).sort()).toEqual(["open","open","search","search","search","search"]);
  expect(audit.every(row=>row.provider_id==="brand-profile-records"&&row.record_id&&row.record_version&&row.query_hash.length===64)).toBe(true);
  expect(audit.every(row=>JSON.parse(row.fields).join(",")===detail.fields.join(","))).toBe(true);
  expect(JSON.stringify(audit)).not.toContain("Executive Brand");
});

it("reconciles Learning completions and overdue assignments with scoped privacy-limited evidence",async()=>{
  const actor={...f.owner,modules:[...new Set([...f.owner.modules,"learning"])],branchId:f.branch,departmentId:f.department};
  const filters={from:"2026-09-01",to:"2026-09-30",currency:"THB"},now=Date.now(),created=Date.parse("2026-09-01T00:00:00Z");
  const courseIds=Array.from({length:7},()=>`learning-course-${key()}`),assignmentIds=Array.from({length:7},()=>`learning-assignment-${key()}`);
  const courses=[
    {title:"Executive Learning completion fixture Alpha",required:"Annual safety"},
    {title:"Executive Learning completion fixture Beta",required:"Annual safety"},
    {title:"Executive Learning completion outside-date fixture",required:"Annual safety"},
    {title:"Executive Learning completion outside-department fixture",required:"Annual safety"},
    {title:"Executive Learning overdue required fixture",required:"Annual safety"},
    {title:"Executive Learning overdue optional fixture",required:null},
    {title:"Executive Learning overdue completed fixture",required:"Annual safety"},
  ];
  await db.batch(courses.map((course,index)=>db.prepare(`INSERT INTO learning_courses
    (id,company_id,title,description,provider,duration_minutes,required_for,status,created_by,created_at,updated_at)
    VALUES(?,?,?,'English-only private course description','English-only provider',30,?,'published',?,?,?)`)
    .bind(courseIds[index],f.company,course.title,course.required,f.owner.userId,created,created)));
  const yesterday=new Date(Date.now()-86400000).toISOString().slice(0,10),tomorrow=new Date(Date.now()+86400000).toISOString().slice(0,10);
  const assignment=async(index:number,options:{department?:string|null;status?:string;dueDate?:string})=>db.prepare(`INSERT INTO learning_assignments
    (id,company_id,branch_id,department_id,course_id,course_version,learner_id,due_date,status,completion_reason,created_by,version,created_at,updated_at)
    VALUES(?,?,?,?,?,1,?,?,?,?,?,2,?,?)`).bind(
      assignmentIds[index],f.company,f.branch,options.department===undefined?f.department:options.department,courseIds[index],f.claimant.userId,
      options.dueDate??"2026-09-20",options.status??"completed",options.status===undefined||options.status==="completed"?"English-only completion reason":null,
      f.owner.userId,created,now).run();
  await Promise.all([
    assignment(0,{}),assignment(1,{}),assignment(2,{}),assignment(3,{department:f.accountsDepartment}),
    assignment(4,{status:"assigned",dueDate:yesterday}),assignment(5,{status:"assigned",dueDate:tomorrow}),
    assignment(6,{status:"completed",dueDate:yesterday}),
  ]);
  const eventIds=[`learning-event-${key()}`,`learning-event-${key()}`,`learning-event-${key()}`,`learning-event-${key()}`];
  const eventRows=[
    {assignment:assignmentIds[0],at:Date.parse("2026-09-12T10:00:00Z")},
    {assignment:assignmentIds[1],at:Date.parse("2026-09-10T10:00:00Z")},
    {assignment:assignmentIds[2],at:Date.parse("2026-08-31T23:59:59Z")},
    {assignment:assignmentIds[3],at:Date.parse("2026-09-11T10:00:00Z")},
  ];
  await db.batch(eventRows.map((event,index)=>db.prepare(`INSERT INTO learning_assignment_events
    (id,assignment_id,version,action,actor_id,reason,status,created_at) VALUES(?,?,2,'complete',?,'English-only private event reason','completed',?)`)
    .bind(eventIds[index],event.assignment,f.claimant.userId,event.at)));

  const completions=await queryMetric(db,actor,"learning-completions",filters);
  expect(completions).toMatchObject({total:"2",unit:"count",status:"ready"});
  expect(completions.rows).toEqual([{label:"2026-09",value:"2"}]);
  const first=await searchExecutiveEvidence(db,actor,{metric:"learning-completions",filters,query:"Executive Learning completion fixture",limit:1});
  const second=await searchExecutiveEvidence(db,actor,{metric:"learning-completions",filters,query:"Executive Learning completion fixture",limit:1,cursor:first.nextCursor!});
  expect(first.provider).toBe("learning-assignment-records");expect(first.hasMore).toBe(true);expect(second.items).toHaveLength(1);
  const detail=await readExecutiveEvidence(db,actor,"learning-completions",first.items[0].id,filters);
  expect(detail.record).toMatchObject({assignment_id:assignmentIds[0],status:"completed",href:`/learning/${assignmentIds[0]}`});
  expect(detail.record).not.toHaveProperty("learner_id");expect(detail.record).not.toHaveProperty("score_bps");expect(detail.record).not.toHaveProperty("result_evidence");
  await expect(readExecutiveEvidence(db,actor,"learning-completions",eventIds[2],filters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});

  const overdue=await queryMetric(db,actor,"overdue-required-learning",filters);
  expect(overdue).toMatchObject({total:"1",unit:"count",status:"ready"});
  const overduePage=await searchExecutiveEvidence(db,actor,{metric:"overdue-required-learning",filters,query:"Executive Learning overdue",limit:10});
  expect(overduePage.items).toHaveLength(1);expect(overduePage.items[0].id).toBe(assignmentIds[4]);
  const overdueDetail=await readExecutiveEvidence(db,actor,"overdue-required-learning",assignmentIds[4],filters);
  expect(overdueDetail.record).toMatchObject({course_title:"Executive Learning overdue required fixture",status:"assigned",due_date:yesterday,required_for:"Annual safety"});
  await db.prepare("UPDATE learning_assignments SET status='completed',completion_reason='English-only updated fixture reason',version=version+1,updated_at=? WHERE id=?")
    .bind(Date.now(),assignmentIds[4]).run();
  await expect(readExecutiveEvidence(db,actor,"overdue-required-learning",assignmentIds[4],filters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});
  await expect(searchExecutiveEvidence(db,{...actor,modules:[]},{metric:"learning-completions",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});

  const audit=(await db.prepare(`SELECT provider_id,metric_id,action,record_id,record_version,field_ids::text fields,query_hash
    FROM executive_evidence_access_events WHERE company_id=? AND actor_id=? AND provider_id='learning-assignment-records' ORDER BY created_at,id`)
    .bind(f.company,f.owner.userId).all<{provider_id:string;metric_id:string;action:string;record_id:string|null;record_version:number|null;fields:string;query_hash:string}>()).results;
  expect(audit.some(row=>row.metric_id==="learning-completions"&&row.record_id===eventIds[0]&&row.record_version===2)).toBe(true);
  expect(audit.some(row=>row.metric_id==="overdue-required-learning"&&row.record_id===assignmentIds[4])).toBe(true);
  expect(audit.every(row=>row.provider_id==="learning-assignment-records"&&row.record_id&&row.record_version&&row.query_hash.length===64)).toBe(true);
  expect(audit.every(row=>!row.fields.includes("learner")&&!row.fields.includes("score")&&!row.fields.includes("result_evidence"))).toBe(true);
  expect(JSON.stringify(audit)).not.toContain("Executive Learning");
});

it("reconciles Talent requisitions and application stages without candidate evidence",async()=>{
  const actor={...f.owner,modules:[...new Set([...f.owner.modules,"talent"])],branchId:f.branch,departmentId:f.department};
  const filters={from:"2026-09-01",to:"2026-09-30",currency:"THB"},created=Date.parse("2026-09-01T00:00:00Z");
  const requisitions=[`talent-req-${key()}`,`talent-req-${key()}`,`talent-req-${key()}`,`talent-req-${key()}`];
  const insertRequisition=async(id:string,jobTitle:string,department:string,status="open")=>db.prepare(`INSERT INTO talent_requisitions
    (id,company_id,branch_id,department_id,job_title,openings,target_start_date,description,status,created_by,version,created_at,updated_at)
    VALUES(?,?,?, ?,?,2,'2026-11-01','English-only private requisition description',?,?,1,?,?)`)
    .bind(id,f.company,f.branch,department,jobTitle,status,f.owner.userId,created,created).run();
  await Promise.all([
    insertRequisition(requisitions[0],"Executive Talent field engineer",""+f.department),
    insertRequisition(requisitions[1],"Executive Talent service specialist",""+f.department),
    insertRequisition(requisitions[2],"Executive Talent other department",""+f.accountsDepartment),
    insertRequisition(requisitions[3],"Executive Talent closed requisition",""+f.department,"closed"),
  ]);
  const candidateIds=Array.from({length:4},()=>`talent-candidate-${key()}`),applicationIds=Array.from({length:4},()=>`talent-application-${key()}`);
  await db.batch(candidateIds.map((id,index)=>db.prepare(`INSERT INTO talent_candidates
    (id,company_id,legal_name,email,phone,consent_evidence,consented_at,retention_until,status,created_by,version,created_at,updated_at)
    VALUES(?,?,?, ?,?,'English-only private consent evidence',?,'2027-09-01','active',?,1,?,?)`)
    .bind(id,f.company,`English-only private candidate ${index+1}`,`private-candidate-${key()}@example.test`,`+1000000000${index}`,created,f.owner.userId,created,created)));
  const appRows=[
    {requisition:requisitions[0],candidate:candidateIds[0],status:"applied",updated:Date.parse("2026-09-10T10:00:00Z")},
    {requisition:requisitions[1],candidate:candidateIds[1],status:"interview",updated:Date.parse("2026-09-12T10:00:00Z")},
    {requisition:requisitions[2],candidate:candidateIds[2],status:"applied",updated:Date.parse("2026-09-13T10:00:00Z")},
    {requisition:requisitions[0],candidate:candidateIds[3],status:"accepted",updated:Date.parse("2026-08-31T23:59:59Z")},
  ];
  await db.batch(appRows.map((row,index)=>db.prepare(`INSERT INTO talent_applications
    (id,company_id,requisition_id,requisition_version,candidate_id,status,owner_id,created_by,version,created_at,updated_at)
    VALUES(?,?,?,1,?,?,?,?,1,?,?)`).bind(applicationIds[index],f.company,row.requisition,row.candidate,row.status,f.owner.userId,f.owner.userId,created,row.updated)));

  const requisitionMetric=await queryMetric(db,actor,"open-requisitions",filters);
  expect(requisitionMetric).toMatchObject({total:"2",unit:"count",status:"ready"});
  const requisitionsPage=await searchExecutiveEvidence(db,actor,{metric:"open-requisitions",filters,query:"Executive Talent",limit:1});
  const requisitionsNext=await searchExecutiveEvidence(db,actor,{metric:"open-requisitions",filters,query:"Executive Talent",limit:1,cursor:requisitionsPage.nextCursor!});
  expect(requisitionsPage.provider).toBe("talent-recruiting-records");expect(requisitionsPage.hasMore).toBe(true);expect(requisitionsNext.items).toHaveLength(1);
  const requisitionDetail=await readExecutiveEvidence(db,actor,"open-requisitions",requisitionsPage.items[0].id,filters);
  expect(requisitionDetail.record).toMatchObject({status:"open",target_start_date:"2026-11-01",href:`/talent/${requisitionsPage.items[0].id}`});
  expect(requisitionDetail.record).not.toHaveProperty("description");

  const applicationMetric=await queryMetric(db,actor,"application-stage-counts",filters);
  expect(applicationMetric).toMatchObject({total:"2",unit:"count",status:"ready"});
  expect((await queryMetric(db,actor,"application-stage-counts",filters,"status")).rows).toEqual([{label:"applied",value:"1"},{label:"interview",value:"1"}]);
  const applicationPage=await searchExecutiveEvidence(db,actor,{metric:"application-stage-counts",filters,query:"Executive Talent",limit:1});
  const applicationNext=await searchExecutiveEvidence(db,actor,{metric:"application-stage-counts",filters,query:"Executive Talent",limit:1,cursor:applicationPage.nextCursor!});
  expect(applicationPage.hasMore).toBe(true);expect(applicationNext.items).toHaveLength(1);
  const applicationDetail=await readExecutiveEvidence(db,actor,"application-stage-counts",applicationPage.items[0].id,filters);
  expect(applicationDetail.record).toMatchObject({status:"interview",job_title:"Executive Talent service specialist",href:`/talent/${applicationPage.items[0].id}`});
  expect(applicationDetail.record).not.toHaveProperty("candidate_id");expect(applicationDetail.record).not.toHaveProperty("candidate_name");expect(applicationDetail.record).not.toHaveProperty("email");
  expect(JSON.stringify(applicationDetail)).not.toContain("private-candidate-");expect(JSON.stringify(applicationDetail)).not.toContain("English-only private candidate");expect(JSON.stringify(applicationDetail)).not.toContain("private consent evidence");
  await expect(readExecutiveEvidence(db,actor,"application-stage-counts",applicationIds[3],filters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});
  await expect(readExecutiveEvidence(db,actor,"application-stage-counts",applicationIds[2],filters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});
  await expect(readExecutiveEvidence(db,actor,"open-requisitions",requisitions[2],filters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});
  await expect(readExecutiveEvidence(db,actor,"open-requisitions",requisitions[3],filters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});

  const nonReviewer={...f.claimant,modules:["talent"]};
  expect((await await metricCatalogFor(db,nonReviewer)).find(metric=>metric.id==="application-stage-counts")?.available).toBe(false);
  await expect(searchExecutiveEvidence(db,nonReviewer,{metric:"application-stage-counts",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  const audit=(await db.prepare(`SELECT provider_id,metric_id,action,record_id,record_version,field_ids::text fields,query_hash
    FROM executive_evidence_access_events WHERE company_id=? AND actor_id=? AND provider_id='talent-recruiting-records' ORDER BY created_at,id`)
    .bind(f.company,f.owner.userId).all<{provider_id:string;metric_id:string;action:string;record_id:string;record_version:number;fields:string;query_hash:string}>()).results;
  expect(audit.some(row=>row.metric_id==="open-requisitions"&&row.record_id===requisitionsPage.items[0].id)).toBe(true);
  expect(audit.some(row=>row.metric_id==="application-stage-counts"&&row.record_id===applicationIds[0])).toBe(true);
  expect(audit.every(row=>row.provider_id==="talent-recruiting-records"&&row.record_id&&row.record_version&&row.query_hash.length===64)).toBe(true);
  expect(audit.every(row=>!row.fields.includes("candidate")&&!row.fields.includes("email")&&!row.fields.includes("phone"))).toBe(true);
  expect(JSON.stringify(audit)).not.toContain("Executive Talent");
});

it("reconciles CRM opportunity pipeline values with scoped opportunity evidence",async()=>{
  const actor={...f.owner,modules:[...new Set([...f.owner.modules,"crm"])],branchId:f.branch,departmentId:f.department};
  const filters={from:"2026-01-01",to:"2026-12-31",currency:"THB"},now=Date.parse("2026-03-12T10:00:00Z");
  const otherBranch=key(),otherDepartment=key();
  await db.batch([
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'branch',?,'CRM evidence other branch',?,?,?)")
      .bind(otherBranch,otherBranch,f.company,f.company,otherBranch),
    db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,'department',?,'CRM evidence other department',?,?,?)")
      .bind(otherDepartment,otherDepartment,otherBranch,f.company,otherBranch),
    db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'customer',?,'Executive CRM fixture customer','THB',?,?)")
      .bind(`crm-customer-${key()}`,f.company,key(),now,now),
    db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'customer',?,'Foreign CRM fixture customer','THB',?,?)")
      .bind(`crm-customer-foreign-${key()}`,"other-company",key(),now,now),
  ]);
  const customer=String(await db.prepare("SELECT id FROM finance_parties WHERE company_id=? AND name='Executive CRM fixture customer'").bind(f.company).first("id"));
  const foreignCustomer=String(await db.prepare("SELECT id FROM finance_parties WHERE company_id=? AND name='Foreign CRM fixture customer'").bind("other-company").first("id"));
  const insert=async(input:{title:string;branch:string;department:string|null;company?:string;customer:string;stage:string;amount:number;currency:string;closeDate:string})=>{
    const lead=key(),opportunity=key();
    await db.prepare(`INSERT INTO crm_leads(id,company_id,branch_id,department_id,title,contact_name,contact_detail,source,brief,owner_id,due_date,status,created_by,created_at,updated_at)
      VALUES(?,?,?,?,?,'English-only contact name','English-only private contact detail','Referral','English-only private CRM brief',?,'2026-09-30','qualified',?,?,?)`)
      .bind(lead,input.company??f.company,input.branch,input.department,input.title,f.owner.userId,now,now,now).run();
    await db.prepare(`INSERT INTO crm_opportunities(id,lead_id,customer_id,stage,expected_minor,currency,close_date,created_by,created_at,updated_at)
      VALUES(?,?,?,?,?,?,?,?,?,?)`)
      .bind(opportunity,lead,input.customer,input.stage,input.amount,input.currency,input.closeDate,f.owner.userId,now,now).run();
    return {lead,opportunity};
  };
  const discovery=await insert({title:"Executive CRM opportunity discovery fixture",branch:f.branch,department:f.department,customer,stage:"discovery",amount:10000,currency:"THB",closeDate:"2026-03-15"});
  const negotiation=await insert({title:"Executive CRM opportunity negotiation fixture",branch:f.branch,department:f.department,customer,stage:"negotiation",amount:20000,currency:"THB",closeDate:"2026-04-15"});
  const lost=await insert({title:"Executive CRM opportunity closed fixture",branch:f.branch,department:f.department,customer,stage:"lost",amount:90000,currency:"THB",closeDate:"2026-05-15"});
  await insert({title:"Executive CRM opportunity currency fixture",branch:f.branch,department:f.department,customer,stage:"proposal",amount:70000,currency:"USD",closeDate:"2026-05-15"});
  await insert({title:"Executive CRM opportunity date fixture",branch:f.branch,department:f.department,customer,stage:"proposal",amount:80000,currency:"THB",closeDate:"2025-12-31"});
  const otherDepartmentRow=await insert({title:"Executive CRM opportunity department fixture",branch:f.branch,department:f.accountsDepartment,customer,stage:"proposal",amount:40000,currency:"THB",closeDate:"2026-06-15"});
  await insert({title:"Executive CRM opportunity branch fixture",branch:otherBranch,department:otherDepartment,customer,stage:"proposal",amount:50000,currency:"THB",closeDate:"2026-06-15"});
  await insert({title:"Executive CRM opportunity foreign company fixture",branch:f.branch,department:f.department,company:"other-company",customer:foreignCustomer,stage:"proposal",amount:60000,currency:"THB",closeDate:"2026-06-15"});

  const warehouse=key(),nowUtc=String(await db.prepare("SELECT to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') utc_date").first("utc_date"));
  const pastDate=new Date(Date.parse(`${nowUtc}T00:00:00Z`)-86400000).toISOString().slice(0,10);
  const followupFilters={from:new Date(Date.parse(`${nowUtc}T00:00:00Z`)-364*86400000).toISOString().slice(0,10),to:nowUtc,currency:"THB"};
  await db.prepare("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status) VALUES(?,?,?,?,?,'active')")
    .bind(warehouse,f.company,f.branch,`EX-${warehouse.slice(0,8)}`,"Executive CRM follow-up warehouse").run();
  const followupOrder=async(department:string|null,kind:"quote"|"sales"|"purchase",company=f.company,party=customer)=>{
    const id=key();await db.prepare(`INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,state,subtotal_minor,tax_minor,created_by,approved_by,created_at,updated_at)
      VALUES(?,?,?,?,?,?,?,?,'THB','2026-01-01','2026-12-31','confirmed',10000,700,?,?,?,?)`)
      .bind(id,company,f.branch,department,kind,`EX-CRM-FOLLOWUP-${id.slice(0,8)}`,party,warehouse,f.owner.userId,f.owner.userId,now,now).run();return id;
  };
  const inScopeOrder=await followupOrder(f.department,"quote"),otherDepartmentOrder=await followupOrder(f.accountsDepartment,"sales"),purchaseOrder=await followupOrder(f.department,"purchase"),foreignOrder=await followupOrder(f.department,"sales","other-company",foreignCustomer);
  const followup=async(input:{id:string;orderId:string;dueDate:string;nextAction:string;status:"open"|"completed"|"cancelled";company?:string})=>db.prepare(`INSERT INTO customer_followups(id,company_id,order_id,owner_id,due_date,next_action,status,version,created_by,created_at,updated_at)
    VALUES(?,?,?,?,?,?,?,1,?,?,?)`).bind(input.id,input.company??f.company,input.orderId,f.owner.userId,input.dueDate,input.nextAction,input.status,f.owner.userId,now,now).run();
  const overdueFollowupOne=key(),overdueFollowupTwo=key(),todayFollowup=key(),completedFollowup=key(),cancelledFollowup=key(),otherDepartmentFollowup=key(),purchaseFollowup=key(),foreignFollowup=key(),outsideDateFollowup=key();
  await Promise.all([
    followup({id:overdueFollowupOne,orderId:inScopeOrder,dueDate:pastDate,nextAction:"Executive CRM follow-up overdue fixture one",status:"open"}),
    followup({id:overdueFollowupTwo,orderId:inScopeOrder,dueDate:pastDate,nextAction:"Executive CRM follow-up overdue fixture two",status:"open"}),
    followup({id:todayFollowup,orderId:inScopeOrder,dueDate:nowUtc,nextAction:"Executive CRM follow-up due today fixture",status:"open"}),
    followup({id:completedFollowup,orderId:inScopeOrder,dueDate:pastDate,nextAction:"Executive CRM follow-up completed fixture",status:"completed"}),
    followup({id:cancelledFollowup,orderId:inScopeOrder,dueDate:pastDate,nextAction:"Executive CRM follow-up cancelled fixture",status:"cancelled"}),
    followup({id:otherDepartmentFollowup,orderId:otherDepartmentOrder,dueDate:pastDate,nextAction:"Executive CRM follow-up other department fixture",status:"open"}),
    followup({id:purchaseFollowup,orderId:purchaseOrder,dueDate:pastDate,nextAction:"Executive CRM follow-up purchase fixture",status:"open"}),
    followup({id:foreignFollowup,orderId:foreignOrder,dueDate:pastDate,nextAction:"Executive CRM follow-up foreign company fixture",status:"open",company:"other-company"}),
    followup({id:outsideDateFollowup,orderId:inScopeOrder,dueDate:"1999-12-31",nextAction:"Executive CRM follow-up outside date fixture",status:"open"}),
  ]);

  const total=await queryMetric(db,actor,"opportunity-pipeline",filters);
  expect(total).toMatchObject({total:"30000",unit:"money",status:"ready",currency:"THB"});
  expect((await queryMetric(db,actor,"opportunity-pipeline",filters,"status")).rows).toEqual([
    {label:"discovery",value:"10000"},{label:"negotiation",value:"20000"},
  ]);
  expect((await queryMetric(db,{...actor,departmentId:f.accountsDepartment},"opportunity-pipeline",filters)).total).toBe("40000");
  const overdueMetric=await queryMetric(db,actor,"overdue-followups",followupFilters);
  expect(overdueMetric).toMatchObject({total:"2",unit:"count",status:"ready"});
  expect((await queryMetric(db,actor,"overdue-followups",followupFilters,"status")).rows).toEqual([{label:"open",value:"2"}]);
  expect((await queryMetric(db,actor,"overdue-followups",followupFilters,"month")).rows).toEqual([{label:pastDate.slice(0,7),value:"2"}]);

  const first=await searchExecutiveEvidence(db,actor,{metric:"opportunity-pipeline",filters,query:"Executive CRM opportunity",limit:1});
  const second=await searchExecutiveEvidence(db,actor,{metric:"opportunity-pipeline",filters,query:"Executive CRM opportunity",limit:1,cursor:first.nextCursor!});
  expect(first.provider).toBe("crm-opportunities");expect(first.items).toHaveLength(1);expect(first.hasMore).toBe(true);expect(second.items).toHaveLength(1);
  expect(new Set([...first.items,...second.items].map(item=>item.id))).toEqual(new Set([discovery.opportunity,negotiation.opportunity]));
  expect(first.items[0]).toMatchObject({label:expect.stringContaining("Executive CRM opportunity"),currency:"THB",amount:"10000",status:"discovery",href:`/crm/${discovery.lead}`});
  const detail=await readExecutiveEvidence(db,actor,"opportunity-pipeline",first.items[0].id,filters);
  expect(detail.record).toMatchObject({id:discovery.opportunity,lead_id:discovery.lead,stage:"discovery",expected_minor:"10000",currency:"THB",close_date:"2026-03-15",version:1,lead_version:1,href:`/crm/${discovery.lead}`});
  expect(detail.fields).toEqual(["lead.title","lead.status","lead.version","opportunity.stage","opportunity.expected_minor","opportunity.currency","opportunity.close_date","opportunity.version"]);
  expect(JSON.stringify([first,second,detail])).not.toMatch(/contact_detail|contact_name|private CRM brief|customer_name|customer_id/);

  const firstFollowups=await searchExecutiveEvidence(db,actor,{metric:"overdue-followups",filters:followupFilters,query:"Executive CRM follow-up overdue",limit:1});
  const secondFollowups=await searchExecutiveEvidence(db,actor,{metric:"overdue-followups",filters:followupFilters,query:"Executive CRM follow-up overdue",limit:1,cursor:firstFollowups.nextCursor!});
  expect(firstFollowups.provider).toBe("crm-customer-followups");expect(firstFollowups.items).toHaveLength(1);expect(firstFollowups.hasMore).toBe(true);expect(secondFollowups.items).toHaveLength(1);
  expect(new Set([...firstFollowups.items,...secondFollowups.items].map(item=>item.id))).toEqual(new Set([overdueFollowupOne,overdueFollowupTwo]));
  const followupDetail=await readExecutiveEvidence(db,actor,"overdue-followups",firstFollowups.items[0].id,followupFilters);
  expect(followupDetail.record).toMatchObject({id:firstFollowups.items[0].id,status:"open",due_date:pastDate,version:1,order_id:inScopeOrder,order_number:expect.stringMatching(/^EX-CRM-FOLLOWUP-/),href:`/customer-followups/${firstFollowups.items[0].id}`,order_href:`/commerce/orders/${inScopeOrder}`});
  expect(followupDetail.fields).toEqual(["followup.next_action","followup.status","followup.due_date","followup.version","order.id","order.number"]);
  expect(JSON.stringify([firstFollowups,secondFollowups,followupDetail])).not.toMatch(/customer_name|owner_id|customer_id|handoff history|private outcome/);
  await expect(readExecutiveEvidence(db,actor,"overdue-followups",todayFollowup,followupFilters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});
  await expect(readExecutiveEvidence(db,actor,"overdue-followups",completedFollowup,followupFilters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});
  await expect(readExecutiveEvidence(db,actor,"overdue-followups",cancelledFollowup,followupFilters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});
  await expect(readExecutiveEvidence(db,actor,"overdue-followups",otherDepartmentFollowup,followupFilters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});
  await expect(readExecutiveEvidence(db,actor,"overdue-followups",purchaseFollowup,followupFilters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});
  await expect(readExecutiveEvidence(db,actor,"overdue-followups",foreignFollowup,followupFilters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});
  await expect(readExecutiveEvidence(db,actor,"overdue-followups",outsideDateFollowup,followupFilters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});
  await expect(searchExecutiveEvidence(db,{...actor,modules:actor.modules.filter(module=>module!=="crm")},{metric:"overdue-followups",filters:followupFilters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,actor,{metric:"overdue-followups",filters:followupFilters,cursor:"not-a-cursor",limit:10})).rejects.toMatchObject({code:"EVIDENCE_CURSOR_INVALID"});

  const scoped={...actor,branchId:f.branch,departmentId:f.department};
  await expect(readExecutiveEvidence(db,scoped,"opportunity-pipeline",otherDepartmentRow.opportunity,filters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});
  await expect(searchExecutiveEvidence(db,{...actor,modules:actor.modules.filter(module=>module!=="crm")},{metric:"opportunity-pipeline",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  await expect(readExecutiveEvidence(db,actor,"opportunity-pipeline",lost.opportunity,filters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});
  await expect(searchExecutiveEvidence(db,actor,{metric:"opportunity-pipeline",filters,cursor:"not-a-cursor",limit:10})).rejects.toMatchObject({code:"EVIDENCE_CURSOR_INVALID"});

  const audit=(await db.prepare(`SELECT provider_id,metric_id,action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events
    WHERE company_id=? AND actor_id=? AND provider_id='crm-opportunities' ORDER BY created_at,id`)
    .bind(f.company,actor.userId).all<{provider_id:string;metric_id:string;action:string;record_id:string;record_version:number;fields:string}>()).results;
  expect(audit.some(row=>row.action==="search"&&row.record_id===discovery.opportunity&&row.record_version===1)).toBe(true);
  expect(audit.some(row=>row.action==="search"&&row.record_id===discovery.lead&&row.record_version===1)).toBe(true);
  expect(audit.some(row=>row.action==="open"&&row.record_id===discovery.opportunity&&row.record_version===1)).toBe(true);
  expect(audit.every(row=>row.fields.includes("expected_minor")&&!JSON.stringify(row).includes("Executive CRM opportunity"))).toBe(true);
  const followupAudit=(await db.prepare(`SELECT provider_id,metric_id,action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events
    WHERE company_id=? AND actor_id=? AND provider_id='crm-customer-followups' ORDER BY created_at,id`)
    .bind(f.company,actor.userId).all<{provider_id:string;metric_id:string;action:string;record_id:string;record_version:number;fields:string}>()).results;
  expect(followupAudit.some(row=>row.action==="search"&&row.record_id===overdueFollowupOne&&row.record_version===1)).toBe(true);
  expect(followupAudit.some(row=>row.action==="open"&&row.record_id===firstFollowups.items[0].id&&row.record_version===1)).toBe(true);
  expect(followupAudit).toHaveLength(3);
  expect(followupAudit.every(row=>row.provider_id==="crm-customer-followups"&&row.metric_id==="overdue-followups")).toBe(true);
  expect(followupAudit.every(row=>JSON.parse(row.fields).includes("followup.next_action"))).toBe(true);
  expect(JSON.stringify(followupAudit)).not.toContain("Executive CRM follow-up overdue");
});
it("counts only feed-visible Collaboration replies and mentions and never reads message text",async()=>{
  const actor={...f.owner,modules:[...new Set([...f.owner.modules,"collaboration"])]};
  const filters={from:"2026-01-01",to:"2026-12-31",currency:"THB"},post=key(),reply=key(),now=Date.parse("2026-03-12T10:00:00Z");
  await db.prepare(`INSERT INTO social_posts(id,kind,author_id,author_name,company_id,branch_id,body,state,version,created_at,updated_at,department_id)
    VALUES(?,'team',?,'Executive collaboration author',?,NULL,'Secret English post body','published',1,?,?,NULL)`)
    .bind(post,f.owner.userId,f.company,now,now).run();
  await db.prepare(`INSERT INTO social_comments(id,post_id,author_id,author_name,body,state,created_at,updated_at,version)
    VALUES(?,?,?,'Executive collaboration replier','Secret English reply body','published',?,?,2)`)
    .bind(reply,post,f.claimant.userId,now+1000,now+1000).run();
  await db.prepare(`INSERT INTO social_mentions(owner_type,owner_id,target_kind,target_id,target_label,created_by,created_at)
    VALUES('post',?,'person',?,'Secret mention target',?,?),('comment',?,'record',?,'Secret record target',?,?)`)
    .bind(post,f.claimant.userId,f.owner.userId,now,reply,`support/${key()}`,f.claimant.userId,now+2000).run();
  const metric=await queryMetric(db,actor,"collaboration-thread-activity",filters);
  const byMonth=await queryMetric(db,actor,"collaboration-thread-activity",filters,"month");
  const byType=await queryMetric(db,actor,"collaboration-thread-activity",filters,"activity-type");
  expect(metric.total).toBe("3");
  expect(byMonth.rows).toEqual([{label:"2026-03",value:"3"}]);
  expect(byType.rows).toEqual([{label:"mention",value:"2"},{label:"reply",value:"1"}]);
  expect((await queryMetric(db,actor,"collaboration-thread-activity",{...filters,from:"2026-04-01",to:"2026-04-30"})).total).toBe("0");

  const outsidePost=key(),outsideReply=key();
  await db.prepare(`INSERT INTO social_posts(id,kind,author_id,author_name,company_id,branch_id,body,state,created_at,updated_at)
    VALUES(?,'team',?,'Out-of-scope collaboration author',?,'out-of-scope-branch','Hidden outside-branch post','published',?,?)`)
    .bind(outsidePost,f.owner.userId,f.company,now,now).run();
  await db.prepare(`INSERT INTO social_comments(id,post_id,author_id,author_name,body,state,created_at,updated_at)
    VALUES(?,?,?,'Out-of-scope replier','Hidden outside-branch reply','published',?,?)`)
    .bind(outsideReply,outsidePost,f.owner.userId,now,now).run();
  const branchActor={...actor,branchId:f.branch,departmentId:f.department};
  expect((await queryMetric(db,branchActor,"collaboration-thread-activity",filters)).total).toBe("3");
  const noCollaboration={...actor,modules:actor.modules.filter(module=>module!=="collaboration")};
  await expect(queryMetric(db,noCollaboration,"collaboration-thread-activity",filters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,noCollaboration,{metric:"collaboration-thread-activity",filters,query:"Executive collaboration",limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});

  const first=await searchExecutiveEvidence(db,actor,{metric:"collaboration-thread-activity",filters,query:"Executive collaboration",limit:1});
  const second=await searchExecutiveEvidence(db,actor,{metric:"collaboration-thread-activity",filters,query:"Executive collaboration",limit:1,cursor:first.nextCursor!});
  const third=await searchExecutiveEvidence(db,actor,{metric:"collaboration-thread-activity",filters,query:"Executive collaboration",limit:1,cursor:second.nextCursor!});
  expect(first.provider).toBe("collaboration-social-activity");
  expect(new Set([...first.items,...second.items,...third.items].map(item=>item.id)).size).toBe(3);
  expect(JSON.stringify([first,second,third])).not.toContain("Secret English");
  expect(JSON.stringify([first,second,third])).not.toContain("Secret mention target");
  const detail=await readExecutiveEvidence(db,actor,"collaboration-thread-activity",`reply:${reply}`,filters);
  expect(detail.record).toMatchObject({id:`reply:${reply}`,source_type:"reply",activity_actor:"Executive collaboration replier",thread_id:post,href:`/posts/${post}`});
  expect(JSON.stringify(detail)).not.toContain("Secret English reply body");
  const audit=(await db.prepare("SELECT provider_id,metric_id,action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events WHERE company_id=? AND actor_id=? AND metric_id='collaboration-thread-activity' ORDER BY created_at,id")
    .bind(f.company,f.owner.userId).all<{provider_id:string;metric_id:string;action:string;record_id:string;record_version:number;fields:string}>()).results;
  expect(audit.map(row=>row.action).sort()).toEqual(["open","search","search","search"]);
  expect(audit.every(row=>row.provider_id==="collaboration-social-activity"&&row.record_id&&!row.fields.includes("body")&&!row.fields.includes("target_label"))).toBe(true);
});
it("reconciles exact quality inspection rates and scopes evidence to current Quality access",async()=>{
  const warehouse=String((await saveMaster(f.owner,"warehouses",null,{branchId:f.branch,code:key(),name:"Executive quality test warehouse"},key())).id);
  const product=String((await saveMaster(f.owner,"products",null,{sku:key(),name:"Executive quality test product",price:"10"},key())).id);
  await db.prepare("INSERT INTO commerce_stock(warehouse_id,product_id,quantity_milli,value_minor,version) VALUES(?,?,3000,3000,1)").bind(warehouse,product).run();
  const hold=`quality-${key()}`,now=Date.parse("2026-04-06T12:00:00Z");
  await db.prepare(`INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,created_by,created_at,updated_at)
    VALUES(?,?,?,?,?,?,'2026-04-30',?,?,?)`).bind(hold,f.company,f.branch,warehouse,product,"English test inspection hold",f.owner.userId,now,now).run();
  const event=async(id:string,result:"pass"|"fail",date:string,reviewedVersion:number)=>db.prepare(`INSERT INTO inventory_quality_inspections(id,hold_id,reviewed_version,result,evidence,inspected_by,created_at)
    VALUES(?,?,?,?,?,?,?)`).bind(id,hold,reviewedVersion,result,`English test inspection evidence ${id}`,f.claimant.userId,Date.parse(date)).run();
  const marchPass=key(),marchFail=key(),aprilPass=key();
  await event(marchPass,"pass","2026-03-04T09:00:00Z",2);
  await event(marchFail,"fail","2026-03-18T09:00:00Z",3);
  await event(aprilPass,"pass","2026-04-02T09:00:00Z",4);
  const releasedHold=`quality-release-${key()}`,releasedAt=Date.parse("2026-03-25T09:00:00Z");
  await db.prepare(`INSERT INTO inventory_quality_holds(id,company_id,branch_id,warehouse_id,product_id,reason,due_date,created_by,created_at,updated_at)
    VALUES(?,?,?,?,?,?,'2026-03-31',?,?,?)`).bind(releasedHold,f.company,f.branch,warehouse,product,"English test released quality hold",f.owner.userId,releasedAt-86400000,releasedAt-86400000).run();
  await db.prepare(`UPDATE inventory_quality_holds SET status='released',last_result='pass',last_inspector=?,released_by=?,release_reason=?,released_at=?,version=version+1,updated_at=? WHERE id=?`)
    .bind(f.claimant.userId,f.manager.userId,"English test release approval",releasedAt,releasedAt,releasedHold).run();

  const actor={...f.owner,modules:["quality"]},filters={from:"2026-01-01",to:"2026-12-31",currency:"THB"};
  const annual=await queryMetric(db,actor,"inspection-pass-rate",filters);
  const byMonth=await queryMetric(db,actor,"inspection-pass-rate",filters,"month");
  const byBranch=await queryMetric(db,{...actor,branchId:f.branch},"inspection-pass-rate",filters,"branch");
  const openHolds=await queryMetric(db,actor,"quality-holds",filters),releases=await queryMetric(db,actor,"quality-releases",filters);
  expect(annual).toMatchObject({total:"6667",unit:"percent",valueScale:2,status:"ready"});
  expect(byMonth.rows).toEqual([{label:"2026-03",value:"5000"},{label:"2026-04",value:"10000"}]);
  expect(byBranch.total).toBe("6667");
  expect(openHolds.total).toBe("1");
  expect(releases.total).toBe("1");
  expect(formatMetric(required(annual.total),annual)).toBe("66.67%");
  expect((await queryMetric(db,actor,"inspection-pass-rate",{...filters,from:"2026-05-01",to:"2026-05-31"})).status).toBe("empty");

  const found=await searchExecutiveEvidence(db,actor,{metric:"inspection-pass-rate",filters,query:"Executive quality test product",limit:1});
  expect(found.provider).toBe("quality-records");
  expect(found.items[0]).toMatchObject({id:aprilPass,label:"Executive quality test product",version:4,href:`/quality/${hold}`});
  expect(found.hasMore).toBe(true);
  const pageTwo=await searchExecutiveEvidence(db,actor,{metric:"inspection-pass-rate",filters,query:"Executive quality test product",limit:1,cursor:found.nextCursor!});
  expect(pageTwo.items).toHaveLength(1);
  const holdEvidence=await searchExecutiveEvidence(db,actor,{metric:"quality-holds",filters,query:"Executive quality test product",limit:10});
  expect(holdEvidence.items.map(item=>item.id)).toEqual([hold]);
  const releaseEvidence=await searchExecutiveEvidence(db,actor,{metric:"quality-releases",filters,query:"Executive quality test product",limit:10});
  expect(releaseEvidence.items.map(item=>item.id)).toEqual([releasedHold]);
  const holdDetail=await readExecutiveEvidence(db,actor,"quality-holds",hold,filters);
  expect(holdDetail.record).toMatchObject({status:"held",reason:"English test inspection hold",product_name:"Executive quality test product"});
  const releaseDetail=await readExecutiveEvidence(db,actor,"quality-releases",releasedHold,filters);
  expect(releaseDetail.record).toMatchObject({status:"released",release_reason:"English test release approval",released_at:releasedAt});
  const detail=await readExecutiveEvidence(db,actor,"inspection-pass-rate",aprilPass,filters);
  expect(detail.provider).toBe("quality-records");
  expect(detail.record).toMatchObject({result:"pass",evidence:`English test inspection evidence ${aprilPass}`,inspected_by:f.claimant.userId,reviewed_version:4,product_name:"Executive quality test product"});

  const audit=(await db.prepare("SELECT provider_id,metric_id,action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events WHERE company_id=? AND provider_id=? ORDER BY created_at,id")
    .bind(f.company,"quality-records").all<{provider_id:string;metric_id:string;action:string;record_id:string|null;record_version:number|null;fields:string}>()).results;
  expect(audit.map(row=>row.action)).toContain("search");
  expect(audit.some(row=>row.action==="open"&&row.record_id===aprilPass&&row.record_version===4)).toBe(true);
  expect(JSON.stringify(audit)).not.toContain("English test inspection evidence");
  expect(JSON.parse(audit.find(row=>row.action==="open"&&row.record_id===aprilPass)!.fields)).toContain("evidence");

  const withoutQuality={...actor,modules:[]},departmentActor={...actor,departmentId:f.department};
  await expect(queryMetric(db,withoutQuality,"inspection-pass-rate",filters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,withoutQuality,{metric:"inspection-pass-rate",filters})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  await expect(queryMetric(db,departmentActor,"inspection-pass-rate",filters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,departmentActor,{metric:"inspection-pass-rate",filters})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  await expect(readExecutiveEvidence(db,actor,"inspection-pass-rate",marchPass,{from:"2026-04-01",to:"2026-04-30",currency:"THB"})).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});
});
it("reconciles net POS sales and payment mix against completed source transactions",async()=>{
  const warehouse=key(),register=key(),shift=key(),now=Date.now();
  await db.prepare("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status) VALUES(?,?,?,?,?,'active')").bind(warehouse,f.company,f.branch,`EX-${warehouse.slice(0,6)}`,"Executive POS test outlet").run();
  await db.prepare("INSERT INTO pos_registers(id,company_id,branch_id,warehouse_id,name,mode,active) VALUES(?,?,?,?,?,'retail',true)").bind(register,f.company,f.branch,warehouse,"Executive POS test register").run();
  await db.prepare("INSERT INTO pos_shifts(id,register_id,company_id,branch_id,opened_by,opened_at,opening_minor,state) VALUES(?,?,?,?,?,?,0,'closed')").bind(shift,register,f.company,f.branch,f.owner.userId,now).run();
  const sale=async(id:string,number:string,date:string,total:number,state="completed")=>db.prepare(`INSERT INTO pos_sales(id,company_id,branch_id,register_id,shift_id,number,currency,business_date,state,subtotal_minor,total_minor,created_by,created_at,updated_at,accounts_json)
    VALUES(?,?,?,?,?,?,'THB',?,?,?, ?,?,?,?,'{}')`).bind(id,f.company,f.branch,register,shift,number,date,state,total,total,f.owner.userId,now,now).run();
  const first=key(),second=key(),cancelled=key();
  await sale(first,"EX-POS-001","2026-03-05",10700);await sale(second,"EX-POS-002","2026-03-15",5000);await sale(cancelled,"EX-POS-003","2026-03-18",8000,"cancelled");
  const payment=async(id:string,saleId:string,method:"cash"|"edc"|"transfer",amount:number)=>db.prepare(`INSERT INTO pos_payments(id,sale_id,company_id,method,amount_minor,tendered_minor,state,created_by,created_at,updated_at)
    VALUES(?,?,?,?,?,?,'succeeded',?,?,?)`).bind(id,saleId,f.company,method,amount,amount,f.owner.userId,now,now).run();
  const cash=key(),card=key(),transfer=key();await payment(cash,first,"cash",6000);await payment(card,first,"edc",4700);await payment(transfer,second,"transfer",5000);
  const refund=key();await db.prepare(`INSERT INTO pos_refunds(id,sale_id,company_id,branch_id,kind,reason,lines_json,amount_minor,state,requested_by,approved_by,created_at,accounting_date,tax_minor)
    VALUES(?,?,?,?,'sale','Approved source reconciliation','[]',1700,'completed',?,?,?,?,100)`).bind(refund,first,f.company,f.branch,f.owner.userId,f.owner.userId,now,"2026-03-20").run();
  const pendingRefund=key();await db.prepare(`INSERT INTO pos_refunds(id,sale_id,company_id,branch_id,kind,reason,lines_json,amount_minor,state,requested_by,created_at,accounting_date)
    VALUES(?,?,?,?,'sale','Pending refund excluded','[]',600,'processing',?,?,?)`).bind(pendingRefund,second,f.company,f.branch,f.owner.userId,now,"2026-03-21").run();
  await db.prepare("INSERT INTO pos_refund_payments(id,refund_id,payment_id,amount_minor,state) VALUES(?,?,?,1700,'succeeded')").bind(key(),refund,card).run();
  const filters={from:"2026-03-01",to:"2026-03-31",currency:"THB"};
  const posActor={...f.owner,modules:[...new Set([...f.owner.modules,"pos"])]};
  const net=await queryMetric(db,posActor,"net-pos-sales",filters);
  const mix=await queryMetric(db,posActor,"pos-payment-mix",filters);
  const gross=await queryMetric(db,posActor,"pos-sales",filters);
  expect(gross.total).toBe("15700");
  expect(net.total).toBe("14000");expect(net.rows).toEqual([{label:"2026-03",value:"14000"}]);
  expect(mix.total).toBe("14000");expect(mix.rows).toEqual([{label:"cash",value:"6000"},{label:"edc",value:"3000"},{label:"transfer",value:"5000"}]);

  const grossEvidence=await searchExecutiveEvidence(db,posActor,{metric:"pos-sales",filters,limit:1});
  expect(grossEvidence.provider).toBe("pos-transaction-records");
  expect(grossEvidence.items).toMatchObject([{id:`sale:${second}`,amount:"5000",currency:"THB",href:`/pos?tab=history&sale=${second}`}]);
  expect(grossEvidence.hasMore).toBe(true);
  const grossNext=await searchExecutiveEvidence(db,posActor,{metric:"pos-sales",filters,limit:1,cursor:grossEvidence.nextCursor!});
  expect(grossNext.items.map(item=>item.id)).toEqual([`sale:${first}`]);
  const grossDetail=await readExecutiveEvidence(db,posActor,"pos-sales",`sale:${first}`,filters);
  expect(grossDetail.record).toMatchObject({source_type:"sale",receipt_number:"EX-POS-001",amount_minor:"10700",contribution_minor:"10700",version:1});

  const netEvidence=await searchExecutiveEvidence(db,posActor,{metric:"net-pos-sales",filters,limit:25});
  expect(netEvidence.items.map(item=>item.id)).toContain(`refund:${refund}`);
  expect(netEvidence.items.reduce((sum,item)=>sum+BigInt(stringField(item,'amount')??"0"),0n)).toBe(14000n);
  const refundDetail=await readExecutiveEvidence(db,posActor,"net-pos-sales",`refund:${refund}`,filters);
  expect(refundDetail.record).toMatchObject({source_type:"refund",receipt_number:"EX-POS-001",accounting_date:"2026-03-20",amount_minor:"1700",contribution_minor:"-1700",href:`/pos?tab=history&sale=${first}`});
  expect(refundDetail.record).not.toHaveProperty("reason");expect(refundDetail.record).not.toHaveProperty("lines_json");
  expect((await searchExecutiveEvidence(db,posActor,{metric:"net-pos-sales",filters:{...filters,from:"2026-03-21"},limit:25})).items).toEqual([]);

  const paymentEvidence=await searchExecutiveEvidence(db,posActor,{metric:"pos-payment-mix",filters,limit:25});
  expect(paymentEvidence.items.reduce((sum,item)=>sum+BigInt(stringField(item,'amount')??"0"),0n)).toBe(14000n);
  expect((await searchExecutiveEvidence(db,posActor,{metric:"pos-payment-mix",filters,query:"edc",limit:25})).items.map(item=>item.id)).toEqual([`payment:${card}`]);
  const paymentDetail=await readExecutiveEvidence(db,posActor,"pos-payment-mix",`payment:${card}`,filters);
  expect(paymentDetail.record).toMatchObject({source_type:"payment",receipt_number:"EX-POS-001",method:"edc",amount_minor:"4700",refunded_minor:"1700",contribution_minor:"3000"});
  for(const detail of [grossDetail,refundDetail,paymentDetail]) {
    expect(detail.record).not.toHaveProperty("reference");expect(detail.record).not.toHaveProperty("provider_id");
    expect(detail.record).not.toHaveProperty("checkout_url");expect(detail.record).not.toHaveProperty("lines_json");
  }
  const withoutPos={...posActor,modules:posActor.modules.filter(module=>module!=="pos")};
  expect((await metricCatalogFor(db,withoutPos)).find(metric=>metric.id==="pos-sales")?.available).toBe(false);
  await expect(queryMetric(db,withoutPos,"pos-sales",filters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,withoutPos,{metric:"pos-sales",filters})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,{...posActor,departmentId:f.department},{metric:"pos-payment-mix",filters})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  await expect(readExecutiveEvidence(db,posActor,"net-pos-sales",`refund:${pendingRefund}`,filters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});
  const posAudit=(await db.prepare("SELECT provider_id,metric_id,action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events WHERE company_id=? AND provider_id=? ORDER BY created_at,id")
    .bind(f.company,"pos-transaction-records").all<{provider_id:string;metric_id:string;action:string;record_id:string|null;record_version:number|null;fields:string}>()).results;
  expect(posAudit.some(row=>row.metric_id==="net-pos-sales"&&row.action==="open"&&row.record_id===`refund:${refund}`&&row.record_version===1)).toBe(true);
  expect(posAudit.some(row=>row.metric_id==="pos-payment-mix"&&row.action==="open"&&row.record_id===`payment:${card}`)).toBe(true);
  expect(posAudit.every(row=>JSON.parse(row.fields).every((field:string)=>!/(reason|lines_json|reference|provider_id|checkout_url)/i.test(field)))).toBe(true);
  expect(JSON.stringify(posAudit)).not.toContain("Approved source reconciliation");
});
it("reconciles Marketing candidate and engagement metrics with scoped evidence that omits contact and event content",async()=>{
  const now=Date.parse("2026-03-20T12:00:00Z"),form=key(),candidateA=key(),candidateB=key(),candidateC=key(),candidateD=key();
  await db.prepare(`INSERT INTO marketing_assets(id,company_id,kind,name,configuration_json,preview_json,status,created_by,reviewed_by,version,created_at,updated_at)
    VALUES(?,?,'form','Executive Marketing English test form','{"consentRequired":true}','{"headline":"English-only test preview"}','active',?,?,2,?,?)`)
    .bind(form,f.company,f.owner.userId,f.owner.userId,now-86400000,now-86400000).run();
  const candidate=async(id:string,name:string,status:"pending"|"handed_off"|"rejected",department:string|null,createdAt:number,version:number)=>db.prepare(`INSERT INTO marketing_lead_candidates(
    id,company_id,branch_id,department_id,form_id,form_version,contact_name,contact_detail,consent_evidence,attribution_json,brief,status,crm_lead_id,created_by,reviewed_by,version,created_at,updated_at)
    VALUES(?,?,?,?,?,2,?,'private-contact@example.test','private consent text','{"campaign":"English launch","source":"newsletter"}','private lead brief',?,NULL,?,NULL,?,?,?)`)
    .bind(id,f.company,f.branch,department,form,name,status,f.owner.userId,version,createdAt,createdAt+1000).run();
  await candidate(candidateA,"Executive Marketing prospect A","pending",f.department,Date.parse("2026-03-04T08:00:00Z"),2);
  await candidate(candidateB,"Executive Marketing prospect B","handed_off",null,Date.parse("2026-03-09T08:00:00Z"),2);
  await candidate(candidateC,"Executive Marketing prospect C","rejected",f.department,Date.parse("2026-03-10T08:00:00Z"),1);
  await candidate(candidateD,"Executive Marketing prospect D","pending",f.department,Date.parse("2025-12-01T08:00:00Z"),1);
  const event=async(id:string,candidateId:string,version:number,eventType:string,channel:string,createdAt:number,evidence:string)=>db.prepare(`INSERT INTO marketing_engagement_events(id,candidate_id,version,event_type,channel,evidence,actor_id,created_at)
    VALUES(?,?,?,?,?,?,?,?)`).bind(id,candidateId,version,eventType,channel,evidence,f.owner.userId,createdAt).run();
  const submitA=key(),openA=key(),submitB=key(),handoffB=key(),submitC=key(),oldEvent=key();
  await event(submitA,candidateA,1,"form_submit","web",Date.parse("2026-03-05T08:00:00Z"),"private submitted form evidence");
  await event(openA,candidateA,2,"email_open","email",Date.parse("2026-03-08T08:00:00Z"),"private provider event evidence");
  await event(submitB,candidateB,1,"form_submit","web",Date.parse("2026-03-11T08:00:00Z"),"private handoff form evidence");
  await event(handoffB,candidateB,2,"crm_handoff","internal",Date.parse("2026-03-12T08:00:00Z"),"private handoff evidence");
  await event(submitC,candidateC,1,"form_submit","web",Date.parse("2026-03-02T08:00:00Z"),"private rejected form evidence");
  await event(oldEvent,candidateD,1,"form_submit","web",Date.parse("2026-02-28T08:00:00Z"),"private out-of-period evidence");

  const actor={...f.owner,modules:[...new Set([...f.owner.modules,"marketing"])]},scoped={...actor,branchId:f.branch,departmentId:f.department};
  const filters={from:"2026-03-01",to:"2026-03-31",currency:"THB"},withoutMarketing={...actor,modules:actor.modules.filter(module=>module!=="marketing")},withoutRead={...actor,permissions:[]};
  const candidates=await queryMetric(db,actor,"marketing-lead-candidates",filters),candidatesByStatus=await queryMetric(db,actor,"marketing-lead-candidates",filters,"status");
  const scopedCandidates=await queryMetric(db,scoped,"marketing-lead-candidates",filters);
  const engagements=await queryMetric(db,actor,"marketing-engagement-events",filters),engagementsByMonth=await queryMetric(db,actor,"marketing-engagement-events",filters,"month");
  const scopedEngagements=await queryMetric(db,scoped,"marketing-engagement-events",filters);
  expect(candidates.total).toBe("4");expect(candidatesByStatus.rows).toEqual([{label:"handed_off",value:"1"},{label:"pending",value:"2"},{label:"rejected",value:"1"}]);
  expect(scopedCandidates.total).toBe("3");expect(engagements.total).toBe("5");expect(engagementsByMonth.rows).toEqual([{label:"2026-03",value:"5"}]);expect(scopedEngagements.total).toBe("3");

  const first=await searchExecutiveEvidence(db,actor,{metric:"marketing-lead-candidates",filters,query:"Executive Marketing prospect",limit:1});
  expect(first.provider).toBe("marketing-candidate-engagement");expect(first.items.map(item=>item.id)).toEqual([candidateA]);expect(first.hasMore).toBe(true);
  const second=await searchExecutiveEvidence(db,actor,{metric:"marketing-lead-candidates",filters,query:"Executive Marketing prospect",limit:1,cursor:first.nextCursor!});
  expect(second.items.map(item=>item.id)).toEqual([candidateB]);
  const candidateDetail=await readExecutiveEvidence(db,actor,"marketing-lead-candidates",candidateA,filters);
  expect(candidateDetail.record).toMatchObject({contact_name:"Executive Marketing prospect A",form_name:"Executive Marketing English test form",status:"pending",version:2,form_version:2});
  expect(candidateDetail.record).not.toHaveProperty("contact_detail");expect(candidateDetail.record).not.toHaveProperty("consent_evidence");
  expect(candidateDetail.record).not.toHaveProperty("attribution_json");expect(candidateDetail.record).not.toHaveProperty("brief");
  expect((await searchExecutiveEvidence(db,scoped,{metric:"marketing-lead-candidates",filters,query:"prospect B"})).items).toEqual([]);
  await expect(readExecutiveEvidence(db,scoped,"marketing-lead-candidates",candidateB,filters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});

  const eventResults=await searchExecutiveEvidence(db,actor,{metric:"marketing-engagement-events",filters,limit:2});
  expect(eventResults.items).toHaveLength(2);expect(eventResults.hasMore).toBe(true);
  const eventResultsSecond=await searchExecutiveEvidence(db,actor,{metric:"marketing-engagement-events",filters,limit:2,cursor:eventResults.nextCursor!});
  const eventResultsThird=await searchExecutiveEvidence(db,actor,{metric:"marketing-engagement-events",filters,limit:2,cursor:eventResultsSecond.nextCursor!});
  expect(eventResultsSecond.items).toHaveLength(2);expect(eventResultsSecond.hasMore).toBe(true);expect(eventResultsThird.items).toHaveLength(1);expect(eventResultsThird.hasMore).toBe(false);
  expect(new Set([...eventResults.items,...eventResultsSecond.items,...eventResultsThird.items].map(item=>item.id)).size).toBe(5);
  expect((await searchExecutiveEvidence(db,actor,{metric:"marketing-engagement-events",filters,query:"email_open",limit:25})).items.map(item=>item.id)).toEqual([openA]);
  expect((await searchExecutiveEvidence(db,actor,{metric:"marketing-engagement-events",filters:{...filters,from:"2026-03-06"},query:"email_open",limit:25})).items.map(item=>item.id)).toEqual([openA]);
  const eventDetail=await readExecutiveEvidence(db,actor,"marketing-engagement-events",openA,filters);
  expect(eventDetail.record).toMatchObject({candidate_id:candidateA,event_type:"email_open",channel:"email",version:2,candidate_version:2});
  expect(eventDetail.record).not.toHaveProperty("evidence");expect(eventDetail.record).not.toHaveProperty("actor_id");
  await expect(readExecutiveEvidence(db,actor,"marketing-engagement-events",submitA,{...filters,from:"2026-03-06"})).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});
  await expect(searchExecutiveEvidence(db,withoutMarketing,{metric:"marketing-lead-candidates",filters})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,withoutRead,{metric:"marketing-engagement-events",filters})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  const marketingAudit=(await db.prepare("SELECT provider_id,metric_id,action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events WHERE company_id=? AND provider_id=? ORDER BY created_at,id")
    .bind(f.company,"marketing-candidate-engagement").all<{provider_id:string;metric_id:string;action:string;record_id:string|null;record_version:number|null;fields:string}>()).results;
  expect(marketingAudit.some(row=>row.metric_id==="marketing-engagement-events"&&row.action==="open"&&row.record_id===openA&&row.record_version===2)).toBe(true);
  expect(marketingAudit.every(row=>JSON.parse(row.fields).every((field:string)=>!/(contact_detail|consent_evidence|attribution_json|brief|evidence|actor_id)/i.test(field)))).toBe(true);
  expect(JSON.stringify(marketingAudit)).not.toContain("private submitted form evidence");expect(JSON.stringify(marketingAudit)).not.toContain("private-contact@example.test");
});
it("reconciles open purchase receipts and latest unresolved supplier invoice match exceptions",async()=>{
  const now=Date.now(),supplier=key(),product=key(),openOrder=key(),receivedOrder=key(),warehouse=key();
  await db.prepare("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status) VALUES(?,?,?,?,?,'active')").bind(warehouse,f.company,f.branch,`EX-${warehouse.slice(0,6)}`,"Executive procurement test warehouse").run();
  await db.prepare("INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status) VALUES(?,?,?,?,?,10000,700,'active')").bind(product,f.company,`EX-${product.slice(0,6)}`,"Executive procurement test product","each").run();
  await db.prepare(`INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,credit_limit_minor,status,version,created_at,updated_at)
    VALUES(?,?,'supplier',?,?,'THB',30,0,'active',1,?,?)`).bind(supplier,f.company,`EX-${supplier.slice(0,8)}`,"Executive procurement test supplier",now,now).run();
  const order=async(id:string,number:string,state:string)=>db.prepare(`INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,state,subtotal_minor,tax_minor,created_by,approved_by,created_at,updated_at)
    VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`).bind(id,f.company,f.branch,f.department,"purchase",number,supplier,warehouse,"THB","2026-03-01","2026-03-20",state,10000,700,f.owner.userId,f.owner.userId,now,now).run();
  await order(openOrder,"EX-PO-OPEN","partially_fulfilled");await order(receivedOrder,"EX-PO-RECEIVED","fulfilled");
  const line=async(orderId:string,fulfilled:number)=>db.prepare(`INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal)
    VALUES(?,?,?, ?,1000,?,10000,700,10000,700,0)`).bind(key(),orderId,product,"Executive procurement test line",fulfilled).run();
  await line(openOrder,400);await line(receivedOrder,1000);
  const fulfillments=[key(),key()];
  await db.prepare("INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at,version) VALUES(?,?,?,?,?,?,1)").bind(fulfillments[0],openOrder,"EX-RECEIPT-OPEN","2026-03-05",f.owner.userId,now).run();
  await db.prepare("INSERT INTO commerce_fulfillments(id,order_id,number,fulfillment_date,created_by,created_at,version) VALUES(?,?,?,?,?,?,1)").bind(fulfillments[1],receivedOrder,"EX-RECEIPT-RECEIVED","2026-03-06",f.owner.userId,now).run();
  const match=async(id:string,fulfillmentId:string,status:"returned"|"review",quantityMatches:boolean,createdAt:number)=>db.prepare(`INSERT INTO procurement_bill_matches(
      id,company_id,branch_id,department_id,order_id,fulfillment_id,supplier_id,currency,chart_id,clearing_account_id,payable_account_id,tax_account_id,reference,invoice_date,due_date,evidence,expected_subtotal_minor,subtotal_minor,tax_minor,quantity_matches,variance_account_id,reviewed_by,review_reason,invoice_id,status,version,created_by,created_at,updated_at)
    VALUES(?,?,?,?,?,?,?,'THB','test-chart','test-clearing','test-payable','test-tax',?,'2026-03-10','2026-03-31','English test evidence',10000,10000,700,?,NULL,NULL,NULL,NULL,?,1,?,?,?)`).bind(id,f.company,f.branch,f.department,fulfillmentId===fulfillments[0]?openOrder:receivedOrder,fulfillmentId,supplier,`EX-BILL-${id.slice(0,8)}`,quantityMatches,status,f.owner.userId,createdAt,createdAt).run();
  const returnedMatch=key();await match(returnedMatch,fulfillments[0],"returned",false,now-1000);
  const latestMatch=key();await match(latestMatch,fulfillments[0],"review",false,now);
  const unresolvedReturn=key();await match(unresolvedReturn,fulfillments[1],"returned",true,now-500);
  await db.prepare("INSERT INTO procurement_bill_match_receipts(id,match_id,fulfillment_id,captured_version,active) VALUES(?,?,?,1,FALSE)").bind(`${returnedMatch}-receipt-0`,returnedMatch,fulfillments[0]).run();
  await db.prepare("INSERT INTO procurement_bill_match_receipts(id,match_id,fulfillment_id,captured_version,active) VALUES(?,?,?,1,TRUE)").bind(`${latestMatch}-receipt-0`,latestMatch,fulfillments[0]).run();
  await db.prepare("INSERT INTO procurement_bill_match_receipts(id,match_id,fulfillment_id,captured_version,active) VALUES(?,?,?,1,FALSE)").bind(`${unresolvedReturn}-receipt-0`,unresolvedReturn,fulfillments[1]).run();
  const actor={...f.owner,modules:[...new Set([...f.owner.modules,"procurement"])]};
  const filters={from:"2026-03-01",to:"2026-03-31",currency:"THB"};
  const withoutProcurement={...actor,modules:actor.modules.filter(module=>module!=="procurement")};
  expect((await metricCatalogFor(db,withoutProcurement)).find(metric=>metric.id==="open-po-receipts")?.available).toBe(false);
  await expect(queryMetric(db,withoutProcurement,"open-po-receipts",filters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,withoutProcurement,{metric:"invoice-match-exceptions",filters})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  const outstanding=await queryMetric(db,actor,"open-po-receipts",filters);
  const receiptsByStatus=await queryMetric(db,actor,"open-po-receipts",filters,"status");
  const exceptions=await queryMetric(db,actor,"invoice-match-exceptions",filters);
  const exceptionsByType=await queryMetric(db,actor,"invoice-match-exceptions",filters,"status");
  expect(outstanding.total).toBe("1");expect(receiptsByStatus.rows).toEqual([{label:"partially_fulfilled",value:"1"}]);
  expect(exceptions.total).toBe("2");expect(exceptionsByType.rows).toEqual([{label:"Quantity variance",value:"1"},{label:"Returned for correction",value:"1"}]);
  const evidence=await searchExecutiveEvidence(db,actor,{metric:"invoice-match-exceptions",filters,query:`EX-BILL-${latestMatch.slice(0,8)}`,limit:25});
  expect(evidence.provider).toBe("procurement-supplier-bills");expect(evidence.items.map(item=>item.id)).toEqual([latestMatch]);
  const detail=await readExecutiveEvidence(db,actor,"invoice-match-exceptions",latestMatch,filters);
  expect(detail.provider).toBe("procurement-supplier-bills");expect(detail.record).toMatchObject({order_number:"EX-PO-OPEN",quantity_matches:false,version:1});
  expect(JSON.stringify(detail)).not.toContain("English test evidence");
  const audit=(await db.prepare("SELECT provider_id,metric_id,action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events WHERE company_id=? AND provider_id=? ORDER BY created_at,id").bind(f.company,"procurement-supplier-bills").all<{provider_id:string;metric_id:string;action:string;record_id:string|null;record_version:number|null;fields:string}>()).results;
  expect(audit.map(row=>row.action).sort()).toEqual(["open","search"]);expect(audit.every(row=>row.metric_id==="invoice-match-exceptions")).toBe(true);
  expect(JSON.parse(audit.find(row=>row.action==="open")!.fields)).toContain("quantity_matches");expect(audit.find(row=>row.action==="open")?.record_id).toBe(latestMatch);
});
it("reconciles terminal sales cancellations and scopes evidence to the reader's current department",async()=>{
  const now=Date.parse("2026-03-15T12:00:00Z"),warehouse=key(),customer=key();
  await db.prepare("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status) VALUES(?,?,?,?,?,'active')").bind(warehouse,f.company,f.branch,`EX-${warehouse.slice(0,6)}`,"Executive sales test warehouse").run();
  await db.prepare(`INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,credit_limit_minor,status,version,created_at,updated_at)
    VALUES(?,?,'customer',?,?,'THB',30,0,'active',1,?,?)`).bind(customer,f.company,`EX-${customer.slice(0,8)}`,"Executive sales test customer",now,now).run();
  const insert=async(id:string,label:string,department:string|null,approvedBy:string|null,subtotal:number,updatedAt:number)=>db.prepare(`INSERT INTO commerce_orders(
    id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,state,subtotal_minor,tax_minor,created_by,approved_by,created_at,updated_at)
    VALUES(?,?,?,?, 'sales', ?,?,?, 'THB','2026-03-01','2026-03-31','cancelled',?,?, ?,?,?,?)`).bind(id,f.company,f.branch,department,`EX-CANCEL-${label}`,customer,warehouse,subtotal,Math.round(subtotal*0.07),f.owner.userId,approvedBy,now,updatedAt).run();
  const approved=key(),draft=key(),otherDepartment=key(),outsidePeriod=key();
  await insert(approved,"APPROVED",f.department,f.owner.userId,10000,Date.parse("2026-03-10T09:00:00Z"));
  await insert(draft,"DRAFT",f.department,null,90000,Date.parse("2026-03-11T09:00:00Z"));
  await insert(otherDepartment,"OTHER-DEPARTMENT",null,f.owner.userId,5000,Date.parse("2026-03-12T09:00:00Z"));
  await insert(outsidePeriod,"OUTSIDE-PERIOD",f.department,f.owner.userId,3000,Date.parse("2026-04-02T09:00:00Z"));
  const actor={...f.owner,modules:["sales"]},scoped={...actor,branchId:f.branch,departmentId:f.department};
  const withoutSales={...actor,modules:[]},filters={from:"2026-03-01",to:"2026-03-31",currency:"THB"};
  await expect(queryMetric(db,withoutSales,"sales-cancellations",filters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,withoutSales,{metric:"sales-cancellations",filters})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  const count=await queryMetric(db,actor,"sales-cancellations",filters),scopedCount=await queryMetric(db,scoped,"sales-cancellations",filters),value=await queryMetric(db,scoped,"sales-cancellation-value",filters),valueByMonth=await queryMetric(db,scoped,"sales-cancellation-value",filters,"month");
  expect(count.total).toBe("3");expect(scopedCount.total).toBe("2");expect(scopedCount.rows).toEqual([{label:"Cancelled sales orders",value:"2"}]);
  expect(value.total).toBe("10700");expect(valueByMonth.rows).toEqual([{label:"2026-03",value:"10700"}]);
  const found=await searchExecutiveEvidence(db,scoped,{metric:"sales-cancellations",filters,limit:1});
  expect(found.provider).toBe("commerce-sales-orders");expect(found.items.map(item=>item.id)).toEqual([draft]);expect(found.items[0]).not.toHaveProperty("amount");expect(found.hasMore).toBe(true);expect(found.nextCursor).toBeTruthy();
  const next=await searchExecutiveEvidence(db,scoped,{metric:"sales-cancellations",filters,limit:1,cursor:found.nextCursor!});expect(next.items.map(item=>item.id)).toEqual([approved]);expect(next.hasMore).toBe(false);
  const valueEvidence=await searchExecutiveEvidence(db,scoped,{metric:"sales-cancellation-value",filters,limit:25});expect(valueEvidence.items.map(item=>item.id)).toEqual([approved]);expect(valueEvidence.items[0]).toMatchObject({amount:"10700"});
  const draftDetail=await readExecutiveEvidence(db,scoped,"sales-cancellations",draft,filters);
  expect(draftDetail.provider).toBe("commerce-sales-orders");expect(draftDetail.record).toMatchObject({status:"cancelled",prior_approval:false,subtotal_minor:"90000"});
  await expect(readExecutiveEvidence(db,scoped,"sales-cancellation-value",draft,filters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});
  const approvedDetail=await readExecutiveEvidence(db,scoped,"sales-cancellation-value",approved,filters);
  expect(approvedDetail.record).toMatchObject({number:"EX-CANCEL-APPROVED",prior_approval:true,total_minor:"10700"});
  const audit=(await db.prepare("SELECT provider_id,metric_id,action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events WHERE company_id=? AND provider_id=? ORDER BY created_at,id").bind(f.company,"commerce-sales-orders").all<{provider_id:string;metric_id:string;action:string;record_id:string|null;record_version:number|null;fields:string}>()).results;
  expect(audit.map(row=>row.action).sort()).toEqual(["open","open","search","search","search"]);expect(audit.every(row=>row.provider_id==="commerce-sales-orders"&&!!row.record_version)).toBe(true);
  expect(audit.some(row=>row.metric_id==="sales-cancellation-value"&&row.action==="open")).toBe(true);
});
it("reconciles fully fulfilled sales orders by order cohort and scopes bounded evidence",async()=>{
  const now=Date.parse("2026-03-20T12:00:00Z"),warehouse=key(),customer=key(),product=key(),secondProduct=key();
  await db.prepare("INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status) VALUES(?,?,?,?,?,'active')").bind(warehouse,f.company,f.branch,`EX-${warehouse.slice(0,6)}`,"Executive fulfillment test warehouse").run();
  await db.prepare(`INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,credit_limit_minor,status,version,created_at,updated_at)
    VALUES(?,?,'customer',?,?,'THB',30,0,'active',1,?,?)`).bind(customer,f.company,`EX-${customer.slice(0,8)}`,"Executive fulfillment test customer",now,now).run();
  await db.prepare("INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status) VALUES(?,?,?,?,?,10000,700,'active')").bind(product,f.company,`EX-${product.slice(0,8)}`,"Executive fulfillment test product","each").run();
  await db.prepare("INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status) VALUES(?,?,?,?,?,10000,700,'active')").bind(secondProduct,f.company,`EX-${secondProduct.slice(0,8)}`,"Executive fulfillment second test product","case").run();
  const insertOrder=async(id:string,number:string,state:string,department:string|null,orderDate:string,withLines=true)=>{
    await db.prepare(`INSERT INTO commerce_orders(
      id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,state,subtotal_minor,tax_minor,created_by,approved_by,created_at,updated_at)
      VALUES(?,?,?,?,'sales',?,?,?,'THB',?,?,?,10000,700,?,?,?,?)`)
      .bind(id,f.company,f.branch,department,number,customer,warehouse,orderDate,orderDate,state,f.owner.userId,f.owner.userId,now,now).run();
    if(withLines) {
      const quantity=state==="fulfilled"?1000:state==="partially_fulfilled"?500:0;
      await db.prepare(`INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal)
        VALUES(?,?,?,'English executive fulfillment test line',1000,?,10000,700,10000,700,0)`)
        .bind(key(),id,product,quantity).run();
      if(number==="EX-FULFILLED-DEPARTMENT") await db.prepare(`INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal)
        VALUES(?,?,?,'Second English test line with another unit',1000,1000,10000,700,10000,700,1)`)
        .bind(key(),id,secondProduct).run();
    }
  };
  const fulfilledDepartment=key(),confirmedDepartment=key(),partialDepartment=key(),fulfilledCompany=key(),cancelled=key(),withoutLines=key(),outsidePeriod=key();
  await insertOrder(fulfilledDepartment,"EX-FULFILLED-DEPARTMENT","fulfilled",f.department,"2026-03-05");
  await insertOrder(confirmedDepartment,"EX-CONFIRMED-DEPARTMENT","confirmed",f.department,"2026-03-10");
  await insertOrder(partialDepartment,"EX-PARTIAL-DEPARTMENT","partially_fulfilled",f.department,"2026-03-15");
  await insertOrder(fulfilledCompany,"EX-FULFILLED-COMPANY","fulfilled",null,"2026-03-18");
  await insertOrder(cancelled,"EX-CANCELLED","cancelled",f.department,"2026-03-19");
  await insertOrder(withoutLines,"EX-NO-LINES","fulfilled",f.department,"2026-03-20",false);
  await insertOrder(outsidePeriod,"EX-OUTSIDE-PERIOD","fulfilled",f.department,"2026-04-01");
  const actor={...f.owner,modules:[...new Set([...f.owner.modules,"sales"])]};
  const scoped={...actor,branchId:f.branch,departmentId:f.department};
  const withoutSales={...actor,modules:actor.modules.filter(module=>module!=="sales")};
  const filters={from:"2026-03-01",to:"2026-03-31",currency:"THB"};
  await expect(queryMetric(db,withoutSales,"sales-fulfillment-rate",filters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,withoutSales,{metric:"sales-fulfillment-rate",filters})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  const total=await queryMetric(db,actor,"sales-fulfillment-rate",filters);
  const byMonth=await queryMetric(db,actor,"sales-fulfillment-rate",filters,"month");
  const byStatus=await queryMetric(db,actor,"sales-fulfillment-rate",filters,"status");
  const departmentTotal=await queryMetric(db,scoped,"sales-fulfillment-rate",filters);
  const salesOrderValue=await queryMetric(db,actor,"sales-orders",filters);
  const salesOrderStatus=await queryMetric(db,actor,"sales-orders",filters,"status");
  const departmentSalesOrderValue=await queryMetric(db,scoped,"sales-orders",filters);
  expect(total.total).toBe("5000");expect(total.rows).toEqual([{label:"Eligible sales orders",value:"5000"}]);
  expect(byMonth.rows).toEqual([{label:"2026-03",value:"5000"}]);
  expect(byStatus.rows).toEqual([{label:"confirmed",value:"0"},{label:"fulfilled",value:"10000"},{label:"partially_fulfilled",value:"0"}]);
  expect(departmentTotal.total).toBe("3333");
  expect(salesOrderValue.total).toBe("53500");
  expect(salesOrderStatus.rows).toEqual([{label:"confirmed",value:"10700"},{label:"fulfilled",value:"32100"},{label:"partially_fulfilled",value:"10700"}]);
  expect(departmentSalesOrderValue.total).toBe("42800");
  const salesOrders=await searchExecutiveEvidence(db,actor,{metric:"sales-orders",filters,limit:2});
  expect(salesOrders.provider).toBe("commerce-sales-orders");
  expect(salesOrders.items.map(item=>item.id)).toEqual([withoutLines,fulfilledCompany]);
  expect(salesOrders.items.every(item=>stringField(item,'amount')==="10700"&&item.summary.includes("Sales order"))).toBe(true);
  expect(salesOrders.items[0]).not.toHaveProperty("due_date");
  expect(salesOrders.items[0]).not.toHaveProperty("outstanding_minor");
  expect(salesOrders.hasMore).toBe(true);
  const nextSalesOrders=await searchExecutiveEvidence(db,actor,{metric:"sales-orders",filters,limit:2,cursor:salesOrders.nextCursor!});
  expect(nextSalesOrders.items.map(item=>item.id)).toEqual([partialDepartment,confirmedDepartment]);
  const matchingSalesOrder=await searchExecutiveEvidence(db,scoped,{metric:"sales-orders",filters,query:"EX-FULFILLED-DEPARTMENT"});
  expect(matchingSalesOrder.items.map(item=>item.id)).toEqual([fulfilledDepartment]);
  const salesOrderDetail=await readExecutiveEvidence(db,scoped,"sales-orders",fulfilledDepartment,filters);
  expect(salesOrderDetail.record).toMatchObject({id:fulfilledDepartment,number:"EX-FULFILLED-DEPARTMENT",status:"fulfilled",order_date:"2026-03-05",total_minor:"10700"});
  expect(salesOrderDetail.record).not.toHaveProperty("prior_approval");
  expect(salesOrderDetail.fields).toEqual(["number","order_date","currency","subtotal_minor","tax_minor","total_minor","status","version"]);
  await expect(readExecutiveEvidence(db,scoped,"sales-orders",fulfilledCompany,filters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});
  await expect(readExecutiveEvidence(db,actor,"sales-orders",cancelled,filters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});
  await expect(searchExecutiveEvidence(db,f.claimant,{metric:"sales-orders",filters})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  const salesAudit=(await db.prepare("SELECT action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events WHERE company_id=? AND provider_id=? AND metric_id=? AND record_id IN (?,?,?,?,?) ORDER BY created_at,id").bind(f.company,"commerce-sales-orders","sales-orders",withoutLines,fulfilledCompany,partialDepartment,confirmedDepartment,fulfilledDepartment).all<{action:string;record_id:string|null;record_version:number|null;fields:string}>()).results;
  expect(salesAudit.map(row=>row.action).sort()).toEqual(["open","search","search","search","search","search"]);
  expect(salesAudit.every(row=>!!row.record_id&&row.record_version===1&&JSON.parse(row.fields).every((field:string)=>["number","order_date","currency","subtotal_minor","tax_minor","total_minor","status","version"].includes(field)))).toBe(true);
  const first=await searchExecutiveEvidence(db,actor,{metric:"sales-fulfillment-rate",filters,limit:2});
  expect(first.provider).toBe("commerce-sales-orders");expect(first.items.map(item=>item.id)).toEqual([fulfilledCompany,partialDepartment]);
  expect(first.hasMore).toBe(true);expect(first.nextCursor).toBeTruthy();
  const second=await searchExecutiveEvidence(db,actor,{metric:"sales-fulfillment-rate",filters,limit:2,cursor:first.nextCursor!});
  expect(second.items.map(item=>item.id)).toEqual([confirmedDepartment,fulfilledDepartment]);expect(second.hasMore).toBe(false);
  const match=await searchExecutiveEvidence(db,scoped,{metric:"sales-fulfillment-rate",filters,query:"EX-FULFILLED-DEPARTMENT"});
  expect(match.items.map(item=>item.id)).toEqual([fulfilledDepartment]);
  const detail=await readExecutiveEvidence(db,scoped,"sales-fulfillment-rate",fulfilledDepartment,filters);
  expect(detail.record).toMatchObject({id:fulfilledDepartment,number:"EX-FULFILLED-DEPARTMENT",status:"fulfilled",order_date:"2026-03-05",version:1});
  expect(detail.fields).toEqual(["number","order_date","status","version"]);
  await expect(readExecutiveEvidence(db,scoped,"sales-fulfillment-rate",fulfilledCompany,filters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});
  await expect(readExecutiveEvidence(db,actor,"sales-fulfillment-rate",cancelled,filters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});
  await expect(readExecutiveEvidence(db,actor,"sales-fulfillment-rate",outsidePeriod,filters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});
  await expect(readExecutiveEvidence(db,actor,"sales-fulfillment-rate",withoutLines,filters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});
  const audit=(await db.prepare("SELECT metric_id,action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events WHERE company_id=? AND provider_id=? AND metric_id=? ORDER BY created_at,id").bind(f.company,"commerce-sales-orders","sales-fulfillment-rate").all<{metric_id:string;action:string;record_id:string|null;record_version:number|null;fields:string}>()).results;
  expect(audit.map(row=>row.action).sort()).toEqual(["open","search","search","search","search","search"]);
  expect(audit.some(row=>row.action==="open"&&row.record_id===fulfilledDepartment&&row.record_version===1)).toBe(true);
  expect(audit.every(row=>JSON.parse(row.fields).every((field:string)=>["number","order_date","status","version"].includes(field)))).toBe(true);
});
it("does not let a distribution recipient expand filters or retain an expired grant",async()=>{
  const id=await report();const s=await saveShare(db,f.owner,id,shareSchema.parse({recipientId:f.claimant.userId,mode:"distribution",widgetIds:["people"],expiresAt:Date.now()+60000,filters:defaultFilters()}),key());
  const before=await reportData(db,f.claimant,id);const after=await reportData(db,f.claimant,id,{from:"2025-01-01",to:"2025-12-31",currency:"USD"});
  expect(after.metrics.headcount.revision).toBe(before.metrics.headcount.revision);
  await db.prepare("UPDATE executive_report_shares SET expires_at=? WHERE id=?").bind(Date.now()-1000,s.id).run();
  await expect(reportView(db,f.claimant,id)).rejects.toMatchObject({code:"REPORT_NOT_FOUND"});
});
it("discards a provider result if access is revoked during generation",async()=>{
  const id=await report(),k=key();await queueGeneration(db,f.owner,aiEnv,id,generateSchema.parse({question:"Prepare this report"}),k);
  await drainExecutiveGeneration(db,aiEnv,async()=>{await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(),f.owner.assignmentId).run();return {result:{message:"Review the design.",definition:emptyDefinition(),followUps:[]},inputTokens:1,outputTokens:1};});
  expect(required((await db.prepare("SELECT state FROM executive_generation_jobs WHERE id=?").bind(k).first<{state:string}>()))?.state).toBe("failed");
  expect(required((await db.prepare("SELECT version FROM executive_reports WHERE id=?").bind(id).first<{version:number}>()))?.version).toBe(2);
});
it("fails an interrupted lease without silently issuing another paid inference",async()=>{
  const id=await report(),k=key();await queueGeneration(db,f.owner,aiEnv,id,generateSchema.parse({question:"Prepare this report"}),k);
  await db.prepare("UPDATE executive_generation_jobs SET state='running',lease_until=? WHERE id=?").bind(Date.now()-1000,k).run();
  const generate=vi.fn();await drainExecutiveGeneration(db,aiEnv,generate);expect(generate).not.toHaveBeenCalled();expect((await generationHistory(db,f.owner,id))[0].state).toBe("failed");
});
it("allows repeated bounded editor changes without advancing other recipients",async()=>{
  const editor=await f.member("executive-editor","holding_owner");
  await db.prepare("UPDATE hr_engagements SET level_id=? WHERE person_id=?").bind(`org-level-${f.company}-2`,`person-${editor.userId}`).run();
  const id=await report();
  await saveShare(db,f.owner,id,shareSchema.parse({recipientId:editor.userId,role:"editor",mode:"recipient",widgetIds:["people"],expiresAt:null,filters:defaultFilters()}),key());
  await saveShare(db,f.owner,id,shareSchema.parse({recipientId:f.claimant.userId,mode:"distribution",widgetIds:["people"],expiresAt:null,filters:defaultFilters()}),key());
  for(const title of ["First design","Second design"]){
    const r=await reportView(db,editor,id);
    await saveReport(db,editor,id,saveSchema.parse({expectedVersion:r.version,definition:{...r.definition,title},filters:r.filters}),key());
    expect((await reportView(db,editor,id)).definition.title).toBe(title);
  }
  expect((await reportView(db,f.claimant,id)).definition.title).toBe("Operating overview");
  const r=await reportView(db,editor,id);
  await expect(saveReport(db,editor,id,saveSchema.parse({expectedVersion:r.version,definition:r.definition,filters:{...r.filters,currency:"USD"}}),key())).rejects.toMatchObject({code:"SHARE_BOUNDS"});
});
it("removes C-level author eligibility after an effective level change",async()=>{
  const author=await f.member("reviewed-executive","executive_author");
  await db.prepare("UPDATE hr_engagements SET level_id=? WHERE person_id=?").bind(`org-level-${f.company}-2`,`person-${author.userId}`).run();
  expect(await canAuthor(db,author)).toBe(true);
  await db.prepare("UPDATE hr_engagements SET level_id=? WHERE person_id=?").bind(`org-level-${f.company}-3`,`person-${author.userId}`).run();
  await expect(createReport(db,author,key())).rejects.toMatchObject({code:"EXECUTIVE_AUTHOR_REQUIRED"});
});
it("rejects authority when its scoped organizational node is inactive",async()=>{
  const assignmentId=await f.grant(f.claimant,"employee","branch",f.branch);
  const actor={...f.claimant,assignmentId,branchId:f.branch};
  await currentAuthority(db,actor);
  await db.prepare("UPDATE organization_nodes SET status='suspended' WHERE id=?").bind(f.branch).run();
  await expect(currentAuthority(db,actor)).rejects.toMatchObject({code:"EXECUTIVE_ACCESS_CHANGED"});
});
it("does not misclassify a transient database failure as revoked access",async()=>{
  const transient=Object.assign(new Error("transaction retry"),{code:"40001"});
  const read=vi.spyOn(db,"prepare").mockImplementationOnce(()=>{throw transient;});
  try{await expect(currentAuthority(db,f.owner)).rejects.toBe(transient);}finally{read.mockRestore();}
});

it("sends exact authorized data and structure to the model while excluding forbidden modules",async()=>{
  const {executiveAiContext}=await import("../src/server/executive/context");
  const owner=await executiveAiContext(db,f.owner,defaultFilters());
  expect(owner.catalog.some(m=>m.id==="headcount")).toBe(true);
  expect(BigInt(owner.snapshots.find(m=>m.metric==="headcount"&&m.breakdown==="default")?.total??"0")).toBeGreaterThan(BigInt(0));
  expect(owner.organization).toMatchObject({available:true});
  const staff=await executiveAiContext(db,f.claimant,defaultFilters());
  expect(staff.organization).toMatchObject({available:false});
  expect(staff.snapshots.some(m=>["headcount","payables","receivables"].includes(m.metric))).toBe(false);
  expect(JSON.stringify(staff)).not.toContain('"positions":');
});
it("keeps scoped AI context inside its branch and does not send company finance totals",async()=>{
  const {executiveAiContext}=await import("../src/server/executive/context");
  const foreignBranch=key();
  await db.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) VALUES(?,'branch',?,'Restricted other branch',?,?,?,'active')").bind(foreignBranch,foreignBranch,f.company,f.company,foreignBranch).run();
  const context=await executiveAiContext(db,{...f.owner,branchId:f.branch},defaultFilters());
  expect(JSON.stringify(context.organization)).not.toContain("Restricted other branch");
  expect(context.snapshots.some(m=>m.metric==="receivables")).toBe(false);
  expect(context.scope.branchId).toBe(f.branch);
});
it("uses a single effective engagement per person for additive organization charts",async()=>{
  const before=await queryMetric(db,f.owner,"headcount",defaultFilters(),"branch");
  await db.prepare("INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at,level_id) SELECT ?,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,'2026-01-01',job_title,currency,created_by,created_at,updated_at,level_id FROM hr_engagements WHERE person_id=? LIMIT 1").bind(key(),`person-${f.claimant.userId}`).run();
  const after=await queryMetric(db,f.owner,"headcount",defaultFilters(),"branch");
  expect(after.total).toBe(before.total);expect(after.rows.reduce((n,r)=>n+BigInt(r.value),BigInt(0))).toBe(BigInt(required(after.total)));
  await expect(queryMetric(db,f.owner,"headcount",defaultFilters(),"month")).rejects.toMatchObject({code:"BREAKDOWN_UNAVAILABLE"});
});
it("persists custom HTML/CSS and dimensions and binds it to authorized live values",async()=>{
  const id=await report();const r=await reportView(db,f.owner,id);
  const definition=definitionSchema.parse({...r.definition,widgets:[{...r.definition.widgets[0],kind:"html",breakdown:"branch",columns:9,height:480,html:"<section><h1>{{title}}</h1><strong>{{total}}</strong>{{chart:donut}}</section>",css:"section { background: #123; border-radius: 22px; }"}]});
  await saveReport(db,f.owner,id,saveSchema.parse({expectedVersion:r.version,definition,filters:r.filters}),key());
  const saved=await reportView(db,f.owner,id);expect(saved.definition.widgets[0]).toMatchObject({columns:9,height:480,kind:"html"});
  const d=await reportData(db,f.owner,id);expect(BigInt(required(d.metrics["headcount:branch"].total))).toBeGreaterThan(BigInt(0));expect(d.documents.people).toContain("<svg");expect(d.documents.people).not.toContain("{{total}}");expect(d.documents.people).toContain("border-radius: 22px");
});
it("isolates generated markup and blocks script, navigation and external fetches",async()=>{
  const {htmlWidgetDocument}=await import("../src/server/executive/html-widget");
  const widget=definitionSchema.parse({...emptyDefinition(),widgets:[{id:"x",title:"Test",metric:"headcount",kind:"html",html:'<script>alert(1)</script><img src="https://evil.test" onerror="alert(1)"><a href="https://evil.test">Go</a><iframe src="https://evil.test"></iframe><meta http-equiv="refresh" content="0;url=https://evil.test"><div>{{rows}}</div>',css:'body{color:red}</style><script>alert(2)</script>'}]}).widgets[0];
  const doc=htmlWidgetDocument(widget,htmlMetric({rows:[{label:'<img src=x onerror=alert(3)>',value:'1'}],definition:"Test",scope:"Company",revision:"x",updatedAt:0}));
  expect(doc).not.toMatch(/<script|<iframe|<img|<[^>]+(?:href|onerror)=/i);
  expect(doc).toContain("default-src 'none'");expect(doc).toContain("connect-src 'none'");expect(doc).toContain("&lt;img");
});
it("keeps custom iframe content stable on heartbeat but updates changed data and clears denied reads",async()=>{
  const {htmlWidgetDocument}=await import("../src/server/executive/html-widget");
  const widget=definitionSchema.parse({...emptyDefinition(),widgets:[{id:"stable",title:"Live",metric:"headcount",kind:"html",html:"<h2>{{total}}</h2><time>{{updatedAt}}</time>"}]}).widgets[0];
  const data=htmlMetric({definition:"Current people",scope:"Test company",revision:key(),updatedAt:1000});
  const first=htmlWidgetDocument(widget,data);
  expect(htmlWidgetDocument(widget,{...data,updatedAt:2000})).toBe(first);
  expect(htmlWidgetDocument(widget,{...data,total:"2",rows:[{label:"People",value:"2"}],revision:key(),updatedAt:3000})).toContain("<h2>2</h2>");
  expect(htmlWidgetDocument(widget,{...data,error:"Access revoked"})).toBe("");
  const blue=htmlWidgetDocument(widget,data,"blue");
  expect(blue).not.toBe(first);
  expect(blue).toContain("--accent:light-dark(#306ed2,#60a5fa)");
  expect(htmlWidgetDocument(widget,{...data,updatedAt:4000},"blue")).toBe(blue);
});
it("persists adaptive styling and color mode in the published definition without changing data access",async()=>{
  const id=await report(),r=await reportView(db,f.owner,id);
  const definition=definitionSchema.parse({...r.definition,colorMode:"auto",accent:"emerald",widgets:[{...r.definition.widgets[0],kind:"html",html:"<section><h2>{{title}}</h2>{{chart:donut}}</section>",css:"section{background:var(--widget-surface);color:var(--widget-text);border:1px solid var(--widget-border)}"}]});
  await saveReport(db,f.owner,id,saveSchema.parse({expectedVersion:r.version,definition,filters:r.filters}),key());
  await saveShare(db,f.owner,id,shareSchema.parse({recipientId:f.claimant.userId,mode:"distribution",widgetIds:["people"],expiresAt:null,filters:r.filters}),key());
  const saved=await reportView(db,f.owner,id);
  await saveReport(db,f.owner,id,saveSchema.parse({expectedVersion:saved.version,definition:{...saved.definition,colorMode:"dark"},filters:r.filters}),key());
  expect((await reportView(db,f.owner,id)).definition.colorMode).toBe("dark");
  expect((await reportView(db,f.claimant,id)).definition.colorMode).toBe("auto");
  const data=await reportData(db,f.claimant,id),doc=data.documents.people;
  expect(data.metrics.headcount.error).toBeUndefined();
  expect(doc).toContain('name="color-scheme" content="light dark"');
  expect(doc).toContain("--widget-text: light-dark(");
  expect(doc).toContain("--accent:light-dark(#158264,#34d399)");
  expect(doc).toContain('stroke="var(--chart-1)"');
  expect(doc).toContain("background:var(--widget-surface)");
  expect(doc).toContain("script-src 'none'");
});
it("counts visible active Legal schedules and holds and audits bounded metadata-only evidence",async()=>{
  const now=Date.parse("2026-09-20T12:00:00Z"),filters={from:"2026-01-01",to:"2026-12-31",currency:"THB"};
  const insertItem=async(id:string,title:string,kind:"obligation"|"contract"="obligation",classification:"internal"|"confidential"="internal",state="active")=>{
    await db.prepare(`INSERT INTO legal_items(id,company_id,branch_id,department_id,kind,title,description,classification,country,jurisdiction,language,currency,timezone,priority,due_date,owner_id,created_by,state,version,created_at,updated_at)
      VALUES(?,?,?,?,?,?,?,?, 'US','US','en','THB','UTC','normal',NULL,?,?,?,1,?,?)`).bind(id,f.company,f.branch,f.department,kind,title,"English test only",classification,f.owner.userId,f.owner.userId,state,now,now).run();
    if(kind==="obligation")await db.prepare("INSERT INTO legal_obligations(item_id,data) VALUES(?,?::jsonb)").bind(id,JSON.stringify({obligationType:"delivery",requirement:"Private English requirement text"})).run();
  };
  const firstItem=key(),secondItem=key(),closedItem=key(),hiddenItem=key(),firstSchedule=key(),secondSchedule=key(),completedSchedule=key(),hiddenSchedule=key();
  await insertItem(firstItem,"Executive legal schedule source A");await insertItem(secondItem,"Executive legal schedule source B","contract");
  await insertItem(closedItem,"Executive legal schedule closed source","obligation","internal","closed");await insertItem(hiddenItem,"Executive legal schedule confidential source","obligation","confidential");
  const insertSchedule=async(id:string,itemId:string,category:string,dueDate:string|null,state:string)=>db.prepare(`INSERT INTO legal_obligation_schedule(id,item_id,category,title,owner_id,due_date,state,version,data,created_by,updated_by,updated_at)
    VALUES(?,?,?,?,?,?,?,?,?::jsonb,?,?,?)`).bind(id,itemId,category,`Executive legal schedule ${category}`,f.owner.userId,dueDate,state,3,JSON.stringify({scheduleType:category}),f.owner.userId,f.owner.userId,now).run();
  await insertSchedule(firstSchedule,firstItem,"delivery","2026-06-10","pending");await insertSchedule(secondSchedule,secondItem,"payment",null,"in_progress");
  await insertSchedule(completedSchedule,firstItem,"completed","2026-06-11","completed");await insertSchedule(hiddenSchedule,hiddenItem,"confidential","2026-06-12","pending");

  const holdActive=key(),holdRelease=key(),holdReleased=key(),holdHidden=key();
  const insertHold=async(id:string,itemId:string,state:"active"|"release_requested"|"released",createdAt:number)=>db.prepare(`INSERT INTO legal_holds(id,item_id,reason,state,created_by,created_at,release_requested_by,released_by,released_at)
    VALUES(?,?,? ,?,?,?,NULL,NULL,NULL)`).bind(id,itemId,"Private English legal hold reason",state,f.owner.userId,createdAt).run();
  await insertHold(holdActive,firstItem,"active",now-4000);await insertHold(holdRelease,secondItem,"release_requested",now-3000);
  await insertHold(holdReleased,firstItem,"released",now-2000);await insertHold(holdHidden,hiddenItem,"active",now-1000);
  await db.prepare("INSERT INTO legal_hold_targets(hold_id,item_id) VALUES(?,?),(?,?)").bind(holdActive,firstItem,holdActive,secondItem).run();

  const obligations=await queryMetric(db,f.owner,"active-legal-obligations",filters);
  const obligationsByState=await queryMetric(db,f.owner,"active-legal-obligations",filters,"status");
  const obligationsByDueMonth=await queryMetric(db,f.owner,"active-legal-obligations",filters,"month");
  const holds=await queryMetric(db,f.owner,"active-legal-holds",filters);
  const holdsByState=await queryMetric(db,f.owner,"active-legal-holds",filters,"status");
  expect(obligations.total).toBe("2");expect(obligations.rows).toEqual([{label:"delivery",value:"1"},{label:"payment",value:"1"}]);
  expect(obligationsByState.rows).toEqual([{label:"in_progress",value:"1"},{label:"pending",value:"1"}]);
  expect(obligationsByDueMonth.rows).toEqual([{label:"2026-06",value:"1"},{label:"No due date",value:"1"}]);
  expect(holds.total).toBe("2");expect(holdsByState.rows).toEqual([{label:"active",value:"1"},{label:"release_requested",value:"1"}]);
  expect((await queryMetric(db,{...f.owner,branchId:f.branch},"active-legal-holds",filters)).total).toBe("2");

  const firstPage=await searchExecutiveEvidence(db,f.owner,{metric:"active-legal-obligations",filters,query:"Executive legal schedule",limit:1});
  expect(firstPage.provider).toBe("legal-obligations-holds");expect(firstPage.items).toHaveLength(1);expect(firstPage.hasMore).toBe(true);
  const secondPage=await searchExecutiveEvidence(db,f.owner,{metric:"active-legal-obligations",filters,query:"Executive legal schedule",limit:1,cursor:firstPage.nextCursor!});
  expect(secondPage.items).toHaveLength(1);expect(secondPage.items[0].id).not.toBe(firstPage.items[0].id);
  const detail=await readExecutiveEvidence(db,f.owner,"active-legal-obligations",firstPage.items[0].id,filters);
  expect(detail.record).toMatchObject({category:expect.any(String),status:expect.any(String),item_kind:expect.any(String)});
  expect(JSON.stringify(detail)).not.toContain("Private English requirement text");
  const holdEvidence=await searchExecutiveEvidence(db,f.owner,{metric:"active-legal-holds",filters,query:"schedule source",limit:1});
  expect(holdEvidence.items).toHaveLength(1);expect(holdEvidence.hasMore).toBe(true);
  const holdPageTwo=await searchExecutiveEvidence(db,f.owner,{metric:"active-legal-holds",filters,query:"schedule source",limit:1,cursor:holdEvidence.nextCursor!});
  expect(holdPageTwo.items).toHaveLength(1);
  const holdDetail=await readExecutiveEvidence(db,f.owner,"active-legal-holds",holdActive,filters);
  expect(holdDetail.record).toMatchObject({status:"active",item_title:"Executive legal schedule source A"});
  expect(JSON.stringify(holdDetail)).not.toContain("Private English legal hold reason");
  await expect(readExecutiveEvidence(db,f.claimant,"active-legal-holds",holdActive,filters)).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  await expect(readExecutiveEvidence(db,f.owner,"active-legal-holds",holdReleased,filters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});

  const audit=(await db.prepare("SELECT provider_id,metric_id,action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events WHERE company_id=? AND provider_id=? ORDER BY created_at,id").bind(f.company,"legal-obligations-holds").all<{provider_id:string;metric_id:string;action:string;record_id:string|null;record_version:number|null;fields:string}>()).results;
  expect(audit.some(row=>row.action==="open"&&row.record_id===holdActive&&row.record_version===null)).toBe(true);
  expect(audit.some(row=>row.action==="open"&&row.record_id===firstItem&&row.record_version===1)).toBe(true);
  expect(audit.every(row=>row.provider_id==="legal-obligations-holds"&&!row.fields.includes("reason")&&!row.fields.includes("requirement"))).toBe(true);
  expect(JSON.stringify(audit)).not.toContain("Private English legal hold reason");
});
it("unions recorded Maintenance downtime per asset and audits bounded scoped work-order evidence",async()=>{
  const actor={...f.owner,modules:["maintenance"]},filters={from:"2026-01-31",to:"2026-02-28",currency:"THB"},now=Date.parse("2026-02-28T12:00:00Z");
  const assetA=key(),assetB=key();
  const insertAsset=async(id:string,code:string,name:string)=>db.prepare(`INSERT INTO operational_assets
    (id,company_id,branch_id,department_id,code,name,category,currency,cost_minor,condition,lifecycle_status,placement,acquired_date,created_by,created_at,updated_at)
    VALUES(?,?,?,?,?,?, 'equipment','THB',0,'good','in_service','Production floor','2026-01-01',?,?,?)`).bind(id,f.company,f.branch,f.department,code,name,f.owner.userId,now,now).run();
  await insertAsset(assetA,"EX-DOWNTIME-A","Executive downtime asset A");await insertAsset(assetB,"EX-DOWNTIME-B","Executive downtime asset B");
  const insertWork=async(id:string,assetId:string,title:string,status:"completed"|"cancelled"|"in_progress",started:number|null,ended:number|null)=>db.prepare(`INSERT INTO maintenance_work_orders
    (id,company_id,branch_id,department_id,asset_id,title,priority,status,owner_id,downtime_started_at,downtime_ended_at,completion_evidence,resulting_condition,created_by,created_at,updated_at)
    VALUES(?,?,?,?,?,?,'normal',?,?,?,?,'English completion record','good',?,?,?)`).bind(id,f.company,f.branch,f.department,assetId,title,status,f.owner.userId,started,ended,f.owner.userId,now,now).run();
  const first=key(),overlap=key(),otherAsset=key(),unfinished=key();
  await insertWork(first,assetA,"Executive downtime interval A","completed",Date.parse("2026-01-31T23:00:00Z"),Date.parse("2026-02-01T02:00:00Z"));
  await insertWork(overlap,assetA,"Executive downtime interval B","completed",Date.parse("2026-02-01T01:00:00Z"),Date.parse("2026-02-01T04:00:00Z"));
  await insertWork(otherAsset,assetB,"Executive downtime interval C","cancelled",Date.parse("2026-02-02T05:00:00Z"),Date.parse("2026-02-02T05:30:00Z"));
  await insertWork(unfinished,assetB,"Executive unfinished downtime","in_progress",Date.parse("2026-02-03T06:00:00Z"),null);

  const total=await queryMetric(db,actor,"asset-downtime",filters),byMonth=await queryMetric(db,actor,"asset-downtime",filters,"month"),byAsset=await queryMetric(db,actor,"asset-downtime",filters,"asset");
  expect(total).toMatchObject({unit:"duration",valueScale:0,total:"19800000",status:"ready"});
  expect(byMonth.rows).toEqual([{label:"2026-01",value:"3600000"},{label:"2026-02",value:"16200000"}]);
  expect(byAsset.rows).toEqual([{label:"EX-DOWNTIME-A · Executive downtime asset A",value:"18000000"},{label:"EX-DOWNTIME-B · Executive downtime asset B",value:"1800000"}]);
  expect((await queryMetric(db,actor,"asset-downtime",{...filters,to:"2026-01-31"})).total).toBe("3600000");
  expect((await queryMetric(db,{...actor,branchId:"outside-current-scope"},"asset-downtime",filters)).total).toBe("0");
  expect(formatMetric(required(total.total),total)).toBe("5.50 h");expect(formatMetric("1800000",total)).toBe("30.00 min");

  const firstPage=await searchExecutiveEvidence(db,actor,{metric:"asset-downtime",filters,query:"Executive downtime",limit:1});
  expect(firstPage.provider).toBe("maintenance-downtime-work-orders");expect(firstPage.items).toHaveLength(1);expect(firstPage.hasMore).toBe(true);
  const secondPage=await searchExecutiveEvidence(db,actor,{metric:"asset-downtime",filters,query:"Executive downtime",limit:1,cursor:firstPage.nextCursor!});
  expect(secondPage.items).toHaveLength(1);expect(secondPage.items[0].id).not.toBe(firstPage.items[0].id);
  const detail=await readExecutiveEvidence(db,actor,"asset-downtime",first,filters);
  expect(detail.record).toMatchObject({id:first,title:"Executive downtime interval A",status:"completed",asset_code:"EX-DOWNTIME-A",duration_ms:"10800000"});
  expect(JSON.stringify(detail)).not.toContain("English completion record");
  await expect(readExecutiveEvidence(db,f.claimant,"asset-downtime",first,filters)).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  const audit=(await db.prepare("SELECT provider_id,metric_id,action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events WHERE company_id=? AND provider_id=? ORDER BY created_at,id").bind(f.company,"maintenance-downtime-work-orders").all<{provider_id:string;metric_id:string;action:string;record_id:string;record_version:number;fields:string}>()).results;
  expect(audit.some(row=>row.action==="open"&&row.record_id===first&&row.record_version===1)).toBe(true);
  expect(audit.every(row=>row.provider_id==="maintenance-downtime-work-orders"&&row.metric_id==="asset-downtime"&&!row.fields.includes("completion_evidence"))).toBe(true);
  expect(JSON.stringify(audit)).not.toContain("English completion record");
});
it("counts customer identity change events and opens audited metadata without identity payloads",async()=>{
  const suffix=key(),base=Date.parse("2026-04-20T09:00:00Z"),now=Date.now();
  const customers=[
    {id:`executive-identity-customer-a-${suffix}`,code:`EX-ID-A-${suffix.slice(0,6)}`,name:"Executive identity customer Alpha"},
    {id:`executive-identity-customer-b-${suffix}`,code:`EX-ID-B-${suffix.slice(0,6)}`,name:"Executive identity customer Beta"},
  ];
  await db.batch(customers.map(customer=>db.prepare(`INSERT INTO finance_parties(id,company_id,kind,code,name,currency,tax_registration,status,version,created_at,updated_at)
    VALUES(?,?,'customer',?,?,'THB','PRIVATE-TAX-REFERENCE-IDENTITY-TEST','active',1,?,?)`).bind(customer.id,f.company,customer.code,customer.name,now,now)));
  const changes=[{id:key(),customer:customers[0],status:"rejected",version:2},{id:key(),customer:customers[1],status:"pending",version:2}];
  const previous=JSON.stringify({tax_registration:"PRIVATE-OLD-TAX-REFERENCE",phone:"PRIVATE-OLD-PHONE",address_json:{line1:"PRIVATE-OLD-ADDRESS"}});
  const proposed=JSON.stringify({tax_registration:"PRIVATE-NEW-TAX-REFERENCE",phone:"PRIVATE-NEW-PHONE",address_json:{line1:"PRIVATE-NEW-ADDRESS"}});
  await db.batch(changes.map(change=>db.prepare(`INSERT INTO customer_identity_changes
    (id,company_id,customer_id,expected_customer_version,previous_json,proposed_json,status,evidence,created_by,version,created_at,updated_at)
    VALUES(?,?,?,1,?::jsonb,?::jsonb,?,'English-only private review evidence',?,?,?,?)`).bind(
      change.id,f.company,change.customer.id,previous,proposed,change.status,f.owner.userId,change.version,base,base+4000,
    )));
  const eventRows=[
    {id:key(),change:changes[0],version:1,action:"request",at:base,customerVersion:1},
    {id:key(),change:changes[0],version:2,action:"reject",at:base+2*86400000,customerVersion:1},
    {id:key(),change:changes[1],version:1,action:"request",at:base+86400000,customerVersion:1},
    {id:key(),change:changes[1],version:2,action:"resubmit",at:Date.parse("2026-05-01T09:00:00Z"),customerVersion:1},
  ];
  await db.batch(eventRows.map(event=>db.prepare(`INSERT INTO customer_identity_change_events
    (id,change_id,version,action,actor_id,reason,customer_version,previous_json,proposed_json,created_at)
    VALUES(?,?,?,?,?,'English-only private reviewer reason',?,?::jsonb,?::jsonb,?)`).bind(
      event.id,event.change.id,event.version,event.action,f.owner.userId,event.customerVersion,previous,proposed,event.at,
    )));
  const filters={from:"2026-04-01",to:"2026-04-30",currency:"THB"};
  const total=await queryMetric(db,f.owner,"identity-change-events",filters);
  expect(total).toMatchObject({total:"3",unit:"count",status:"ready"});
  expect((await queryMetric(db,f.owner,"identity-change-events",filters,"month")).rows).toEqual([{label:"2026-04",value:"3"}]);
  expect((await queryMetric(db,f.owner,"identity-change-events",filters,"status")).rows).toEqual([{label:"reject",value:"1"},{label:"request",value:"2"}]);
  await expect(queryMetric(db,{...f.owner,branchId:f.branch},"identity-change-events",filters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,{...f.owner,branchId:f.branch},{metric:"identity-change-events",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,{...f.owner,departmentId:f.department},{metric:"identity-change-events",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,f.claimant,{metric:"identity-change-events",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});

  const first=await searchExecutiveEvidence(db,f.owner,{metric:"identity-change-events",filters,query:"Executive identity customer",limit:1});
  const second=await searchExecutiveEvidence(db,f.owner,{metric:"identity-change-events",filters,query:"Executive identity customer",limit:1,cursor:first.nextCursor!});
  expect(first.provider).toBe("customer-identity-events");expect(first.items).toHaveLength(1);expect(second.items).toHaveLength(1);
  expect(first.items[0].href).toBe(`/customer-identity/${changes[0].id}`);expect(stringField(first.items[0],'status')).toBe("reject");
  const detail=await readExecutiveEvidence(db,f.owner,"identity-change-events",first.items[0].id,filters);
  expect(detail.record).toMatchObject({change_id:changes[0].id,action:"reject",customer_version:1,version:2,change_status:"rejected",change_version:2,customer_name:customers[0].name,customer_code:customers[0].code,href:`/customer-identity/${changes[0].id}`});
  expect(detail.fields).toEqual(["action","created_at","version","customer_version","customer.name","customer.code","change.status","change.version"]);
  expect(JSON.stringify([first,second,detail])).not.toContain("PRIVATE-");
  expect(JSON.stringify([first,second,detail])).not.toContain("private reviewer reason");
  expect(JSON.stringify([first,second,detail])).not.toContain("private review evidence");
  expect(detail.record).not.toHaveProperty("previous_json");expect(detail.record).not.toHaveProperty("proposed_json");
  await expect(readExecutiveEvidence(db,f.owner,"identity-change-events",first.items[0].id,{...filters,to:"2026-04-20"})).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});

  const audit=(await db.prepare(`SELECT provider_id,metric_id,action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events
    WHERE company_id=? AND actor_id=? AND provider_id='customer-identity-events' ORDER BY created_at,id`).bind(f.company,f.owner.userId).all<{provider_id:string;metric_id:string;action:string;record_id:string|null;record_version:number|null;fields:string}>()).results;
  expect(audit.map(row=>row.action).sort()).toEqual(["open","search","search"]);
  expect(audit.every(row=>row.provider_id==="customer-identity-events"&&row.metric_id==="identity-change-events"&&row.record_id&&row.record_version!==null)).toBe(true);
  expect(audit.every(row=>JSON.parse(row.fields).join(",")===detail.fields.join(","))).toBe(true);
  expect(JSON.stringify(audit)).not.toContain("PRIVATE-");expect(JSON.stringify(audit)).not.toContain("private reviewer reason");
});
it("reconciles active organization nodes and exposes submitted change metadata to Company HR",async()=>{
  const actor={...f.owner,modules:["organization"]},now=Date.now(),branch=key(),department=key(),suspendedBranch=key(),change=key(),draft=key();
  const filters={from:"2026-01-01",to:"2026-12-31",currency:"THB"};
  const baselineNodes=await queryMetric(db,actor,"active-organization-nodes",filters);
  const baselineChanges=await queryMetric(db,actor,"pending-structure-changes",filters);
  await db.batch([
    db.prepare(`INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision)
      VALUES(?,'branch',?,'Executive evidence branch',?,?,?,'active',3)`).bind(branch,`ex-${branch.slice(0,8)}`,f.company,f.company,branch),
    db.prepare(`INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision)
      VALUES(?,'department',?,'Executive evidence unit',?,?,?,'active',4)`).bind(department,`ex-${department.slice(0,8)}`,branch,f.company,branch),
    db.prepare(`INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision)
      VALUES(?,'branch',?,'Executive suspended unit',?,?,?,'suspended',2)`).bind(suspendedBranch,`ex-${suspendedBranch.slice(0,8)}`,f.company,f.company,suspendedBranch),
    db.prepare(`INSERT INTO organization_node_changes(id,company_id,target_node_id,action,node_kind,slug,display_name,branch_id,expected_node_version,state,version,requested_by,reason,created_at,updated_at)
      VALUES(?,?,NULL,'create','department',?, 'Executive pending department',?,NULL,'submitted',2,?,'English-only private organization request reason',?,?)`)
      .bind(change,f.company,`ex-change-${change.slice(0,8)}`,branch,f.owner.userId,now-1000,now),
    db.prepare(`INSERT INTO organization_node_changes(id,company_id,target_node_id,action,node_kind,slug,display_name,branch_id,expected_node_version,state,version,requested_by,reason,created_at,updated_at)
      VALUES(?,?,NULL,'create','department',?, 'Executive draft department',?,NULL,'draft',1,?,'English-only private draft request reason',?,?)`)
      .bind(draft,f.company,`ex-draft-${draft.slice(0,8)}`,branch,f.owner.userId,now-2000,now-2000),
  ]);

  const nodes=await queryMetric(db,actor,"active-organization-nodes",filters);
  expect(BigInt(required(nodes.total))-BigInt(required(baselineNodes.total))).toBe(2n);
  expect((await queryMetric(db,{...actor,branchId:branch},"active-organization-nodes",filters)).total).toBe("2");
  expect((await queryMetric(db,{...actor,branchId:branch,departmentId:department},"active-organization-nodes",filters)).total).toBe("1");
  const first=await searchExecutiveEvidence(db,actor,{metric:"active-organization-nodes",filters,query:"Executive evidence",limit:1});
  const second=await searchExecutiveEvidence(db,actor,{metric:"active-organization-nodes",filters,query:"Executive evidence",limit:1,cursor:first.nextCursor!});
  expect(first.provider).toBe("organization-records");expect(first.items).toHaveLength(1);expect(second.items).toHaveLength(1);
  const nodeDetail=await readExecutiveEvidence(db,actor,"active-organization-nodes",first.items[0].id,filters);
  expect([requiredField(nodeDetail.record,'display_name'),requiredField(nodeDetail.record,'kind')]).toEqual(["Executive evidence branch","branch"]);
  expect(nodeDetail.fields).toEqual(["kind","slug","display_name","status","branch.name","parent.name","hierarchy_revision"]);
  expect(requiredField(nodeDetail.record,'slug')).toBe(`ex-${branch.slice(0,8)}`);expect(nodeDetail.record).not.toHaveProperty("id",branch);
  await expect(readExecutiveEvidence(db,{...actor,branchId:f.branch},"active-organization-nodes",first.items[0].id,filters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});

  const changes=await queryMetric(db,actor,"pending-structure-changes",filters);
  expect(BigInt(required(changes.total))-BigInt(required(baselineChanges.total))).toBe(1n);
  const changePage=await searchExecutiveEvidence(db,actor,{metric:"pending-structure-changes",filters,query:"Executive pending department",limit:10});
  expect(changePage.items).toHaveLength(1);expect(changePage.items[0].href).toContain(encodeURIComponent(change));
  const changeDetail=await readExecutiveEvidence(db,actor,"pending-structure-changes",changePage.items[0].id,filters);
  expect(changeDetail.record).toMatchObject({action:"create",node_kind:"department",display_name:"Executive pending department",state:"submitted",version:2,branch_name:"Executive evidence branch"});
  expect(changeDetail.fields).toEqual(["action","node_kind","display_name","state","version","created_at","updated_at","branch.name"]);
  expect(JSON.stringify([first,second,nodeDetail,changePage,changeDetail])).not.toContain("private organization request reason");
  expect(JSON.stringify([first,second,nodeDetail,changePage,changeDetail])).not.toContain("private draft request reason");
  await expect(searchExecutiveEvidence(db,{...actor,departmentId:department},{metric:"pending-structure-changes",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,f.claimant,{metric:"pending-structure-changes",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});

  const audit=(await db.prepare(`SELECT provider_id,metric_id,action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events
    WHERE company_id=? AND actor_id=? AND provider_id='organization-records' ORDER BY created_at,id`).bind(f.company,actor.userId).all<{provider_id:string;metric_id:string;action:string;record_id:string|null;record_version:number|null;fields:string}>()).results;
  expect(audit.map(row=>row.action).sort()).toEqual(["open","open","search","search","search"]);
  expect(audit.every(row=>row.record_id&&row.record_version!==null)).toBe(true);
  expect(audit.filter(row=>row.metric_id==="active-organization-nodes").every(row=>JSON.parse(row.fields).join(",")===nodeDetail.fields.join(","))).toBe(true);
  expect(audit.filter(row=>row.metric_id==="pending-structure-changes").every(row=>JSON.parse(row.fields).join(",")===changeDetail.fields.join(","))).toBe(true);
  expect(JSON.stringify(audit)).not.toContain("private organization request reason");
});
it("reconciles effective IAM assignments and audits pending access review evidence",async()=>{
  const actor={...f.owner,modules:["identity_access"]},now=Date.now(),suffix=key(),filters={from:"2020-01-01",to:"2020-01-31",currency:"THB"};
  const baselineCompanyAssignments=await queryMetric(db,actor,"active-assignments",filters);
  const baselineBranchAssignments=await queryMetric(db,{...actor,branchId:f.branch},"active-assignments",filters);
  const baselineDepartmentAssignments=await queryMetric(db,{...actor,branchId:f.branch,departmentId:f.department},"active-assignments",filters);
  const baselineOtherDepartmentAssignments=await queryMetric(db,{...actor,branchId:f.branch,departmentId:f.accountsDepartment},"active-assignments",filters);
  const baselineCompanyReviews=await queryMetric(db,actor,"pending-access-reviews",filters);
  const baselineBranchReviews=await queryMetric(db,{...actor,branchId:f.branch},"pending-access-reviews",filters);
  const baselineDepartmentReviews=await queryMetric(db,{...actor,branchId:f.branch,departmentId:f.department},"pending-access-reviews",filters);
  const baselineOtherDepartmentReviews=await queryMetric(db,{...actor,branchId:f.branch,departmentId:f.accountsDepartment},"pending-access-reviews",filters);
  const assignments=[
    {id:key(),user:`executive-iam-assignee-alpha-${suffix}`,scope:f.department,validUntil:now+86400000,revokedAt:null},
    {id:key(),user:`executive-iam-assignee-beta-${suffix}`,scope:f.accountsDepartment,validUntil:now+86400000,revokedAt:null},
    {id:key(),user:`executive-iam-assignee-expired-${suffix}`,scope:f.department,validUntil:now-1000,revokedAt:null},
    {id:key(),user:`executive-iam-assignee-revoked-${suffix}`,scope:f.department,validUntil:now+86400000,revokedAt:now},
  ];
  await db.batch(assignments.map(assignment=>db.prepare(`INSERT INTO assignments
    (id,user_id,role,modules_json,company_id,branch_id,valid_from,valid_until,revoked_at,version,scope_kind,scope_node_id,source)
    VALUES(?,?,?,'[]',?,?,?,?,?,1,?,?,'manual')`).bind(
      assignment.id,assignment.user,"employee",f.company,f.branch,now-86400000,assignment.validUntil,assignment.revokedAt,"department",assignment.scope,
    )));
  const activeAssignments=await queryMetric(db,actor,"active-assignments",filters);
  expect(BigInt(required(activeAssignments.total))-BigInt(required(baselineCompanyAssignments.total))).toBe(2n);
  expect(BigInt(required((await queryMetric(db,{...actor,branchId:f.branch},"active-assignments",filters)).total))-BigInt(required(baselineBranchAssignments.total))).toBe(2n);
  expect(BigInt(required((await queryMetric(db,{...actor,branchId:f.branch,departmentId:f.department},"active-assignments",filters)).total))-BigInt(required(baselineDepartmentAssignments.total))).toBe(1n);
  expect(BigInt(required((await queryMetric(db,{...actor,branchId:f.branch,departmentId:f.accountsDepartment},"active-assignments",filters)).total))-BigInt(required(baselineOtherDepartmentAssignments.total))).toBe(1n);
  const assignmentFirst=await searchExecutiveEvidence(db,actor,{metric:"active-assignments",filters,query:"executive-iam-assignee-",limit:1});
  const assignmentSecond=await searchExecutiveEvidence(db,actor,{metric:"active-assignments",filters,query:"executive-iam-assignee-",limit:1,cursor:assignmentFirst.nextCursor!});
  expect(assignmentFirst.provider).toBe("identity-access-assignments");expect(assignmentFirst.items).toHaveLength(1);expect(assignmentSecond.items).toHaveLength(1);
  expect(assignmentFirst.items[0].label).toContain("executive-iam-assignee-alpha");expect(assignmentSecond.items[0].label).toContain("executive-iam-assignee-beta");
  const assignmentDetail=await readExecutiveEvidence(db,actor,"active-assignments",assignmentFirst.items[0].id,filters);
  expect(assignmentDetail.record).toMatchObject({user_id:assignments[0].user,role:"employee",scope_kind:"department",scope_node_id:f.department,source:"manual",version:1});
  expect(assignmentDetail.fields).toContain("user.name");expect(assignmentDetail.fields).toContain("branch.name");
  await expect(readExecutiveEvidence(db,{...actor,branchId:f.branch,departmentId:f.accountsDepartment},"active-assignments",assignmentFirst.items[0].id,filters)).rejects.toMatchObject({status:404,code:"EVIDENCE_NOT_FOUND"});
  await expect(searchExecutiveEvidence(db,f.claimant,{metric:"active-assignments",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});

  const dueAt=now+5*86400000,campaign=key(),completedCampaign=key();
  await db.batch([
    db.prepare(`INSERT INTO iam_access_review_campaigns(id,company_id,name,due_at,status,version,created_by,reason,created_at,updated_at,completed_at)
      VALUES(?,?,?,?,'open',3,?,?,?, ?,NULL)`).bind(campaign,f.company,"Executive IAM review campaign",dueAt,f.owner.userId,"English-only private campaign reason",now-4000,now-3000),
    db.prepare(`INSERT INTO iam_access_review_campaigns(id,company_id,name,due_at,status,version,created_by,reason,created_at,updated_at,completed_at)
      VALUES(?,?,?,?,'completed',2,?,?,?, ?,?)`).bind(completedCampaign,f.company,"Executive completed IAM review",dueAt,f.owner.userId,"English-only private completed campaign reason",now-2000,now-1000,now-500),
  ]);
  const reviewItems=[
    {id:key(),campaign:campaign,assignment:assignments[0].id,assignmentVersion:1,user:assignments[0].user,role:"employee",scopeKind:"department",scopeNode:f.department,source:"manual",validUntil:assignments[0].validUntil,decision:"pending",decisionReason:null,decidedAt:null},
    {id:key(),campaign:campaign,assignment:assignments[1].id,assignmentVersion:1,user:assignments[1].user,role:"employee",scopeKind:"department",scopeNode:f.accountsDepartment,source:"manual",validUntil:assignments[1].validUntil,decision:"pending",decisionReason:null,decidedAt:null},
    {id:key(),campaign:campaign,assignment:f.owner.assignmentId,assignmentVersion:1,user:f.owner.userId,role:"holding_owner",scopeKind:"holding",scopeNode:null,source:"legacy",validUntil:9999999999999,decision:"pending",decisionReason:null,decidedAt:null},
    {id:key(),campaign:campaign,assignment:assignments[2].id,assignmentVersion:1,user:assignments[2].user,role:"employee",scopeKind:"department",scopeNode:f.department,source:"manual",validUntil:assignments[2].validUntil,decision:"keep",decisionReason:"English-only private reviewer decision note",decidedAt:now-800},
    {id:key(),campaign:completedCampaign,assignment:assignments[0].id,assignmentVersion:1,user:assignments[0].user,role:"employee",scopeKind:"department",scopeNode:f.department,source:"manual",validUntil:assignments[0].validUntil,decision:"pending",decisionReason:null,decidedAt:null},
  ];
  await db.batch(reviewItems.map(item=>db.prepare(`INSERT INTO iam_access_review_items
    (id,campaign_id,company_id,assignment_id,assignment_version,user_id,role,scope_kind,scope_node_id,source,valid_until,decision,decision_reason,decided_at)
    VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)`).bind(
      item.id,item.campaign,f.company,item.assignment,item.assignmentVersion,item.user,item.role,item.scopeKind,item.scopeNode,item.source,item.validUntil,item.decision,item.decisionReason,item.decidedAt,
    )));
  const companyReviews=await queryMetric(db,actor,"pending-access-reviews",filters);
  expect(BigInt(required(companyReviews.total))-BigInt(required(baselineCompanyReviews.total))).toBe(3n);
  expect(BigInt(required((await queryMetric(db,{...actor,branchId:f.branch},"pending-access-reviews",filters)).total))-BigInt(required(baselineBranchReviews.total))).toBe(3n);
  expect(BigInt(required((await queryMetric(db,{...actor,branchId:f.branch,departmentId:f.department},"pending-access-reviews",filters)).total))-BigInt(required(baselineDepartmentReviews.total))).toBe(2n);
  expect(BigInt(required((await queryMetric(db,{...actor,branchId:f.branch,departmentId:f.accountsDepartment},"pending-access-reviews",filters)).total))-BigInt(required(baselineOtherDepartmentReviews.total))).toBe(2n);
  const reviewFirst=await searchExecutiveEvidence(db,actor,{metric:"pending-access-reviews",filters,query:"Executive IAM review campaign",limit:1});
  const reviewSecond=await searchExecutiveEvidence(db,actor,{metric:"pending-access-reviews",filters,query:"Executive IAM review campaign",limit:1,cursor:reviewFirst.nextCursor!});
  expect(reviewFirst.provider).toBe("identity-access-reviews");expect(reviewFirst.items).toHaveLength(1);expect(reviewSecond.items).toHaveLength(1);
  expect(reviewFirst.items[0].id).not.toBe(reviewSecond.items[0].id);expect(reviewFirst.items[0].href).toContain(`/people/settings?section=reviews#${campaign}`);
  const reviewDetail=await readExecutiveEvidence(db,actor,"pending-access-reviews",reviewFirst.items[0].id,filters);
  expect(reviewDetail.record).toMatchObject({campaign_name:"Executive IAM review campaign",campaign_status:"open",campaign_version:3,decision:"pending"});
  expect(reviewDetail.fields).toEqual(["campaign.name","campaign.status","campaign.due_at","campaign.version","user.id","user.name","role","scope_kind","scope_node_id","scope.name","branch.name","source","valid_until","assignment_version","decision"]);
  expect(JSON.stringify([reviewFirst,reviewSecond,reviewDetail])).not.toContain("private campaign reason");
  expect(JSON.stringify([reviewFirst,reviewSecond,reviewDetail])).not.toContain("private reviewer decision note");
  await expect(searchExecutiveEvidence(db,f.claimant,{metric:"pending-access-reviews",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});

  const assignmentAudit=(await db.prepare(`SELECT metric_id,action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events
    WHERE company_id=? AND actor_id=? AND provider_id='identity-access-assignments' ORDER BY created_at,id`).bind(f.company,actor.userId).all<{metric_id:string;action:string;record_id:string|null;record_version:number|null;fields:string}>()).results;
  expect(assignmentAudit.map(row=>row.action).sort()).toEqual(["open","search","search"]);
  expect(assignmentAudit.every(row=>row.metric_id==="active-assignments"&&row.record_id&&row.record_version===1)).toBe(true);
  expect(assignmentAudit.every(row=>JSON.parse(row.fields).join(",")===assignmentDetail.fields.join(","))).toBe(true);
  const reviewAudit=(await db.prepare(`SELECT metric_id,action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events
    WHERE company_id=? AND actor_id=? AND provider_id='identity-access-reviews' ORDER BY created_at,id`).bind(f.company,actor.userId).all<{metric_id:string;action:string;record_id:string|null;record_version:number|null;fields:string}>()).results;
  expect(reviewAudit.map(row=>row.action).sort()).toEqual(["open","search","search"]);
  expect(reviewAudit.every(row=>row.metric_id==="pending-access-reviews"&&row.record_id&&row.record_version===3)).toBe(true);
  expect(reviewAudit.every(row=>JSON.parse(row.fields).join(",")===reviewDetail.fields.join(","))).toBe(true);
  expect(JSON.stringify([assignmentAudit,reviewAudit])).not.toContain("private campaign reason");
  expect(JSON.stringify([assignmentAudit,reviewAudit])).not.toContain("private reviewer decision note");
});
it("reconciles Holding commercial metrics from Control billing and limits access and distribution to the Holding owner",async()=>{
  const actor=f.owner,control=env.CONTROL_DB,holding=actor.holdingId,now=Date.now(),filters={from:"2026-04-01",to:"2026-05-31",currency:"THB"};
  await control.prepare(`INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding,status)
    VALUES(?,?,?,?,?,?,'active')`).bind(holding,"executive-commercial", "Executive Commercial Fixture","executive-commercial-write","executive-commercial-read","executive-commercial-files").run();
  await control.prepare("INSERT INTO billing_accounts(holding_id,version,created_at) VALUES(?,7,?)").bind(holding,now).run();
  await control.prepare("INSERT INTO beta_billing(holding_id,subscription_id,status,current_period_end,last_event_at,updated_at) VALUES(?,?,'trialing',?,?,?)")
    .bind(holding,`sub_${key()}`,Math.floor((now+30*86400000)/1000),now,now).run();
  const activeEnrollment=key(),endedEnrollment=key();
  await control.batch([
    control.prepare(`INSERT INTO billing_enrollments(id,holding_id,plan_id,started_at,ended_at,pending_end,effective_at)
      VALUES(?,?, 'standard',?,NULL,TRUE,?)`).bind(activeEnrollment,holding,now-86400000,now+7*86400000),
    control.prepare(`INSERT INTO billing_enrollments(id,holding_id,plan_id,started_at,ended_at,pending_end,effective_at)
      VALUES(?,?, 'priority',?,?,FALSE,NULL)`).bind(endedEnrollment,holding,now-30*86400000,now-86400000),
  ]);
  const active=await queryMetric(db,actor,"active-subscriptions",filters);
  expect(active).toMatchObject({total:"2",unit:"count",status:"ready",scope:"Selected Holding billing",scopeBoundary:{level:"holding"}});
  expect((await queryMetric(db,actor,"active-subscriptions",filters,"status")).rows).toEqual([{label:"ending",value:"1"},{label:"trialing",value:"1"}]);
  expect((await queryMetric(db,actor,"active-subscriptions",filters,"plan")).rows).toEqual([{label:"Legacy subscription",value:"1"},{label:"Standard Support",value:"1"}]);
  const subscriptionFirst=await searchExecutiveEvidence(db,actor,{metric:"active-subscriptions",filters,limit:1}),
    subscriptionSecond=await searchExecutiveEvidence(db,actor,{metric:"active-subscriptions",filters,limit:1,cursor:subscriptionFirst.nextCursor!});
  expect(subscriptionFirst.provider).toBe("commercial-subscriptions");
  expect(subscriptionFirst.items).toHaveLength(1);expect(subscriptionSecond.items).toHaveLength(1);
  expect(subscriptionFirst.items[0].id).not.toBe(subscriptionSecond.items[0].id);
  expect(JSON.stringify([subscriptionFirst,subscriptionSecond])).not.toContain("sub_");
  const enrollmentDetail=await readExecutiveEvidence(db,actor,"active-subscriptions",subscriptionFirst.items[0].id,filters),
    legacyDetail=await readExecutiveEvidence(db,actor,"active-subscriptions",subscriptionSecond.items[0].id,filters);
  expect(enrollmentDetail.record).toMatchObject({source:"enrollment",plan_name:"Standard Support",status:"ending",ends_at:now+7*86400000,href:"/settings?section=usage&subscription=enrollment"});
  expect(enrollmentDetail.fields).toEqual(["plan.name","status","started_at","effective_at","billing_accounts.version"]);
  expect(legacyDetail.record).toMatchObject({source:"legacy",plan_name:"Legacy subscription",status:"trialing",href:"/settings?section=usage&subscription=legacy"});
  expect(JSON.stringify(legacyDetail)).not.toContain("sub_");

  const invoiceCases=[
    {status:"open",due:"2026-04-10",kind:"main"},
    {status:"uncollectible",due:"2026-04-20",kind:"adjustment"},
    {status:"paid",due:"2026-04-15",kind:"main"},
    {status:"void",due:"2026-04-18",kind:"main"},
    {status:"draft",due:"2026-04-22",kind:"main"},
    {status:"open",due:"2026-05-20",kind:"main"},
    {status:"open",due:"2026-06-01",kind:"main"},
    {status:"open",due:"2026-03-31",kind:"main"},
  ];
  const invoiceIds=invoiceCases.map(()=>key());
  await control.batch(invoiceCases.flatMap((invoice,index)=>{
    const periodId=key(),month=`2026-${String(index+1).padStart(2,"0")}`,start=Date.parse(`${month}-01`),end=index===11?start+86400000:Date.parse(`2026-${String(index+2).padStart(2,"0")}-01`);
    return [
      control.prepare(`INSERT INTO billing_periods(id,holding_id,enrollment_id,month,start_at,end_at,plan_id,plan_revision,support_satang)
        VALUES(?,?,?,?,?,?,'standard',1,0)`).bind(periodId,holding,activeEnrollment,month,start,end),
      control.prepare(`INSERT INTO billing_invoices(id,holding_id,period_id,kind,status,total_satang,lines_json,payment_mode,due_at,created_at,approved_by,updated_at)
        VALUES(?,?,?,?,?,100000,'[]','manual',?,?,?,?)`).bind(invoiceIds[index],holding,periodId,invoice.kind,invoice.status,Date.parse(`${invoice.due}T12:00:00Z`),now-index,actor.userId,now-index),
    ];
  }));
  const overdue=await queryMetric(db,actor,"overdue-invoices",filters);
  expect(overdue).toMatchObject({total:"3",unit:"count",status:"ready",scopeBoundary:{level:"holding"}});
  expect((await queryMetric(db,actor,"overdue-invoices",filters,"month")).rows).toEqual([{label:"2026-04",value:"2"},{label:"2026-05",value:"1"}]);
  expect((await queryMetric(db,actor,"overdue-invoices",filters,"status")).rows).toEqual([{label:"open",value:"2"},{label:"uncollectible",value:"1"}]);
  const previousSourceRevision=overdue.sourceRevision;
  await control.prepare("UPDATE billing_invoices SET status='paid',updated_at=? WHERE id=?").bind(now+1,invoiceIds[0]).run();
  const refreshed=await queryMetric(db,actor,"overdue-invoices",filters);
  expect(refreshed.total).toBe("2");expect(refreshed.sourceRevision).not.toBe(previousSourceRevision);
  const invoiceFirst=await searchExecutiveEvidence(db,actor,{metric:"overdue-invoices",filters,limit:1}),
    invoiceSecond=await searchExecutiveEvidence(db,actor,{metric:"overdue-invoices",filters,limit:1,cursor:invoiceFirst.nextCursor!});
  expect(invoiceFirst.provider).toBe("commercial-overdue-invoices");expect(invoiceFirst.items).toHaveLength(1);expect(invoiceSecond.items).toHaveLength(1);
  expect(invoiceFirst.items[0].id).not.toBe(invoiceSecond.items[0].id);
  expect(invoiceFirst.items[0]).toMatchObject({amount:"100000",currency:"THB",href:"/settings?section=usage&invoice="+invoiceIds[1]});
  const invoiceDetail=await readExecutiveEvidence(db,actor,"overdue-invoices",invoiceFirst.items[0].id,filters);
  expect(invoiceDetail.record).toMatchObject({kind:"adjustment",status:"uncollectible",total_satang:"100000",month:"2026-02",href:"/settings?section=usage&invoice="+invoiceIds[1]});
  expect(invoiceDetail.fields).toEqual(["kind","status","due_at","total_satang","updated_at","billing_periods.month"]);
  expect(JSON.stringify(invoiceDetail)).not.toMatch(/lines_json|stripe_id|hosted_url|pdf_url|card_json|payment_mode/);
  await control.prepare("UPDATE billing_invoices SET status='paid',updated_at=? WHERE id=?").bind(now+2,invoiceIds[5]).run();
  await expect(readExecutiveEvidence(db,actor,"overdue-invoices",invoiceSecond.items[0].id,filters)).rejects.toMatchObject({code:"EVIDENCE_NOT_FOUND"});

  expect(metricCatalog(actor).find(metric=>metric.id==="active-subscriptions")).toMatchObject({available:true,acceptancePending:true});
  expect(metricCatalog(f.claimant).find(metric=>metric.id==="active-subscriptions")).toMatchObject({available:false});
  await expect(queryMetric(db,f.claimant,"overdue-invoices",filters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,f.claimant,{metric:"active-subscriptions",filters,limit:5})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  await expect(readExecutiveEvidence(db,f.claimant,"overdue-invoices",invoiceFirst.items[0].id,filters)).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});

  const commercialAudit=(await db.prepare("SELECT provider_id,metric_id,action,record_id,record_version,field_ids::text fields FROM executive_evidence_access_events WHERE company_id=? AND actor_id=? AND provider_id IN ('commercial-subscriptions','commercial-overdue-invoices')")
    .bind(f.company,actor.userId).all<{provider_id:string;metric_id:string;action:string;record_id:string|null;record_version:number|null;fields:string}>()).results;
  expect(commercialAudit.filter(row=>row.action==="open")).toHaveLength(3);
  expect(commercialAudit.every(row=>row.record_id&&row.record_version!==null)).toBe(true);
  expect(commercialAudit.some(row=>row.provider_id==="commercial-subscriptions"&&row.metric_id==="active-subscriptions")).toBe(true);
  expect(commercialAudit.some(row=>row.provider_id==="commercial-overdue-invoices"&&row.metric_id==="overdue-invoices")).toBe(true);
  expect(JSON.stringify(commercialAudit)).not.toMatch(/lines_json|stripe_id|hosted_url|pdf_url|card_json|payment_mode/);

  const report=await createReport(db,actor,key(),"Holding billing pulse"),definition=definitionSchema.parse({...emptyDefinition("Holding billing pulse"),widgets:[{id:"billing",title:"Overdue subscriptions",metric:"overdue-invoices",kind:"kpi",width:"half"}]});
  await saveReport(db,actor,report.id,saveSchema.parse({expectedVersion:1,definition,filters}),key());
  await expect(saveShare(db,actor,report.id,shareSchema.parse({recipientId:f.claimant.userId,mode:"distribution",widgetIds:["billing"],expiresAt:null,filters}),key()))
    .rejects.toMatchObject({code:"RECIPIENT_SOURCE_REQUIRED"});
});
it("reconciles Agent governance metrics with bounded, permission-checked evidence that excludes model and source contents",async()=>{
  const actor={...f.owner,modules:[...f.owner.modules,"agent"]},filters={from:"2026-09-01",to:"2026-09-30",currency:"THB"},now=Date.parse("2026-09-15T12:00:00Z"),
    agentId=key(),approvedVersionId=key(),draftVersionId=key(),otherAgentId=key(),otherVersionId=key(),
    succeededSessionId=key(),failedSessionId=key(),oldSessionId=key(),foreignSessionId=key(),
    succeededId=key(),failedId=key(),oldId=key(),foreignId=key();
  await db.batch([
    db.prepare(`INSERT INTO agent_catalog(id,company_id,name,purpose,status,created_by,reviewed_by,version,created_at,updated_at)
      VALUES(?,?,?,'Private purpose text','approved',?,?,3,?,?)`).bind(agentId,f.company,"Executive agent evidence fixture",f.claimant.userId,f.owner.userId,now-1000,now-500),
    db.prepare(`INSERT INTO agent_versions(id,agent_id,version_number,prompt,allowed_sources_json,prohibited_actions_json,status,created_by,reviewed_by,version,created_at,updated_at)
      VALUES(?,?,4,'Private prompt text','[\"/private/source\"]','[\"approve\"]','approved',?,?,2,?,?)`).bind(approvedVersionId,agentId,f.claimant.userId,f.owner.userId,now-900,now-400),
    db.prepare(`INSERT INTO agent_versions(id,agent_id,version_number,prompt,allowed_sources_json,prohibited_actions_json,status,created_by,reviewed_by,version,created_at,updated_at)
      VALUES(?,?,5,'Draft prompt text','[\"/draft/source\"]','[]','draft',?,NULL,1,?,?)`).bind(draftVersionId,agentId,f.owner.userId,now,now),
    db.prepare(`INSERT INTO agent_catalog(id,company_id,name,purpose,status,created_by,version,created_at,updated_at)
      VALUES(?,?,?,'Foreign purpose','approved',?,1,?,?)`).bind(otherAgentId,"other-company","Foreign agent fixture",f.owner.userId,now,now),
    db.prepare(`INSERT INTO agent_versions(id,agent_id,version_number,prompt,allowed_sources_json,status,created_by,reviewed_by,version,created_at,updated_at)
      VALUES(?,?,1,'Foreign prompt','[]','approved',?,?,1,?,?)`).bind(otherVersionId,otherAgentId,f.claimant.userId,f.owner.userId,now,now),
    ...[[succeededSessionId,f.company,actor.userId,actor.assignmentId],[failedSessionId,f.company,actor.userId,actor.assignmentId],[oldSessionId,f.company,actor.userId,actor.assignmentId],[foreignSessionId,"other-company",actor.userId,actor.assignmentId]].map(([id,company,user,assignment])=>
      db.prepare(`INSERT INTO ai_private_sessions(id,company_id,user_id,assignment_id,scope_key,conversation_key,agent_assignment_id,created_at,last_used_at)
        VALUES(?,?,?,?, 'executive-evidence',?,NULL,?,?)`).bind(id,company,user,assignment,`English fixture ${id}`,now,now)),
    db.prepare(`INSERT INTO ai_invocation_evidence(id,session_id,turn_id,company_id,user_id,agent_version_id,question_digest,sources_digest,authority_snapshot_json,provider_snapshot_json,outcome,input_tokens,output_tokens,error,created_at)
      VALUES(?,?,?,?,?,?,'private-question-digest','private-source-digest','{}','{}','succeeded',10,15,NULL,?)`).bind(succeededId,succeededSessionId,"turn-executive-evidence-success",f.company,actor.userId,approvedVersionId,now-100),
    db.prepare(`INSERT INTO ai_invocation_evidence(id,session_id,turn_id,company_id,user_id,agent_version_id,question_digest,sources_digest,authority_snapshot_json,provider_snapshot_json,outcome,input_tokens,output_tokens,error,created_at)
      VALUES(?,?,?,?,?,?,'private-question-digest','private-source-digest','{}','{}','failed',10,0,'Private model error text',?)`).bind(failedId,failedSessionId,"turn-executive-evidence-failure",f.company,actor.userId,approvedVersionId,now-200),
    db.prepare(`INSERT INTO ai_invocation_evidence(id,session_id,turn_id,company_id,user_id,agent_version_id,question_digest,sources_digest,authority_snapshot_json,provider_snapshot_json,outcome,created_at)
      VALUES(?,?,?,?,?,?,'private-old-question','private-old-source','{}','{}','succeeded',?)`).bind(oldId,oldSessionId,"turn-executive-evidence-old",f.company,actor.userId,approvedVersionId,Date.parse("2025-12-31T23:00:00Z")),
    db.prepare(`INSERT INTO ai_invocation_evidence(id,session_id,turn_id,company_id,user_id,agent_version_id,question_digest,sources_digest,authority_snapshot_json,provider_snapshot_json,outcome,created_at)
      VALUES(?,?,?,?,?,?,'private-foreign-question','private-foreign-source','{}','{}','succeeded',?)`).bind(foreignId,foreignSessionId,"turn-executive-evidence-foreign","other-company",actor.userId,otherVersionId,now),
  ]);

  const approvedMetric=await queryMetric(db,actor,"approved-agent-versions",filters),outcomes=await queryMetric(db,actor,"agent-invocation-outcomes",filters),
    statuses=await queryMetric(db,actor,"agent-invocation-outcomes",filters,"status");
  expect(approvedMetric).toMatchObject({total:"1",unit:"count",status:"ready"});
  expect(outcomes).toMatchObject({total:"2",unit:"count",status:"ready"});
  expect(statuses.rows).toEqual([{label:"failed",value:"1"},{label:"succeeded",value:"1"}]);

  const versionSearch=await searchExecutiveEvidence(db,actor,{metric:"approved-agent-versions",filters,query:"Executive agent evidence",limit:10});
  expect(versionSearch.provider).toBe("agent-governance");expect(versionSearch.items).toHaveLength(1);
  expect(versionSearch.items[0]).toMatchObject({id:approvedVersionId,label:"Executive agent evidence fixture · Version 4",href:"/agents",version:2});
  const versionDetail=await readExecutiveEvidence(db,actor,"approved-agent-versions",approvedVersionId,filters);
  expect(versionDetail.record).toMatchObject({agent_id:agentId,agent_name:"Executive agent evidence fixture",version_number:4,version:2,href:"/agents"});
  expect(versionDetail.fields).toEqual(["agent.name","agent.status","version.version_number","version.status","version.version","version.created_at","version.updated_at"]);
  expect(JSON.stringify([versionSearch,versionDetail])).not.toMatch(/prompt|purpose|allowed_sources|prohibited_actions/);
  await expect(readExecutiveEvidence(db,actor,"approved-agent-versions",draftVersionId,filters)).rejects.toMatchObject({code:"EVIDENCE_NOT_FOUND"});

  const first=await searchExecutiveEvidence(db,actor,{metric:"agent-invocation-outcomes",filters,query:"turn-executive-evidence",limit:1}),
    second=await searchExecutiveEvidence(db,actor,{metric:"agent-invocation-outcomes",filters,query:"turn-executive-evidence",limit:1,cursor:first.nextCursor!});
  expect(first.provider).toBe("agent-governance");expect(first.items).toHaveLength(1);expect(second.items).toHaveLength(1);
  expect(new Set([...first.items,...second.items].map(item=>item.id))).toEqual(new Set([succeededId,failedId]));
  const invocationDetail=await readExecutiveEvidence(db,actor,"agent-invocation-outcomes",first.items[0].id,filters);
  expect(invocationDetail.record).toMatchObject({turn_id:expect.stringMatching(/^turn-executive-evidence-/),href:"/agents"});
  expect(invocationDetail.fields).toEqual(["turn_id","outcome","created_at"]);
  expect(JSON.stringify([first,second,invocationDetail])).not.toMatch(/private-question|private-source|Private model error|authority_snapshot|provider_snapshot|user_id|input_tokens|output_tokens/);
  await expect(readExecutiveEvidence(db,actor,"agent-invocation-outcomes",oldId,filters)).rejects.toMatchObject({code:"EVIDENCE_NOT_FOUND"});

  const scoped={...actor,branchId:f.branch};
  await expect(queryMetric(db,scoped,"approved-agent-versions",filters)).rejects.toMatchObject({code:"METRIC_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,scoped,{metric:"approved-agent-versions",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  await expect(searchExecutiveEvidence(db,{...actor,modules:[]},{metric:"agent-invocation-outcomes",filters,limit:10})).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});
  await expect(readExecutiveEvidence(db,{...actor,modules:[]},"approved-agent-versions",approvedVersionId,filters)).rejects.toMatchObject({code:"EVIDENCE_FORBIDDEN"});

  const audit=(await db.prepare(`SELECT provider_id,metric_id,action,record_id,record_version,field_ids::text fields,query_hash FROM executive_evidence_access_events
    WHERE company_id=? AND actor_id=? AND provider_id='agent-governance' ORDER BY created_at,id`).bind(f.company,actor.userId)
    .all<{provider_id:string;metric_id:string;action:string;record_id:string|null;record_version:number|null;fields:string;query_hash:string}>()).results;
  expect(audit.some(row=>row.metric_id==="approved-agent-versions"&&row.action==="open"&&row.record_id===approvedVersionId&&row.record_version===2)).toBe(true);
  expect(audit.some(row=>row.metric_id==="agent-invocation-outcomes"&&row.action==="open"&&row.record_id===succeededId&&row.record_version===1)).toBe(true);
  expect(audit.every(row=>JSON.parse(row.fields).every((field:string)=>!/(prompt|question|source|error|user|provider)/i.test(field)))).toBe(true);
  expect(JSON.stringify(audit)).not.toMatch(/private-question|private-source|Private model error|turn-executive-evidence/);
});

function required<Value>(value:Value|null|undefined):Value{if(value==null)throw Error('Required fixture fact is missing');return value;}
function requiredField(value:object,name:string):unknown{if(!(name in value))throw Error('Required evidence field is missing: '+name);return Reflect.get(value,name);}
function stringField(value:object,name:string):string{const field=requiredField(value,name);if(typeof field!=='string')throw Error('Required evidence field is not a string: '+name);return field;}
function htmlMetric(overrides:Partial<MetricData>={}):MetricData{return {schemaVersion:2,metric:'headcount',total:'1',unit:'count',currency:null,valueScale:0,rows:[{label:'People',value:'1'}],formula:'count',definition:'Current people',scope:'Company',scopeBoundary:{level:'company',revision:'company-scope'},referenceAt:0,updatedAt:0,sourceRevision:'source',queryDefinitionRevision:'query',revision:'fixture',status:'ready',sourceReferences:[],...overrides};}
