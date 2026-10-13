import { beforeAll,beforeEach,expect,it,vi } from "vitest";
import { env,applyMigrations } from "../../support/runtime-env";
import { expenseFixture,expenseDb as db,key } from "../../support/helpers/expense-fixture";
import { expenseRoute } from "../../../src/server/modules/people_payroll/public/expense-routing";
import { getExpense } from "../../../src/server/modules/people_payroll/public/expenses";
import { approvalView } from "../../../src/server/modules/workflow/public/approvals";
import { saveSpendingTemplate } from "../../../src/server/modules/people_payroll/public/spending-templates";
import { saveEmployeeRequest,employeeRequestSchema,actOnEmployeeRequest,requestActionSchema,getEmployeeRequest } from "../../../src/server/modules/records/public/employee-requests";
import { resolveFlow } from "../../../src/server/modules/workflow/public/flow-routing";
import { approvalAdministrationData } from "../../../src/server/modules/workflow/public/approval-administration";
vi.mock("../../../src/server/modules/runtime/adapters/runtime",()=>({holdingResources:async()=>({write:db,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();});
it("keeps approval administration subject to Company authority and original record access",async()=>{
 const id=await f.claim(f.owner),privateId=await f.claim();
 const visible=await approvalAdministrationData(f.owner,'expense',id,db);
 expect(visible?.options.canManage).toBe(true);
 expect(visible?.rows).toMatchObject([{record_type:'expense',record_id:id,version:1,destination:`expenses/${id}`}]);
 expect(await approvalAdministrationData(f.claimant,'expense',id,db)).toBeNull();
 expect((await approvalAdministrationData(f.owner,'expense',privateId,db))?.rows).toEqual([]);
 const foreign=await expenseFixture();
 expect((await approvalAdministrationData(foreign.owner,'expense',id,db))?.rows).toEqual([]);
 expect(await db.prepare('SELECT company_id,version FROM finance_expenses WHERE id=?').bind(id).first()).toMatchObject({company_id:f.company,version:1});
});
it("filters beta approval administration before exposing non-expense business records",async()=>{
 const id=await f.claim(f.owner);
 expect(await approvalAdministrationData({...f.owner,beta:true},'leave',id,db)).toBeNull();
 expect(await approvalAdministrationData(f.owner,'legal',id,db)).toBeNull();
 expect((await approvalAdministrationData({...f.owner,beta:true},'expense',id,db))?.rows.map(row=>row.record_id)).toEqual([id]);
});
it("resolves exactly the nearest higher reporting supervisor, never a same-level peer",async()=>{
  const route=await expenseRoute(db,f.company,f.claimant.userId);
  expect(route.blockedReason).toBeNull();expect(route.steps.map(s=>s.stage)).toEqual(["department_approval","accounts_review","accounts_approval"]);
  expect(route.steps[0].candidateUserIds).toEqual([f.head.userId]);
  expect(route.steps[2].snapshot).toMatchObject({spendingPolicy:1,templateVersion:1,requesterLevel:6});
});
it("requires a department route even for a CEO request",async()=>{
  await db.prepare("UPDATE hr_engagements SET department_id=NULL WHERE person_id=?").bind(`person-${f.owner.userId}`).run();
  const id=await f.claim(f.owner);await f.attach(id);
  await expect(f.action(f.owner,id,"submit")).rejects.toMatchObject({code:"EXPENSE_ROUTE_REQUIRED"});
  expect((await expenseRoute(db,f.company,f.owner.userId)).blockedReason).toContain("department");
});
it("routes manager, Accounting and Finance requesters through independent participants",async()=>{
  expect((await expenseRoute(db,f.company,f.head.userId)).steps[0].userId).toBe(f.owner.userId);
  await f.grant(f.tie,"finance_preparer");await f.grant(f.tie,"treasury");
  for(const requester of [f.reviewer,f.payer]){
    await db.prepare("UPDATE hr_engagements SET department_id=? WHERE person_id=?").bind(f.department,`person-${requester.userId}`).run();
    const route=await expenseRoute(db,f.company,requester.userId);
    expect(route.blockedReason).toBeNull();expect(route.steps[0].userId).toBe(f.head.userId);
    expect(route.steps.flatMap(s=>s.candidateUserIds)).not.toContain(requester.userId);
  }
});
it("blocks pending decisions after a manager departure until the reporting data is repaired",async()=>{
  const id=await f.claim();await f.attach(id);await f.action(f.claimant,id,"submit");
  await db.prepare("UPDATE hr_engagements SET status='ended' WHERE person_id=?").bind(`person-${f.head.userId}`).run();
  const view=await approvalView(db,"expense",id,f.owner);expect(view.canDecide).toBe(false);expect(view.eligibleApproverIds).toEqual([]);
});
it("records a supplier payment request with its selected payee through posting",async()=>{
  const {saveExpense,expenseUpdateSchema,postExpense}=await import("../../../src/server/modules/people_payroll/public/expenses");
  const supplier=`supplier-${key()}`;
  await db.prepare("INSERT INTO finance_parties(id,company_id,kind,code,name,currency,created_at,updated_at) VALUES(?,?,'supplier',?,'Supplier','THB',1,1)").bind(supplier,f.company,supplier).run();
  const id=String((await saveExpense(f.claimant,null,expenseUpdateSchema.parse({expectedVersion:1,requestKind:"payment",payeePartyId:supplier,expenseDate:"2026-09-01",merchant:"Supplier",receiptReference:"Supplier payment request",businessPurpose:"Pay supplier for service",currency:"THB",amount:"100",tax:"7",categoryAccountId:f.accounts.expense}),key())).id);
  await f.throughAccounts(id);await f.action(f.manager,id,"approve");
  const row=await getExpense(f.manager,id);await postExpense(f.manager,id,{expectedVersion:Number(row.version),periodId:f.period,entryDate:"2026-09-02"},key());
  expect(await db.prepare("SELECT party_id FROM finance_invoices WHERE source_id=?").bind(id).first("party_id")).toBe(supplier);
});
it("allows Accounting Level 4 and rejects Level 5 even with the same role",async()=>{
  expect((await expenseRoute(db,f.company,f.claimant.userId)).blockedReason).toBeNull();
  await db.prepare("UPDATE hr_engagements SET level_id=? WHERE person_id=?").bind(`org-level-${f.company}-5`,`person-${f.manager.userId}`).run();
  expect((await expenseRoute(db,f.company,f.claimant.userId)).blockedReason).toContain("Level 1–4");
});
it("checks readiness per request, leaving unrelated incomplete profiles harmless",async()=>{
  await f.member("incomplete");expect((await expenseRoute(db,f.company,f.claimant.userId)).blockedReason).toBeNull();
  await db.prepare("UPDATE hr_engagements SET manager_person_id=NULL WHERE person_id=?").bind(`person-${f.claimant.userId}`).run();
  expect((await expenseRoute(db,f.company,f.claimant.userId)).blockedReason).toContain("manager");
});
it("freezes submitted template and supervisor despite later organization and template changes",async()=>{
  const id=await f.claim();await f.attach(id);await f.action(f.claimant,id,"submit");
  await db.prepare("UPDATE hr_engagements SET manager_person_id=? WHERE person_id=?").bind(`person-${f.owner.userId}`,`person-${f.claimant.userId}`).run();
  await saveSpendingTemplate(db,f.owner,{expectedVersion:1,reason:"Explicit expense exception",exceptions:[{positionId:null,requestKind:"expense",action:"skip_supervisor",supervisorUserId:null,reason:"Approved temporary company exception"}]},key());
  const view=await approvalView(db,"expense",id,f.head);
  expect(view.canDecide).toBe(true);expect(JSON.parse(view.currentApprover!.route_snapshot_json!)).toMatchObject({templateVersion:1});
  const fresh=await expenseRoute(db,f.company,f.claimant.userId);expect(fresh.steps[0].stage).toBe("department_approval");expect(fresh.steps[1].snapshot?.templateVersion).toBe(2);
});
it("requires explicit exceptions for missing managers without exempting Accounting",async()=>{
  await db.prepare("UPDATE hr_engagements SET manager_person_id=NULL WHERE person_id=?").bind(`person-${f.claimant.userId}`).run();
  await saveSpendingTemplate(db,f.owner,{expectedVersion:1,reason:"Temporary payment exception",exceptions:[{positionId:null,requestKind:"payment",action:"skip_supervisor",supervisorUserId:null,reason:"Company leadership decision"}]},key());
  expect((await expenseRoute(db,f.company,f.claimant.userId,[],false,"expense")).blockedReason).toContain("manager");
  expect((await expenseRoute(db,f.company,f.claimant.userId,[],false,"payment")).steps.map(s=>s.stage)).toEqual(["department_approval","accounts_review","accounts_approval"]);
});
it("keeps purchase requests out of purchasing until both Accounting steps complete",async()=>{
  const request=await saveEmployeeRequest(db,f.claimant,null,employeeRequestSchema.parse({kind:"purchase",title:"New laptop",description:"Replacement for damaged laptop",location:"Office",branchId:f.branch,departmentId:f.department,currency:"THB",items:[{description:"Laptop",quantity:"1",estimatedCost:"25000"}]}),key());
  async function act(actor:typeof f.claimant,action:string){const row=await getEmployeeRequest(db,actor,request.id);return actOnEmployeeRequest(db,actor,request.id,requestActionSchema.parse({action,expectedVersion:row.version,note:"Reviewed spending details and supporting documents"}),key());}
  await act(f.claimant,"submit");await act(f.head,"approve");
  let row=await getEmployeeRequest(db,f.owner,request.id);expect(row.state).toBe("awaiting_approval");expect(JSON.parse(row.approval_json).decision).toBe("pending");
  expect((await resolveFlow(db,f.reviewer,"request",request.id)).current?.key).toBe("purchase_1");
  await act(f.reviewer,"approve");await act(f.manager,"approve");
  row=await getEmployeeRequest(db,f.owner,request.id);expect(JSON.parse(row.approval_json).decision).toBe("approved");expect(JSON.parse(row.approval_json).decisions).toHaveLength(3);
  expect(row.state).toBe("blocked");expect(row.blocked_reason).toContain("coverage"); // Purchasing team still needs configuration.
});

it("allows the named department appointee to decide with an active employee assignment",async()=>{
 const employeeId=await f.grant(f.head,"employee");
 const employee={...f.head,role:"employee",assignmentId:employeeId};
 const id=await f.claim();await f.attach(id);await f.action(f.claimant,id,"submit");
 expect((await approvalView(db,"expense",id,employee)).canDecide).toBe(true);
 expect((await getExpense(employee,id)).id).toBe(id);
 expect((await approvalView(db,"expense",id,f.head)).canDecide).toBe(true);
 expect((await approvalView(db,"expense",id,f.tie)).canDecide).toBe(false);
 const {selectWorkRole}=await import("../../../src/server/modules/work_projection/public/work-role-store");
 const sessionId=crypto.randomUUID();await selectWorkRole(db,{sessionId,userId:f.head.userId,companyId:f.company,assignmentId:employeeId,assignmentVersion:1,revision:0,expiresAt:Date.now()+60000});
 await f.action({...employee,authority:{sessionId,revision:1,assignmentVersion:1}},id,"approve");
 const evidence=await db.prepare("SELECT d.authority_json FROM approval_decisions d JOIN approval_instances i ON i.id=d.instance_id WHERE i.record_id=? AND d.actor_user_id=?").bind(id,f.head.userId).first<{authority_json:string}>();
 expect(JSON.parse(evidence!.authority_json).activeAuthority).toMatchObject({role:'employee',assignmentId:employeeId,assignmentVersion:1,revision:1});
 expect((await getExpense(f.head,id)).workflow_stage).toBe("accounts_review");
});
it("does not turn a Treasury assignment into the same person's Accounting reviewer role",async()=>{
 const treasuryId=await f.grant(f.manager,"treasury");
 const treasury={...f.manager,role:"treasury",assignmentId:treasuryId};
 const id=await f.claim();await f.throughAccounts(id);
 expect((await approvalView(db,"expense",id,treasury)).canDecide).toBe(false);
 await expect(f.action(treasury,id,"approve")).rejects.toMatchObject({status:403});
 expect((await approvalView(db,"expense",id,f.manager)).canDecide).toBe(true);
 await f.action(f.manager,id,"approve");
});
