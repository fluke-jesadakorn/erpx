import { previewServiceTeam,serviceTeamSchema } from "../src/server/service-teams";
import { beforeAll,beforeEach,expect,it,vi } from "vitest";
import { applyMigrations,env } from "./runtime-env";
import { expenseFixture,expenseDb as db,key } from "./helpers/expense-fixture";
import { employeeRequestSchema,previewEmployeeRequest,saveEmployeeRequest } from "../src/server/employee-requests";
import { resolveFlow } from "../src/server/flow-routing";
import { workReports,workReportCsv } from "../src/server/work-reports";
import { operationFromFlow } from "../src/lib/operation-presentation";
vi.mock("../src/server/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();});
function purchase(){return employeeRequestSchema.parse({kind:"purchase",title:"Laptop",description:"Replace the damaged workstation",location:"Office",branchId:f.branch,departmentId:f.department,currency:"THB",items:[{description:"Laptop",quantity:"1",estimatedCost:"100"}]});}
it("previews the owner route without saving a request or approval cycle",async()=>{
 const before=await db.prepare("SELECT COUNT(*) count FROM employee_requests WHERE company_id=?").bind(f.company).first<{count:number}>();
 const preview=await previewEmployeeRequest(db,f.claimant,purchase());
 expect(preview.steps.length).toBeGreaterThan(0);expect(preview.blockers).toEqual([]);
 const after=await db.prepare("SELECT COUNT(*) count FROM employee_requests WHERE company_id=?").bind(f.company).first<{count:number}>();expect(after).toEqual(before);
 const uncovered=await previewEmployeeRequest(db,f.claimant,employeeRequestSchema.parse({...purchase(),kind:"service",category:"housekeeping",title:"",description:"",location:""}));
 expect(uncovered.blockers.length).toBeGreaterThan(0);
 expect(uncovered.blockers[0]).toContain("coverage");
 await expect(previewEmployeeRequest(db,{...f.claimant,companyId:key()},purchase())).rejects.toMatchObject({status:403});
});
it("keeps a legacy approved expense business status while awaiting GL posting, without new decisions",async()=>{
 const id=await f.claim();await db.prepare("UPDATE finance_expenses SET workflow_version=1,status='approved',workflow_stage='draft' WHERE id=?").bind(id).run();
 const flow=await resolveFlow(db,f.owner,"expense",id);
 expect(flow.state).toBe("approved");expect(flow.current?.key).toBe("gl_posting");expect(operationFromFlow(flow).actionKind).toBe("work");
 expect((await db.prepare("SELECT COUNT(*) count FROM approval_decisions d JOIN approval_instances i ON i.id=d.instance_id WHERE i.record_id=?").bind(id).first<{count:number}>())?.count).toBe(0);
});
it("aggregates and paginates over 150 accessible records, with the same scoped CSV and hidden records excluded",async()=>{
 const saved=await saveEmployeeRequest(db,f.claimant,null,purchase(),key());
 await db.prepare(`INSERT INTO employee_requests SELECT (jsonb_populate_record(NULL::employee_requests,to_jsonb(r)||jsonb_build_object('id',r.id||'-copy-'||n))).* FROM employee_requests r CROSS JOIN generate_series(1,150) n WHERE r.id=?`).bind(saved.id).run();
 const first=await workReports(f.claimant,{mode:"drafts",type:"purchase"});
 expect(first.total).toBe(151);expect(first.rows).toHaveLength(50);expect(first.allRows).toEqual([]);
 const last=await workReports(f.claimant,{mode:"drafts",type:"purchase",page:4},true);
 expect(last.rows).toHaveLength(1);expect(last.allRows).toHaveLength(151);
 expect(new Set([...first.rows,...last.rows].map(r=>r.id)).size).toBe(51);
 expect(workReportCsv(last.allRows).split("\r\n")).toHaveLength(152);
 const unrelated=await f.member("report-outsider","employee",1,f.department);
 const hidden=await workReports(unrelated,{mode:"drafts",type:"purchase"},true);expect(hidden.total).toBe(0);expect(hidden.allRows).toEqual([]);expect(hidden.options.requesters).toEqual([]);
 const csv=workReportCsv([{...last.rows[0],title:"=HYPERLINK(\"https://example.invalid\")"}]);expect(csv).toContain("'=HYPERLINK");
},120000);

it("previews service coverage with owner eligibility and does not publish a revision",async()=>{
 const input=serviceTeamSchema.parse({name:"Test service coverage",members:[{userId:f.it.userId,responsibility:"lead"}],coverage:[{category:"it",scopeKind:"holding",scopeNodeId:f.company}]});
 const before=await db.prepare("SELECT COUNT(*) count FROM service_team_revisions WHERE company_id=?").bind(f.company).first();
 const preview=await previewServiceTeam(db,f.owner,null,input);expect(preview.newVersion).toBe(1);
 expect(await db.prepare("SELECT COUNT(*) count FROM service_team_revisions WHERE company_id=?").bind(f.company).first()).toEqual(before);
 await expect(previewServiceTeam(db,f.claimant,null,input)).rejects.toMatchObject({status:403});
 await expect(previewServiceTeam(db,f.owner,null,{...input,members:[]})).rejects.toMatchObject({status:422});
});
