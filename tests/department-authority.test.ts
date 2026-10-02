import { beforeAll,beforeEach,expect,it,vi } from "vitest";
import { env,applyMigrations } from "./runtime-env";
import { expenseFixture,expenseDb as db,key } from "./helpers/expense-fixture";
import { appointmentOverview,changeAppointment,departmentFlowInput,departmentRoute,publishDepartmentFlow } from "../src/server/department-authority";
import { approvalView } from "../src/server/approvals";
import { getExpense } from "../src/server/expenses";
import { loadIamAuthority } from "../src/server/iam-store";
import { decideDepartmentOverride,departmentOverrideDetail,departmentOverrideWork } from "../src/server/department-decisions";
import { saveEmployeeRequest,employeeRequestSchema,actOnEmployeeRequest,requestActionSchema,getEmployeeRequest } from "../src/server/employee-requests";
import { reconcilePositionGpo } from "../src/server/position-gpo";

vi.mock("../src/server/runtime",()=>({holdingResources:async()=>({write:db,files:env.H001_FILES}),runtimeEnv:async()=>env}));
let f:Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async()=>{await applyMigrations(db,env.STAFF_TEST_MIGRATIONS);});
beforeEach(async()=>{f=await expenseFixture();});

it("accepts 1–20 ordered selectors and rejects an empty or 21-step flow",()=>{
  const base={departmentId:"department",expectedVersion:0,reason:"Publish ordered approval"};
  const selectors=Array.from({length:21},(_,index)=>({kind:"person" as const,personId:`person-${index}`}));
  expect(departmentFlowInput.safeParse({...base,steps:selectors.slice(0,1)}).success).toBe(true);
  expect(departmentFlowInput.safeParse({...base,steps:selectors.slice(0,20)}).success).toBe(true);
  expect(departmentFlowInput.safeParse({...base,steps:[]}).success).toBe(false);
  expect(departmentFlowInput.safeParse({...base,steps:selectors}).success).toBe(false);
});

it("records explicit HR and head appointments with authority, versions and replay",async()=>{
  const hr={kind:"hr" as const,departmentId:null,positionId:null,personId:`person-${f.tie.userId}`,expectedVersion:0,reason:"Appoint HR lead"};
  const command=key(),first=await changeAppointment(db,f.owner,hr,command);
  expect(await changeAppointment(db,f.owner,hr,command)).toEqual(first);
  await expect(changeAppointment(db,f.owner,{...hr,personId:`person-${f.head.userId}`},command)).rejects.toMatchObject({code:"IDEMPOTENCY_CONFLICT"});
  await expect(changeAppointment(db,f.claimant,{...hr,expectedVersion:1},key())).rejects.toMatchObject({status:403});
  const head={kind:"head" as const,departmentId:f.department,positionId:null,personId:`person-${f.tie.userId}`,expectedVersion:1,reason:"Rotate department head"};
  expect((await changeAppointment(db,f.tie,head,key()) as {version:number}).version).toBe(2);
  await expect(changeAppointment(db,f.tie,head,key())).rejects.toMatchObject({code:"VERSION_CONFLICT"});
  const overview=await appointmentOverview(db,f.owner);
  expect(overview.history.filter((h)=>h.provenance==="explicit")).toHaveLength(2);
  expect((await departmentRoute(db,f.company,f.claimant.userId)).steps[0].userId).toBe(f.tie.userId);
});

it("loads publish authority with a bounded number of database reads",async()=>{
  const prepare=vi.spyOn(db,"prepare");
  try{
    const overview=await appointmentOverview(db,f.owner);
    expect(overview.departments.length).toBeGreaterThan(1);
    const actorLookups=prepare.mock.calls.filter(([sql])=>
      sql.includes("SELECT id FROM hr_people WHERE company_id=? AND iam_same_person"));
    expect(actorLookups).toHaveLength(1);
  }finally{prepare.mockRestore();}
  expect((await appointmentOverview(db,f.head)).canPublishDepartmentIds).toContain(f.department);
});

it("grants People and Leave work from the current HR appointment without granting IAM or Finance",async()=>{
  const unappointed=await f.member("Unappointed HR","hr_admin");
  expect((await loadIamAuthority(db,f.company,unappointed.userId,"hr_admin")).permissions).not.toContain("people.manage");
  const personId=`person-${f.claimant.userId}`;
  await changeAppointment(db,f.owner,{kind:"hr",departmentId:null,positionId:null,personId,expectedVersion:0,reason:"Appoint People lead"},key());
  const appointed=await loadIamAuthority(db,f.company,f.claimant.userId,"employee");
  expect(appointed.permissions).toEqual(expect.arrayContaining(["people.read","people.manage","leave.policy.manage"]));
  expect(appointed.permissions).not.toContain("iam.manage");
  expect(appointed.permissions).not.toContain("finance.pay");
  await db.prepare("UPDATE hr_engagements SET status='ended' WHERE person_id=?").bind(personId).run();
  const departed=await loadIamAuthority(db,f.company,f.claimant.userId,"employee");
  expect(departed.permissions).not.toContain("people.manage");
  expect(departed.hrAppointmentVersion).toBeUndefined();
});

