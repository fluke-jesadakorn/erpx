import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { env, applyMigrations } from "./runtime-env";
import { expenseDb as db, expenseFixture, key } from "./helpers/expense-fixture";
import { createReport, reportView, reportLibrary, reportData, saveReport, saveShare, revokeShare, saveSchema } from "../src/server/executive/reports";
import { queueGeneration, drainExecutiveGeneration, generationHistory, cancelGeneration } from "../src/server/executive/generation";
import { canAuthor, currentAuthority } from "../src/server/executive/access";
import { queryMetric } from "../src/server/executive/metrics";
import { defaultFilters, definitionSchema, emptyDefinition, generateSchema, shareSchema } from "../src/lib/executive/canvas";
vi.mock("../src/server/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
const aiEnv={AI_BASE_URL:"https://opencode.ai/zen/go/v1",AI_MODEL:"muse-spark-1.3-contributor",AI_API_KEY:"synthetic-key"};
let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
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
  const result=await queryMetric(db,f.claimant,"my-tasks",defaultFilters());expect(result.total).toBe(350);expect(result.rows).toEqual([{label:"ready",value:350}]);
});
it("allows a bounded published live view without granting source-module access",async()=>{
  const id=await report();await saveShare(db,f.owner,id,shareSchema.parse({recipientId:f.claimant.userId,mode:"distribution",widgetIds:["people"],expiresAt:null,filters:defaultFilters()}),key());
  const view=await reportView(db,f.claimant,id);expect(view.canEdit).toBe(false);expect(view.lockedFilters).toBe(true);expect(view.shares).toEqual([]);expect(view.revisions).toEqual([]);
  expect((await reportData(db,f.claimant,id)).metrics.headcount.error).toBeUndefined();
  await expect(saveReport(db,f.claimant,id,saveSchema.parse({expectedVersion:2,definition:emptyDefinition(),filters:defaultFilters()}),key())).rejects.toMatchObject({code:"REPORT_READ_ONLY"});
  expect(await generationHistory(db,f.claimant,id)).toEqual([]);
});
it("uses recipient source access instead of the publisher's authority by default",async()=>{
  const id=await report();await saveShare(db,f.owner,id,shareSchema.parse({recipientId:f.claimant.userId,mode:"recipient",widgetIds:["people"],expiresAt:null,filters:defaultFilters()}),key());
  expect((await reportData(db,f.claimant,id)).metrics.headcount.error).toBeTruthy();
});
it("pins shared definitions so later widgets do not expand a grant",async()=>{
  const id=await report();await saveShare(db,f.owner,id,shareSchema.parse({recipientId:f.claimant.userId,mode:"distribution",widgetIds:["people"],expiresAt:null,filters:defaultFilters()}),key());
  const r=await reportView(db,f.owner,id);await saveReport(db,f.owner,id,saveSchema.parse({expectedVersion:r.version,definition:{...r.definition,widgets:[...r.definition.widgets,{id:"stock",title:"Stock",metric:"inventory"}]},filters:r.filters}),key());
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
  expect(revenue.total).toBe(300300);expect(ar.total).toBe(150300);expect(revenue.rows).toEqual([{label:"2026-03",value:300300}]);
  expect((await queryMetric(db,f.owner,"invoiced-sales",{from:"2026-01-01",to:"2026-12-31",currency:"USD"})).total).toBe(1001);
  expect((await queryMetric(db,f.owner,"invoiced-sales",{from:"2026-04-01",to:"2026-04-30",currency:"THB"})).total).toBe(0);
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
  expect((await db.prepare("SELECT state FROM executive_generation_jobs WHERE id=?").bind(k).first<{state:string}>())?.state).toBe("failed");
  expect((await db.prepare("SELECT version FROM executive_reports WHERE id=?").bind(id).first<{version:number}>())?.version).toBe(2);
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
  expect(owner.snapshots.find(m=>m.metric==="headcount"&&m.breakdown==="default")?.total).toBeGreaterThan(0);
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
  expect(after.total).toBe(before.total);expect(after.rows.reduce((n,r)=>n+r.value,0)).toBe(after.total);
  await expect(queryMetric(db,f.owner,"headcount",defaultFilters(),"month")).rejects.toMatchObject({code:"BREAKDOWN_UNAVAILABLE"});
});
it("persists custom HTML/CSS and dimensions and binds it to authorized live values",async()=>{
  const id=await report();const r=await reportView(db,f.owner,id);
  const definition=definitionSchema.parse({...r.definition,widgets:[{...r.definition.widgets[0],kind:"html",breakdown:"branch",columns:9,height:480,html:"<section><h1>{{title}}</h1><strong>{{total}}</strong>{{chart:donut}}</section>",css:"section { background: #123; border-radius: 22px; }"}]});
  await saveReport(db,f.owner,id,saveSchema.parse({expectedVersion:r.version,definition,filters:r.filters}),key());
  const saved=await reportView(db,f.owner,id);expect(saved.definition.widgets[0]).toMatchObject({columns:9,height:480,kind:"html"});
  const d=await reportData(db,f.owner,id);expect(d.metrics["headcount:branch"].total).toBeGreaterThan(0);expect(d.documents.people).toContain("<svg");expect(d.documents.people).not.toContain("{{total}}");expect(d.documents.people).toContain("border-radius: 22px");
});
it("isolates generated markup and blocks script, navigation and external fetches",async()=>{
  const {htmlWidgetDocument}=await import("../src/server/executive/html-widget");
  const widget=definitionSchema.parse({...emptyDefinition(),widgets:[{id:"x",title:"Test",metric:"headcount",kind:"html",html:'<script>alert(1)</script><img src="https://evil.test" onerror="alert(1)"><a href="https://evil.test">Go</a><iframe src="https://evil.test"></iframe><meta http-equiv="refresh" content="0;url=https://evil.test"><div>{{rows}}</div>',css:'body{color:red}</style><script>alert(2)</script>'}]}).widgets[0];
  const doc=htmlWidgetDocument(widget,{metric:"headcount",total:1,unit:"count",currency:null,rows:[{label:'<img src=x onerror=alert(3)>',value:1}],definition:"Test",scope:"Company",revision:"x",updatedAt:0});
  expect(doc).not.toMatch(/<script|<iframe|<img|<[^>]+(?:href|onerror)=/i);
  expect(doc).toContain("default-src 'none'");expect(doc).toContain("connect-src 'none'");expect(doc).toContain("&lt;img");
});
it("keeps custom iframe content stable on heartbeat but updates changed data and clears denied reads",async()=>{
  const {htmlWidgetDocument}=await import("../src/server/executive/html-widget");
  const widget=definitionSchema.parse({...emptyDefinition(),widgets:[{id:"stable",title:"Live",metric:"headcount",kind:"html",html:"<h2>{{total}}</h2><time>{{updatedAt}}</time>"}]}).widgets[0];
  const data={metric:"headcount" as const,total:1,unit:"count" as const,currency:null,rows:[{label:"People",value:1}],definition:"Current people",scope:"Test company",revision:key(),updatedAt:1000};
  const first=htmlWidgetDocument(widget,data);
  expect(htmlWidgetDocument(widget,{...data,updatedAt:2000})).toBe(first);
  expect(htmlWidgetDocument(widget,{...data,total:2,rows:[{label:"People",value:2}],revision:key(),updatedAt:3000})).toContain("<h2>2</h2>");
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
