import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { env, applyMigrations } from "./runtime-env";
import { seedExpenseCompany } from "../fixtures/expenses/company";
import type { WorkContext } from "../src/server/authorization";
import { GET } from "../src/app/api/v1/holdings/[holdingSlug]/people/hr-settings/route";
import { administerLeave } from "../src/server/leave-self-service";
import { calendarData } from "../src/server/calendar";
import { addDays, rosterMonday } from "../src/lib/people-roster";

const state = vi.hoisted(()=>({actor:null as WorkContext|null}));
vi.mock("../src/server/authorization",()=>({requireWorkContext:async()=>state.actor!}));
vi.mock("../src/server/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
const db=env.H001_DB, context={params:Promise.resolve({holdingSlug:"test"})}, key=()=>crypto.randomUUID();
let fixture:Awaited<ReturnType<typeof seedExpenseCompany>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{fixture=await seedExpenseCompany(db,env.H001_FILES);state.actor=fixture.owner;});
const read = (query="") => GET(new Request(`http://localhost:3100/api/v1/holdings/test/people/hr-settings${query?`?${query}`:""}`),context);
const engagement = () => `eng-${fixture.claimant.userId}`;

async function policyWithHoliday() {
  const type=await administerLeave(fixture.owner,{resource:"type",code:"PERSONAL",name:"Personal leave",description:"",tracked:true,evidenceAfterUnits:null,evidenceNote:""},key());
  const policy=await administerLeave(fixture.owner,{resource:"policy",policyKey:`policy-${key()}`,name:"Five-day policy",version:1,effectiveFrom:"2020-01-01",effectiveTo:null,timezone:"Asia/Bangkok",workweek:[1,2,3,4,5],allowNegative:false,rules:[{leaveTypeId:type.id,grantKind:"annual",grantUnits:12,accrualUnits:0,accrualPeriod:null,carryoverLimit:null,employeeEligible:true,contractorEligible:false,evidencePurpose:null,evidenceMinUnits:null,evidenceMinCount:0,evidenceDueStage:"before_final_approval",evidenceGraceDays:0,evidenceNote:""}]},key());
  const calendar=await administerLeave(fixture.owner,{resource:"calendar",name:"Bangkok holidays",timezone:"Asia/Bangkok"},key());
  const date=addDays(rosterMonday(),14);
  await administerLeave(fixture.owner,{resource:"holiday",calendarId:calendar.id,date,name:"Team holiday"},key());
  await administerLeave(fixture.owner,{resource:"assignment",engagementId:engagement(),policyVersionId:policy.id,calendarId:calendar.id,effectiveFrom:"2020-01-01",effectiveTo:null},key());
  return {date,policy,calendar};
}

it("loads only authorized settings topics and protects staff policy details",async()=>{
  const editor=await read("editor=person");
  expect(editor.status).toBe(200);
  expect((await editor.json()).data.editorNodes.some((node:{id:string})=>node.id===fixture.department)).toBe(true);
  state.actor=fixture.claimant;
  const index=await read();
  expect(index.status).toBe(200);
  expect((await index.json()).data.topics).toEqual([]);
  for (const query of ["topic=leave","topic=levels",`engagement=${engagement()}`,`impact=${engagement()}`,"editor=person"]) {
    const response=await read(query);
    expect(response.status).toBe(403);
  }
  state.actor=await fixture.member("pay-reader","payroll_preparer");
  const topics=(await (await read()).json()).data.topics;
  expect(topics).toContain("pay-groups");
  expect(topics).not.toContain("leave");
  expect((await read(`engagement=${engagement()}`)).status).toBe(403);
});

it("returns current policy and holidays without compensation or private contacts",async()=>{
  await policyWithHoliday();
  await db.prepare("UPDATE hr_people SET email='private@example.test',phone='private-phone' WHERE id=?").bind(`person-${fixture.claimant.userId}`).run();
  const response=await read(`engagement=${engagement()}`);
  expect(response.status).toBe(200);
  expect(response.headers.get("Cache-Control")).toBe("private, no-store");
  const payload=await response.json();
  expect(payload.data.assignments[0]).toMatchObject({policy:"Five-day policy",calendar:"Bangkok holidays",workweek_json:[1,2,3,4,5]});
  expect(JSON.stringify(payload)).not.toContain("private@example.test");
  expect(JSON.stringify(payload)).not.toContain("private-phone");
  expect(JSON.stringify(payload)).not.toContain("base_amount_minor");
  state.actor={...fixture.owner,branchId:fixture.branch,departmentId:fixture.accountsDepartment};
  expect((await read(`engagement=${engagement()}`)).status).toBe(404);
  expect((await read("topic=leave")).status).toBe(403);
});

it("shows assigned holidays from the same Calendar adapter and honors department, person and assignment dates",async()=>{
  const {date,calendar}=await policyWithHoliday();
  const before=await db.prepare("SELECT count(*)::int total FROM command_receipts").first();
  const query={date,view:"week",mode:"team",layers:"leave",node:fixture.department};
  const result=await calendarData(db,fixture.owner,query);
  const holiday=result.items.find(item=>item.kind==="holiday");
  expect(holiday).toMatchObject({title:"Team holiday",description:"Bangkok holidays",startDate:date,endDate:addDays(date,1),participants:[{id:`person-${fixture.claimant.userId}`,name:"claimant"}]});
  expect((await calendarData(db,fixture.owner,{...query,node:fixture.accountsDepartment})).items.some(item=>item.kind==="holiday")).toBe(false);
  expect((await calendarData(db,fixture.claimant,{...query,mode:"mine"})).items.some(item=>item.kind==="holiday")).toBe(true);
  expect((await calendarData(db,fixture.reviewer,{date,view:"week",mode:"mine",layers:"leave"})).items.some(item=>item.kind==="holiday")).toBe(false);
  await db.prepare("UPDATE leave_policy_assignments SET effective_to=? WHERE holiday_calendar_id=? AND engagement_id=?").bind(addDays(date,-1),calendar.id,engagement()).run();
  expect((await calendarData(db,fixture.owner,query)).items.some(item=>item.kind==="holiday")).toBe(false);
  expect(await db.prepare("SELECT count(*)::int total FROM command_receipts").first()).toEqual(before);
});

it("summarizes move impacts while enforcing compensation permissions and selected Company",async()=>{
  await policyWithHoliday();
  state.actor=fixture.head;
  const response=await read(`impact=${engagement()}`);
  expect(response.status).toBe(200);
  const hr=(await response.json()).data.people[0];
  expect(hr).toMatchObject({engagementId:engagement(),name:"claimant",policy:"Five-day policy",calendar:"Bangkok holidays",upcomingShifts:0,upcomingLeave:0});
  expect(hr).not.toHaveProperty("payGroup");
  state.actor=fixture.owner;
  expect((await (await read(`impact=${engagement()}`)).json()).data.people[0]).toHaveProperty("payGroup",null);
  expect((await read(`impact=${engagement()},another-company-engagement`)).status).toBe(404);
  expect((await read(`topic=leave&engagement=${engagement()}`)).status).toBe(422);
});