it("freezes the published policy and selected people on submission while new requests use the new version",async()=>{
  await publishDepartmentFlow(db,f.head,{departmentId:f.department,expectedVersion:0,steps:[{kind:"person",personId:`person-${f.head.userId}`},{kind:"position",positionId:(await db.prepare("SELECT position_id FROM hr_engagements WHERE person_id=?").bind(`person-${f.owner.userId}`).first<{position_id:string}>())!.position_id}],reason:"Two department approvals"},key());
  const id=await f.claim();await f.attach(id);await f.action(f.claimant,id,"submit");
  const saved=await approvalView(db,"expense",id,f.head);
  expect(saved.steps.filter(s=>s.stage==="department_approval")).toHaveLength(2);
  expect(JSON.parse(saved.steps[0].route_snapshot_json!)).toMatchObject({departmentPolicyVersion:1,selectedPersonId:`person-${f.head.userId}`});
  await publishDepartmentFlow(db,f.head,{departmentId:f.department,expectedVersion:1,steps:[{kind:"person",personId:`person-${f.tie.userId}`}],reason:"New flow for new requests"},key());
  expect((await approvalView(db,"expense",id,f.head)).steps[0].approver_user_id).toBe(f.head.userId);
  expect((await departmentRoute(db,f.company,f.claimant.userId)).steps[0].userId).toBe(f.tie.userId);
  await f.action(f.head,id,"approve");
  expect((await getExpense(f.owner,id)).workflow_stage).toBe("department_approval");
  expect((await approvalView(db,"expense",id,f.owner)).currentApprover?.approver_user_id).toBe(f.owner.userId);
});

it("lets only a higher real reporting manager override a pending department step with a reason",async()=>{
  const id=await f.claim();await f.attach(id);await f.action(f.claimant,id,"submit");
  expect((await approvalView(db,"expense",id,f.owner)).canOverride).toBe(true);
  expect((await approvalView(db,"expense",id,f.tie)).canOverride).toBe(false);
  expect((await departmentOverrideWork(f.owner)).find(item=>item.id===id)).toMatchObject({queue:"todo",actionKind:"override",relationships:["approval"]});
  expect(Object.keys(await departmentOverrideDetail(f.owner,"expense",id))).not.toContain("amount");
  const row=await getExpense(f.owner,id);
  const command=key(),input={kind:"expense",id,expectedVersion:Number(row.version),outcome:"approve",reason:"Manager is unavailable; reviewed the receipt"};
  const decided=await decideDepartmentOverride(f.owner,input,command);
  expect(await decideDepartmentOverride(f.owner,input,command)).toEqual(decided);
  const view=await approvalView(db,"expense",id,f.owner),event=view.history.find(h=>h.action==="override");
  expect(event).toMatchObject({previous_approver_user_id:f.head.userId,approver_user_id:f.owner.userId,actor_user_id:f.owner.userId});
  expect(JSON.parse(event!.authority_json!)).toMatchObject({outcome:"approve",originalApproverUserId:f.head.userId});
  expect(view.currentApprover?.stage).toBe("accounts_review");
});

