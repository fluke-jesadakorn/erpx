import { beforeAll, beforeEach, afterEach, expect, it, vi } from "vitest";
import { Client, StreamableHTTPClientTransport } from "@modelcontextprotocol/client";
import { env, applyMigrations } from "../../support/runtime-env";
import { expenseDb as db, expenseFixture, key } from "../../support/helpers/expense-fixture";
import { createExternalConnection, findExecutiveToken, listExternalConnections, resolveExecutivePrincipal, revokeExternalConnection } from "../../../src/server/modules/runtime/public/executive/connections";
import { boundedExecutiveResult, handleExecutiveMcp, metricProjection } from "../../../src/server/modules/runtime/public/executive/mcp";
import { createReport, reportData, saveReport, saveSchema, saveShare, revokeShare } from "../../../src/server/modules/runtime/public/executive/reports";
import * as metrics from "../../../src/server/modules/runtime/public/executive/metrics";
import { defaultFilters, emptyDefinition, type Breakdown, type Filters, type MetricData, type MetricId } from "../../../src/lib/shared/executive/canvas";
import { loadIamAuthority } from "../../../src/server/modules/identity_access/public/iam-store";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "../../../src/server/modules/runtime/adapters/runtime/types";
vi.mock("../../../src/server/modules/runtime/adapters/runtime",()=>({runtimeEnv:async()=>env,holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),holdingBindingBySlug:async()=>({id:"h001",slug:"test",displayName:"Test"})}));
let f:Awaited<ReturnType<typeof expenseFixture>>;
const originalQuery=metrics.queryMetric;
// MCP uses aggregate reads; narrow the overloaded API for correctly typed spies.
const metricReads:{queryMetric:(db:SqlDatabase,actor:WorkContext,metric:MetricId,filters:Filters,breakdown?:Breakdown)=>Promise<MetricData>}=metrics;
const clients:Client[]=[];
beforeAll(async()=>{
  await Promise.all([applyMigrations(db,env.STAFF_TEST_MIGRATIONS),applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS)]);
  await env.CONTROL_DB.prepare("INSERT INTO holding_bindings(holding_id,slug,display_name,write_binding,read_binding,files_binding) VALUES('h001','test','Test','H001_DB','H001_READ_DB','H001_FILES')").run();
});
beforeEach(async()=>{
  f=await expenseFixture();
  for(const a of [f.owner,f.reviewer,f.claimant])await env.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,"createdAt","updatedAt") VALUES(?,?,?,1,1)').bind(a.userId,a.name,`${a.userId}@example.test`).run();
});
afterEach(async()=>{vi.restoreAllMocks();await Promise.all(clients.splice(0).map(c=>c.close()));});
async function token(actor=f.owner,scopeId=actor.companyId) {
  return createExternalConnection(env.CONTROL_DB,db,actor,{name:"MCP test agent",scopeId},key());
}
async function client(secret:string) {
  const c=new Client({name:"erpx-team-test",version:"1.0.0"});clients.push(c);
  await c.connect(new StreamableHTTPClientTransport(new URL("http://localhost:3100/api/mcp/executive"),{authProvider:{token:async()=>secret},fetch:async(input,init)=>handleExecutiveMcp(new Request(input instanceof Request?input.url:String(input),init))}));
  return c;
}
async function rpc(secret:string,method="tools/list",params:unknown=undefined) {
  return handleExecutiveMcp(new Request("http://localhost:3100/api/mcp/executive",{method:"POST",headers:{Authorization:`Bearer ${secret}`,"Content-Type":"application/json",Accept:"application/json, text/event-stream"},body:JSON.stringify({jsonrpc:"2.0",id:1,method,...(params?{params}:{})})}));
}
function data(result:Awaited<ReturnType<Client["callTool"]>>) {
  if(result.structuredContent)return result.structuredContent as Record<string,unknown>;
  const content=result.content as {type:string;text?:string}[];
  return JSON.parse(content.find(c=>c.type==="text")!.text!) as Record<string,unknown>;
}
async function report() {
  const r=await createReport(db,f.owner,key(),"MCP operating overview");
  await saveReport(db,f.owner,r.id,saveSchema.parse({expectedVersion:1,filters:defaultFilters(),definition:{...emptyDefinition("MCP operating overview"),widgets:[{id:"people",title:"People",metric:"headcount",kind:"html",html:"<p>Private web markup</p>",css:"p{color:red}"}]}}),key());
  return r.id;
}
async function recipient() {
  await db.prepare("UPDATE assignments SET role='executive_author' WHERE id=?").bind(f.reviewer.assignmentId).run();
  await db.prepare("UPDATE hr_engagements SET level_id=? WHERE person_id=?").bind(`org-level-${f.company}-2`,`person-${f.reviewer.userId}`).run();
  return {...f.reviewer,role:"executive_author",iam:await loadIamAuthority(db,f.company,f.reviewer.userId,"executive_author")} satisfies WorkContext;
}
it("uses the official v2 client to discover exactly five read-only tools and matches Canvas metrics without private HTML or recipients",async()=>{
  const id=await report(),t=await token(),c=await client(t.secret),tools=await c.listTools();
  expect(tools.tools.map(t=>t.name).sort()).toEqual(["get_dashboard","get_dashboard_data","list_dashboards","list_metrics","query_metric"]);
  expect(tools.tools.every(t=>t.annotations?.readOnlyHint===true)).toBe(true);
  const library=data(await c.callTool({name:"list_dashboards",arguments:{}}));expect(library.dashboards).toEqual(expect.arrayContaining([expect.objectContaining({id})]));
  const definition=data(await c.callTool({name:"get_dashboard",arguments:{dashboardId:id}}));
  expect(JSON.stringify(definition)).not.toMatch(/Private web markup|recipientId|publisher_json|html.*<p/);
  const principal=await resolveExecutivePrincipal(env.CONTROL_DB,await findExecutiveToken(env.CONTROL_DB,`Bearer ${t.secret}`));
  const result=data(await c.callTool({name:"get_dashboard_data",arguments:{dashboardId:id}})),canvas=await reportData(db,principal.actor,id);
  expect((result.metrics as Record<string,MetricData>).headcount).toMatchObject({total:canvas.metrics.headcount.total,unit:canvas.metrics.headcount.unit,valueScale:canvas.metrics.headcount.valueScale,revision:canvas.metrics.headcount.revision,status:canvas.metrics.headcount.status,scopeBoundary:canvas.metrics.headcount.scopeBoundary,sourceReferences:expect.any(Array)});
  expect(result).not.toHaveProperty("documents");expect(result).not.toHaveProperty("shares");
  const listed=await listExternalConnections(env.CONTROL_DB,db,f.owner);expect(JSON.stringify(listed)).not.toContain(t.secret);expect(listed.connections[0].status).toBe("active");
  const persisted=await env.CONTROL_DB.prepare("SELECT secret_hash FROM executive_external_tokens WHERE id=?").bind(t.id).first<{secret_hash:string}>();expect(persisted?.secret_hash).toMatch(/^[a-f0-9]{64}$/);expect(persisted?.secret_hash).not.toContain(t.secret);
  await expect(revokeExternalConnection(env.CONTROL_DB,db,f.reviewer,t.id)).rejects.toMatchObject({code:"CONNECTION_NOT_FOUND"});
});
it("keeps finance totals above the safe integer range exact and rejects arbitrary SQL/company arguments",async()=>{
  const party=key(),now=Date.now(),exact="9007199254740993";
  await db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'customer',?,'MCP customer','THB',?,?)").bind(party,f.company,party,now,now).run();
  await db.prepare(`INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,status,created_by,created_at,updated_at)
    VALUES(?,?,?,'receivable',?,'2026-03-01','2026-03-31','THB',?,?,?,?,?,'posted',?,?,?)`).bind(key(),f.company,party,key(),exact,exact,exact,f.accounts.cash,f.accounts.revenue,f.owner.userId,now,now).run();
  const c=await client((await token()).secret),args={metric:"receivables",filters:defaultFilters()};
  const value=data(await c.callTool({name:"query_metric",arguments:args})).data as MetricData;expect(value.total).toBe(exact);expect(value.valueScale).toBe(2);expect(value.currency).toBe("THB");
  const invalid=await c.callTool({name:"query_metric",arguments:{...args,companyId:"foreign",sql:"SELECT * FROM user"}});expect(invalid.isError).toBe(true);
});
it("rejects expired and revoked credentials and attributes denial without storing the secret",async()=>{
  const expired=await token();await env.CONTROL_DB.prepare("UPDATE executive_external_tokens SET created_at=1,expires_at=2 WHERE id=?").bind(expired.id).run();expect((await rpc(expired.secret)).status).toBe(401);
  const revoked=await token();await revokeExternalConnection(env.CONTROL_DB,db,f.owner,revoked.id);expect((await rpc(revoked.secret)).status).toBe(401);
  expect((await rpc("not-a-token")).status).toBe(401);
  const audit=(await env.CONTROL_DB.prepare("SELECT * FROM executive_mcp_audit WHERE token_id IN (?,?) AND outcome='denied'").bind(expired.id,revoked.id).all()).results;
  expect(audit).toHaveLength(2);expect(audit.every(a=>a.outcome==="denied"&&a.user_id===f.owner.userId)).toBe(true);expect(JSON.stringify(audit)).not.toContain(revoked.secret);
});
it("rejects revoked assignments, suspended identities, and changed assignment scopes on every request",async()=>{
  const t=await token();
  await db.prepare("INSERT INTO iam_members(company_id,user_id,principal_id,status) VALUES(?,?,?,'suspended')").bind(f.company,f.owner.userId,`local:${f.owner.userId}`).run();expect((await rpc(t.secret)).status).toBe(403);
  await db.prepare("UPDATE iam_members SET status='active' WHERE company_id=? AND user_id=?").bind(f.company,f.owner.userId).run();
  await db.prepare("UPDATE assignments SET version=version+1 WHERE id=?").bind(f.owner.assignmentId).run();expect((await rpc(t.secret)).status).toBe(403);
  const t2=await token();await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(),f.owner.assignmentId).run();expect((await rpc(t2.secret)).status).toBe(403);
});
it("checks executive eligibility and current IAM instead of treating a share as a KPI grant",async()=>{
  await expect(token(f.claimant)).rejects.toMatchObject({code:"EXECUTIVE_CONNECTION_FORBIDDEN"});
  const reader=await recipient(),id=await report();
  await saveShare(db,f.owner,id,{recipientId:reader.userId,mode:"distribution",role:"viewer",widgetIds:["people"],expiresAt:null,filters:{from:"2026-03-01",to:"2026-03-31",currency:"THB"}},key());
  const c=await client((await token(reader)).secret);
  const result=data(await c.callTool({name:"get_dashboard_data",arguments:{dashboardId:id,filters:defaultFilters()}}));
  expect(result.filters).toEqual({from:"2026-03-01",to:"2026-03-31",currency:"THB"});expect(result.mode).toBe("distribution");expect((result.metrics as Record<string,MetricData>).headcount.status).toMatch(/ready|empty/);
  expect((await c.callTool({name:"query_metric",arguments:{metric:"headcount",filters:defaultFilters()}})).isError).toBe(true);
  const catalog=data(await c.callTool({name:"list_metrics",arguments:{}})).metrics as {id:string;available:boolean}[];expect(catalog.find(m=>m.id==="headcount")?.available).toBe(false);
});
it("isolates company, branch and department scopes and denies unshared dashboards",async()=>{
  const id=await report(),other=await expenseFixture();
  const t=await token(f.owner,f.department),c=await client(t.secret);
  const p=await resolveExecutivePrincipal(env.CONTROL_DB,await findExecutiveToken(env.CONTROL_DB,`Bearer ${t.secret}`));
  expect(p.actor).toMatchObject({companyId:f.company,branchId:f.branch,departmentId:f.department});
  const result=data(await c.callTool({name:"query_metric",arguments:{metric:"headcount",filters:defaultFilters()}})).data as MetricData;
  expect(result.total).toBe((await originalQuery(db,p.actor,"headcount",defaultFilters())).total);expect(result.scopeBoundary.level).toBe("department");
  const foreign=await createReport(db,other.owner,key(),"Foreign dashboard");expect((await c.callTool({name:"get_dashboard",arguments:{dashboardId:foreign.id}})).isError).toBe(true);
  await expect(token(f.owner,other.branch)).rejects.toMatchObject({code:"CONNECTION_SCOPE_FORBIDDEN"});
  const r=await recipient(),reader=await client((await token(r)).secret);expect((await reader.callTool({name:"get_dashboard",arguments:{dashboardId:id}})).isError).toBe(true);
  const branch=await token(f.owner,f.branch);const bp=await resolveExecutivePrincipal(env.CONTROL_DB,await findExecutiveToken(env.CONTROL_DB,`Bearer ${branch.secret}`));expect(bp.actor.branchId).toBe(f.branch);expect(bp.actor.departmentId).toBeNull();
});
it("drops data if a token or assignment is revoked during the metric read",async()=>{
  for(const kind of ["token","assignment"] as const) {
    const t=await token(),c=await client(t.secret);
    vi.spyOn(metricReads,"queryMetric").mockImplementationOnce(async(d,a,id,filters,breakdown)=>{
      const result=await originalQuery(d,a,id,filters,breakdown);
      if(kind==="token")await revokeExternalConnection(env.CONTROL_DB,db,f.owner,t.id);
      else await db.prepare("UPDATE assignments SET version=version+1 WHERE id=?").bind(f.owner.assignmentId).run();
      return result;
    });
    const result=await c.callTool({name:"query_metric",arguments:{metric:"headcount",filters:defaultFilters()}}).catch(()=>null);
    if(result){expect(result.isError).toBe(true);expect(result).not.toHaveProperty("structuredContent");}
    vi.restoreAllMocks();
  }
});
it("drops distributed data if a share is withdrawn between the source read and response",async()=>{
  const reader=await recipient(),id=await report(),saved=await saveShare(db,f.owner,id,{recipientId:reader.userId,mode:"distribution",role:"viewer",widgetIds:["people"],expiresAt:null,filters:defaultFilters()},key());
  const c=await client((await token(reader)).secret);
  vi.spyOn(metricReads,"queryMetric").mockImplementationOnce(async(d,a,m,filters,breakdown)=>{const result=await originalQuery(d,a,m,filters,breakdown);await revokeShare(db,f.owner,id,saved.id,key());return result;});
  const result=await c.callTool({name:"get_dashboard_data",arguments:{dashboardId:id}});expect(result.isError).toBe(true);expect(result).not.toHaveProperty("structuredContent");
});
it("limits large requests, rejects cross-origin requests, and atomically rate-limits concurrent calls",async()=>{
  const t=await token();
  const large=await handleExecutiveMcp(new Request("http://localhost:3100/api/mcp/executive",{method:"POST",headers:{Authorization:`Bearer ${t.secret}`,"Content-Type":"application/json"},body:" ".repeat(33000)}));expect(large.status).toBe(413);
  const cross=await handleExecutiveMcp(new Request("http://localhost:3100/api/mcp/executive",{method:"POST",headers:{Origin:"https://foreign.invalid",Authorization:`Bearer ${t.secret}`}}));expect(cross.status).toBe(403);
  await env.CONTROL_DB.prepare("INSERT INTO executive_mcp_rate_limits(user_id,company_id,window_start,requests) VALUES(?,?,?,59) ON CONFLICT(user_id,company_id) DO UPDATE SET window_start=excluded.window_start,requests=59").bind(f.owner.userId,f.company,Math.floor(Date.now()/60000)*60000).run();
  const responses=await Promise.all([rpc(t.secret),rpc(t.secret)]);expect(responses.map(r=>r.status).sort()).toEqual([200,429]);expect(responses.find(r=>r.status===429)?.headers.has("Retry-After")).toBe(true);
});
it("returns bounded rows with complete totals and explicit truncation",()=>{
  const base=metrics.metricUnavailable("headcount","pending","unavailable");
  const fixture={...base,total:"9007199254740993",status:"ready" as const,rows:Array.from({length:400},(_,i)=>({label:`Row ${i} ${"x".repeat(700)}`,value:"9007199254740993"}))};
  const result=boundedExecutiveResult({data:metricProjection(fixture,200)}),m=result.data as ReturnType<typeof metricProjection>;
  expect(Buffer.byteLength(JSON.stringify(result))).toBeLessThan(131072+512);expect(m.total).toBe(fixture.total);expect(m.rows.length).toBeLessThan(200);expect(m.pagination.truncated).toBe(true);expect(m.pagination.totalRows).toBe(400);
  expect(metricProjection(base,100).valueAvailable).toBe(false);
});
it("paginates authorized KPI groups and retains catalogue acceptance flags",async()=>{
  const c=await client((await token()).secret),args={metric:"headcount",filters:defaultFilters(),breakdown:"level",limit:1};
  const first=data(await c.callTool({name:"query_metric",arguments:args})).data as ReturnType<typeof metricProjection>;
  expect(first.rows).toHaveLength(1);expect(first.pagination.truncated).toBe(true);
  const second=data(await c.callTool({name:"query_metric",arguments:{...args,cursor:first.pagination.nextCursor}})).data as ReturnType<typeof metricProjection>;
  expect(second.rows).toHaveLength(1);expect(second.rows[0].label).not.toBe(first.rows[0].label);expect(second.total).toBe(first.total);expect(second.revision).toBe(first.revision);
  const catalog=data(await c.callTool({name:"list_metrics",arguments:{}})).metrics as {id:string;acceptancePending:boolean}[];
  expect(catalog.find(m=>m.id==="active-legal-obligations")?.acceptancePending).toBe(true);
});
it("creates at most one credential for an idempotency key and never replays its secret",async()=>{
  const requestKey=key(),input={name:"Idempotent connection",scopeId:f.company};
  const issued=await createExternalConnection(env.CONTROL_DB,db,f.owner,input,requestKey);
  await expect(createExternalConnection(env.CONTROL_DB,db,f.owner,input,requestKey)).rejects.toMatchObject({code:"CONNECTION_ALREADY_CREATED"});
  const count=await env.CONTROL_DB.prepare("SELECT count(*)::integer count FROM executive_external_tokens WHERE id=?").bind(issued.id).first<{count:number}>();expect(count?.count).toBe(1);
});
it("keeps recipient reads unavailable and reloads changed IAM permissions and executive eligibility",async()=>{
  const r=await recipient(),id=await report(),t=await token(r),c=await client(t.secret);
  await saveShare(db,f.owner,id,{recipientId:r.userId,mode:"recipient",role:"viewer",widgetIds:["people"],expiresAt:null,filters:defaultFilters()},key());
  const value=(data(await c.callTool({name:"get_dashboard_data",arguments:{dashboardId:id}})).metrics as Record<string,MetricData&{valueAvailable:boolean}>).headcount;
  expect(value.status).toBe("unavailable");expect(value.valueAvailable).toBe(false);
  await db.prepare("UPDATE hr_engagements SET level_id=? WHERE person_id=?").bind(`org-level-${f.company}-3`,`person-${r.userId}`).run();expect((await rpc(t.secret)).status).toBe(403);
  await db.prepare("UPDATE hr_engagements SET level_id=? WHERE person_id=?").bind(`org-level-${f.company}-2`,`person-${r.userId}`).run();
  await db.prepare("UPDATE iam_roles SET status='archived' WHERE id='executive_author' AND company_id='*'").run();expect((await rpc(t.secret)).status).toBe(403);
  await db.prepare("UPDATE iam_roles SET status='active' WHERE id='executive_author' AND company_id='*'").run();
});
it("fails closed when the control database or audit persistence is unavailable",async()=>{
  const t=await token(),prepare=env.CONTROL_DB.prepare.bind(env.CONTROL_DB);
  vi.spyOn(env.CONTROL_DB,"prepare").mockImplementation(sql=>{if(sql.includes("executive_mcp_audit"))throw Error("private db error");return prepare(sql);});
  const response=await rpc(t.secret);expect(response.status).toBe(503);expect(await response.text()).not.toContain("private db error");
  vi.restoreAllMocks();vi.spyOn(env.CONTROL_DB,"prepare").mockImplementation(()=>{throw Error("database unavailable");});expect((await rpc(t.secret)).status).toBe(503);
});
it("serves multiple users concurrently without mixing principals or scopes",async()=>{
  const reader=await recipient(),a=await client((await token()).secret),b=await client((await token(reader)).secret);
  const [owner,executive]=await Promise.all([a.callTool({name:"list_metrics",arguments:{}}),b.callTool({name:"list_metrics",arguments:{}})]);
  const allowed=(r:typeof owner)=>(data(r).metrics as {id:string;available:boolean}[]).find(m=>m.id==="headcount")?.available;
  expect(allowed(owner)).toBe(true);expect(allowed(executive)).toBe(false);
});
it("rolls back a connection and its final control audit together, then retries once without replaying its secret",async()=>{
 const requestKey=key(),input={name:"Atomic control connection",scopeId:f.company},fault=`executive_connection_final_audit_${key().replaceAll("-","")}`;
 const snapshot=async()=>({tokens:(await env.CONTROL_DB.prepare("SELECT * FROM executive_external_tokens WHERE company_id=? ORDER BY id").bind(f.company).all()).results,audit:(await env.CONTROL_DB.prepare("SELECT * FROM executive_mcp_audit WHERE company_id=? ORDER BY id").bind(f.company).all()).results});
 const before=await snapshot();await env.CONTROL_DB.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'EXECUTIVE_CONNECTION_FINAL_AUDIT' USING ERRCODE='P0001';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON executive_mcp_audit FOR EACH ROW WHEN(NEW.token_id='${requestKey}' AND NEW.action='connection.create') EXECUTE FUNCTION ${fault}();`);
 try{await expect(createExternalConnection(env.CONTROL_DB,db,f.owner,input,requestKey)).rejects.toThrow("EXECUTIVE_CONNECTION_FINAL_AUDIT");expect(await snapshot()).toEqual(before);}finally{await env.CONTROL_DB.exec(`DROP TRIGGER ${fault} ON executive_mcp_audit;DROP FUNCTION ${fault}();`);}
 const issued=await createExternalConnection(env.CONTROL_DB,db,f.owner,input,requestKey);expect(issued.id).toBe(requestKey);await expect(createExternalConnection(env.CONTROL_DB,db,f.owner,input,requestKey)).rejects.toMatchObject({code:"CONNECTION_ALREADY_CREATED"});
 expect(await env.CONTROL_DB.prepare("SELECT count(*)::integer count FROM executive_external_tokens WHERE id=?").bind(requestKey).first("count")).toBe(1);expect(await env.CONTROL_DB.prepare("SELECT count(*)::integer count FROM executive_mcp_audit WHERE token_id=? AND action='connection.create'").bind(requestKey).first("count")).toBe(1);
});
