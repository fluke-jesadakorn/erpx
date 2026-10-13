import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import { expenseFixture, expenseDb as db, key } from "../../support/helpers/expense-fixture";
import { getWorkReview } from "../../../src/server/modules/work_projection/public/work-review";
import { workReviewCommand } from "../../../src/lib/work_projection/work-review";
import { expenseDecisionSchema, transitionExpense } from "../../../src/server/modules/people_payroll/public/expenses";
import { employeeRequestSchema, requestActionSchema, saveEmployeeRequest, actOnEmployeeRequest, getEmployeeRequest } from "../../../src/server/modules/records/public/employee-requests";
import { saveTask, taskSchema, transitionTask, taskActionSchema } from "../../../src/server/modules/operations/public/staff-tasks";
import { createLeave, submitLeave, leaveDraftSchema } from "../../../src/server/modules/people_payroll/public/leave";
import * as flowRouting from "../../../src/server/modules/workflow/public/flow-routing";

vi.mock("../../../src/server/modules/runtime/adapters/runtime",()=>({holdingResources:async()=>({write:env.H001_DB,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f: Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();});

it("reads the recorded independent expense route and uses an idempotent owner command",async()=>{
  const id=await f.claim();await f.attach(id);await f.action(f.claimant,id,"submit");
  const review=await getWorkReview(f.head,"expense",id);
  expect(review.source).toMatchObject({kind:"expense",id,version:review.version});
  expect(review.requester).toBeTruthy();expect(review.evidence).toHaveLength(1);
  expect(review.permittedActions).toEqual(expect.arrayContaining(["approve","reject"]));
  expect((await getWorkReview(f.claimant,"expense",id)).permittedActions).toEqual([]);
  const command=workReviewCommand("expense",id,review.version,"approve","Reviewed original evidence"), receipt=key();
  const input=expenseDecisionSchema.parse(command.body);
  await expect(transitionExpense(f.head,id,"approve",{...input,expectedVersion:review.version-1},key())).rejects.toMatchObject({status:409});
  const result=await transitionExpense(f.head,id,"approve",input,receipt);
  expect(await transitionExpense(f.head,id,"approve",input,receipt)).toEqual(result);
  await expect(transitionExpense(f.head,id,"approve",input,key())).rejects.toMatchObject({status:403});
  expect((await getWorkReview(f.head,"expense",id)).permittedActions).toEqual([]);
  expect((await getWorkReview(f.reviewer,"expense",id)).fullReviewReason).toContain("accounting");
});

it("requires a fresh review if the record changes between detail and route reads",async()=>{
  const id=await f.claim();await f.attach(id);await f.action(f.claimant,id,"submit");
  const resolve=flowRouting.resolveFlow;
  const changed=vi.spyOn(flowRouting,"resolveFlow").mockImplementation(async(...args)=>{const result=await resolve(...args);return {...result,recordVersion:result.recordVersion+1};});
  try {await expect(getWorkReview(f.head,"expense",id)).rejects.toMatchObject({status:409,code:"VERSION_CONFLICT"});}
  finally {changed.mockRestore();}
});

it("does not disclose expense evidence across companies or to an unrelated employee",async()=>{
  const id=await f.claim();await f.attach(id);await f.action(f.claimant,id,"submit");
  await expect(getWorkReview({...f.head,companyId:`other-${key()}`},"expense",id)).rejects.toMatchObject({status:404});
  const unrelated=await f.member("unrelated","employee",1,f.department);
  await expect(getWorkReview(unrelated,"expense",id)).rejects.toMatchObject({status:404});
});

it("shows purchase return authority and keeps accounting preparation in the full source",async()=>{
  const saved=await saveEmployeeRequest(db,f.claimant,null,employeeRequestSchema.parse({kind:"purchase",title:"Replace workstation",description:"A replacement for daily work",location:"Office",branchId:f.branch,departmentId:f.department,currency:"THB",items:[{description:"Laptop",quantity:"1",estimatedCost:"100"}]}),key());
  const id=saved.id;
  await actOnEmployeeRequest(db,f.claimant,id,requestActionSchema.parse({action:"submit",expectedVersion:1,note:"Ready for review"}),key());
  const review=await getWorkReview(f.head,"purchase_request",id);
  expect(review.permittedActions).toContain("approve");expect(review.permittedActions).toContain("return");
  expect(review.steps.some(step=>step.person===f.head.userId)).toBe(false);
  await expect(getWorkReview(f.head,"equipment_request",id)).rejects.toMatchObject({status:404});
  const command=workReviewCommand("purchase_request",id,review.version,"approve","Need confirmed");
  await actOnEmployeeRequest(db,f.head,id,requestActionSchema.parse(command.body),key());
  const row=await getEmployeeRequest(db,f.reviewer,id);
  expect(row.version).toBeGreaterThan(review.version);
  const preparation=await getWorkReview(f.reviewer,"purchase_request",id);
  expect(preparation.permittedActions).toEqual([]);expect(preparation.fullReviewReason).toContain("accounting");
});

it("uses task authority for a start review without granting the assignee self review",async()=>{
  const saved=await saveTask(db,f.claimant,null,taskSchema.parse({title:"Inspect deliveries",assigneeUserId:f.it.userId,startApprovers:[f.head.userId],dueAt:"2026-10-15T02:00:00Z"}),key());
  await transitionTask(db,f.claimant,saved.id,taskActionSchema.parse({action:"submit",expectedVersion:1,note:"Ready"}),key());
  expect((await getWorkReview(f.head,"staff_task",saved.id)).permittedActions).toContain("approve");
  expect((await getWorkReview(f.it,"staff_task",saved.id)).permittedActions).toEqual([]);
});

it("reads a leave review through the leave owner and preserves its source version",async()=>{
  await f.member("hr-reviewer","hr_admin",10,f.department);
  const saved=await createLeave(f.claimant,{startDate:"2026-10-15",days:1,handoverNote:"Team handover"},key());
  const id=String(saved.id);
  await submitLeave("h001",id,leaveDraftSchema.parse({startDate:"2026-10-15",days:1,handoverNote:"Team handover",expectedVersion:1}),key(),f.claimant);
  const review=await getWorkReview(f.head,"leave",id);
  expect(review.source.href).toContain(`/tasks/people_payroll/${id}`);
  expect(review.source.version).toBe(review.version);
  expect(review.permittedActions).toContain("approve");
});