it("queues position GPO removal on departure without changing appointment authority",async()=>{
  const personId=`person-${f.head.userId}`,
    engagement=(await db.prepare("SELECT id,position_id,version FROM hr_engagements WHERE person_id=?").bind(personId).first<{id:string;position_id:string;version:number}>())!,
    position=engagement.position_id;
  const connection=`gpo-connection-${key()}`,now=Date.now();
  await db.prepare("INSERT INTO iam_connections(id,company_id,name,provider,config,status,created_by,created_at) VALUES(?,?,'Test Directory','active_directory','{}'::jsonb,'active',?,?)").bind(connection,f.company,f.it.userId,now).run();
  await db.prepare("INSERT INTO position_gpo_policies(company_id,position_id,version,revision,connection_id,gpo_id,user_filter_group_id,computer_target_ou_id,updated_at) VALUES(?,?,1,1,?,'gpo','group','ou',?)").bind(f.company,position,connection,now).run();
  await db.prepare("INSERT INTO position_gpo_revisions(id,company_id,position_id,revision,connection_id,gpo_id,user_filter_group_id,computer_target_ou_id,directory_gpo_version,affected_person_ids,published_by,reason,command_id,created_at) VALUES(?,?,?,1,?,'gpo','group','ou','1','[]'::jsonb,?,'Test binding',?,?)").bind(`revision-${key()}`,f.company,position,connection,f.it.userId,key(),now).run();
  // The current inherited-GPO model reconciles verified source claims instead
  // of firing a person-only trigger. Seed the membership that ERPX previously
  // applied, then verify departure queues its canonical removal job.
  const accountId=`gpo-account-${key()}`,sourceId=`position-gpo:${engagement.id}`,ruleId=`legacy-position:${position}`;
  await db.batch([
    db.prepare("INSERT INTO people_directory_membership_sources(connection_id,group_id,member_id,source_id,created_at) VALUES(?,'group',?,?,?)")
      .bind(connection,accountId,sourceId,now),
    db.prepare(`INSERT INTO organization_gpo_membership_claims(
      connection_id,group_id,member_id,source_id,rule_id,company_id,position_id,engagement_id,person_id,
      revision,engagement_version,gpo_id,computer_target_ou_id
    ) VALUES(?,'group',?,?,?,?,?,?,?,1,?,'gpo','ou')`)
      .bind(connection,accountId,sourceId,ruleId,f.company,position,engagement.id,personId,engagement.version),
  ]);
  await db.prepare("UPDATE hr_engagements SET status='ended',version=version+1 WHERE person_id=?").bind(personId).run();
  await reconcilePositionGpo(db);
  expect(await db.prepare("SELECT desired_action,status FROM position_gpo_jobs WHERE company_id=? AND person_id=?").bind(f.company,personId).first()).toMatchObject({desired_action:"remove",status:"queued"});
  expect((await appointmentOverview(db,f.owner)).seats.find((s)=>s.kind==="head"&&s.scope_id===f.department)?.person_id).toBe(personId);
});

it("overrides only the pending purchase department step and continues to Accounting",async()=>{
  const request=await saveEmployeeRequest(db,f.claimant,null,employeeRequestSchema.parse({kind:"purchase",title:"Replacement laptop",description:"Replace broken laptop",location:"Office",branchId:f.branch,departmentId:f.department,currency:"THB",items:[{description:"Laptop",quantity:"1",estimatedCost:"25000"}]}),key());
  await actOnEmployeeRequest(db,f.claimant,request.id,requestActionSchema.parse({action:"submit",expectedVersion:request.version}),key());
  const before=await getEmployeeRequest(db,f.owner,request.id),command=key();
  const input={kind:"request",id:request.id,expectedVersion:before.version,outcome:"approve",reason:"Head unavailable; reviewed department need"};
  expect((await departmentOverrideWork(f.owner)).some(item=>item.id===request.id)).toBe(true);
  const result=await decideDepartmentOverride(f.owner,input,command);
  expect(await decideDepartmentOverride(f.owner,input,command)).toEqual(result);
  const after=await getEmployeeRequest(db,f.owner,request.id),saved=JSON.parse(after.approval_json);
  expect(after.state).toBe("awaiting_approval");
  expect(saved.currentStep).toBe(1);
  expect(saved.decisions[0]).toMatchObject({override:true,originalApproverUserId:f.head.userId,actualApproverUserId:f.owner.userId});
});

it("records a rejected equipment override with both identities and prevents a second decision",async()=>{
  const request=await saveEmployeeRequest(db,f.claimant,null,employeeRequestSchema.parse({kind:"equipment",title:"Replacement device",description:"Existing device is broken",location:"Office",branchId:f.branch,departmentId:f.department,neededDate:"2026-10-01",currency:"THB",items:[{description:"Laptop",quantity:"1",estimatedCost:"25000"}]}),key());
  await actOnEmployeeRequest(db,f.claimant,request.id,requestActionSchema.parse({action:"submit",expectedVersion:request.version}),key());
  const before=await getEmployeeRequest(db,f.owner,request.id);
  await decideDepartmentOverride(f.owner,{kind:"request",id:request.id,expectedVersion:before.version,outcome:"reject",reason:"Replacement requires a revised specification"},key());
  const after=await getEmployeeRequest(db,f.owner,request.id),saved=JSON.parse(after.approval_json);
  expect(after.state).toBe("rejected");
  expect(saved.decisions.at(-1)).toMatchObject({override:true,originalApproverUserId:f.head.userId,actualApproverUserId:f.owner.userId});
  await expect(decideDepartmentOverride(f.owner,{kind:"request",id:request.id,expectedVersion:after.version,outcome:"approve",reason:"Late duplicate"},key())).rejects.toMatchObject({code:"OVERRIDE_UNAVAILABLE"});
});
