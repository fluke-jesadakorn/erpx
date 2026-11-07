import { env } from "../../support/runtime-env";
import { applyMigrations, type TestMigration } from "../../support/runtime-env";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import {
  activeStaff,
  requestRoute,
  staffPeople,
} from "@erpx/server-operations/public/staff-policy";
import {
  assignLevel,
  saveLevel,
  setDepartmentHead,
} from "@erpx/server-operations/public/staff-settings";
import {approvalView,prepareDecision,openApproval} from '@erpx/server-workflow/public/approval-planning';
import {rerouteApproval} from '@erpx/server-workflow/public/approvals';
import {projectSchema,saveProject,saveTask,delegateTask,revokeTaskDelegation,taskDelegationSchema,taskDelegationRevokeSchema,taskSchema,taskActionSchema,taskDetail,listTasks,transitionTask} from '@erpx/server-operations/public/staff-tasks';
import {taskWorkState} from '@erpx/server-operations/public/staff-task-read';
import {getTask} from '@erpx/server-operations/public/staff-task-read';
import {
  listProjectMilestones,
  projectMilestoneSchema,
  saveProjectMilestone,
  transitionProjectMilestone,
} from "@erpx/server-projects/public/project-milestones";
import {
  generateOccurrences,
  saveSchedule,
  saveTemplate,
  scheduleSchema,
  templateSchema,
  changeScheduleState,
  runTemplate,
} from "@erpx/server-operations/public/routines";
import {createLeave,submitLeave,approveLeave,transitionLeave,leaveDraftSchema,leaveDecisionSchema,leaveLifecycleSchema} from '@erpx/server-people_payroll/public/leave';
import {getLeave} from '@erpx/server-people_payroll/public/leave-read';
import {saveExpense,expenseUpdateSchema} from '@erpx/server-people_payroll/public/expenses';
import {getExpense} from '@erpx/server-people_payroll/public/expense-read';
import { executeCommand } from "@erpx/server-workflow/public/commands";
import { administrationAccess } from "@erpx/server-runtime/public/administration";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { attachmentBindingState, enableAttachmentV2ForTest, seedAttachmentSet } from "../../support/helpers/attachment-v2";
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
const db = env.H001_DB;
let company: string,
  branch: string,
  department: string,
  otherDepartment: string;
let requester: WorkContext,
  head: WorkContext,
  hr: WorkContext,
  owner: WorkContext,
  worker: WorkContext,
  outsider: WorkContext;
afterEach(()=>vi.unstubAllEnvs());
const key = () => crypto.randomUUID();
const user = (name: string) => `${company}-${name}`;
function actor(
  name: string,
  role = "employee",
  dept = department,
): WorkContext {
  return {
    userId: user(name),
    assignmentId: `grant-${user(name)}`,
    role,
    companyId: company,
    branchId: branch,
    departmentId: dept,
    holdingId: "h001",
    holdingSlug: "test",
    holdingName: "Test",
    name,
    email: `${name}@example.test`,
    scope: {
      kind: "department",
      holdingId: "h001",
      holdingSlug: "test",
      holdingName: "Test",
      companyId: company,
      branchId: branch,
      departmentId: dept,
      departmentSlug: "test",
      departmentName: "Test",
    },
    assignmentScope: "holding",
    inherited: true,
    modules: [],
    assignments: [],
    workspaces: [],
  };
}
async function staff(
  name: string,
  role = "employee",
  dept = department,
  type = "employee",
) {
  const id = user(name),
    person = `person-${id}`,
    engagement = `eng-${id}`;
  await db.batch([
    db
      .prepare(
        "INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES(?,?,?,'[]',?,0,9999999999999,'holding')",
      )
      .bind(`grant-${id}`, id, role, company),
    db
      .prepare(
        "INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES(?,?,?,?,'TH','fixture',1,1)",
      )
      .bind(person, id, company, name),
    db
      .prepare(
        "INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,'active','2020-01-01','Test staff','THB','fixture',1,1)",
      )
      .bind(
        engagement,
        person,
        company,
        branch,
        dept,
        type,
        type === "contractor" ? "contractor" : "salaried",
      ),
  ]);
  return actor(name, role, dept);
}
async function leave() {
  const created = await createLeave(
    requester,
    { startDate: "2026-09-14", days: 2, handoverNote: "Team handover" },
    key(),
  );
  return String(created.id);
}
async function submit(id: string, version = 1) {
  return submitLeave(
    "h001",
    id,
    leaveDraftSchema.parse({
      startDate: "2026-09-14",
      days: 2,
      handoverNote: "Team handover",
      expectedVersion: version,
    }),
    key(),
    requester,
  );
}
async function task(
  start: string[] = [],
  completion: string[] = [head.userId],
  assignee = worker.userId,
) {
  const result = await saveTask(
    db,
    requester,
    null,
    taskSchema.parse({
      title: "Inspect deliveries",
      description: "Check the receiving log",
      assigneeUserId: assignee,
      dueAt: "2026-10-01T02:00:00Z",
      startApprovers: start,
      completionApprovers: completion,
    }),
    key(),
  );
  return String(result.id);
}
async function action(
  who: WorkContext,
  id: string,
  act: "submit" | "start" | "complete" | "approve" | "reject" | "cancel",
  note = "Verified",
) {
  const row = await getTask(db, who, id);
  return transitionTask(
    db,
    who,
    id,
    taskActionSchema.parse({ action: act, expectedVersion: row.version, note }),
    key(),
  );
}

beforeAll(async () => {
  enableAttachmentV2ForTest();
  await applyMigrations(
    db,
    (env as typeof env & { STAFF_TEST_MIGRATIONS: TestMigration[] })
      .STAFF_TEST_MIGRATIONS,
  );
});
beforeEach(async () => {
  company = `co-${key()}`;
  branch = `b-${company}`;
  department = `d-${company}`;
  otherDepartment = `other-${company}`;
  for (const [id, kind, parent, slug] of [
    [company, "company", null, company],
    [branch, "branch", company, branch],
    [department, "department", branch, department],
    [otherDepartment, "department", branch, otherDepartment],
  ])
    await db
      .prepare(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id) VALUES(?,?,?,?,?,?,?)",
      )
      .bind(
        id,
        kind,
        slug,
        id,
        parent,
        company,
        kind === "department" ? branch : null,
      )
      .run();
  requester = await staff(
    "requester",
    "sales_operator",
    department,
    "contractor",
  );
  head = await staff("head", "people_manager", otherDepartment);
  hr = await staff("hr", "hr_admin", otherDepartment);
  owner = await staff("owner", "holding_owner");
  worker = await staff("worker", "treasury", otherDepartment);
  outsider = await staff("outsider", "people_manager");
  await db.prepare("UPDATE hr_engagements SET manager_person_id=? WHERE person_id=?").bind(`person-${head.userId}`,`person-${requester.userId}`).run();
  await setDepartmentHead(
    db,
    owner,
    department,
    {
      personId: `person-${head.userId}`,
      expectedVersion: 0,
      reason: "Designate independent department head",
    },
    key(),
  );
});
describe("Staff request routing and approval transactions", () => {
  it("keeps Work task actions equal to authorized details through approvals, delegation and revocation", async () => {
    const id = await task([head.userId], [head.userId]);
    const compare = async () => {
      for (const who of [requester, head, hr, worker, outsider]) {
        const row = (await listTasks(db, who)).find(record => record.id === id);
        if (!row) {
          await expect(taskDetail(db, who, id)).rejects.toMatchObject({ status: 404 });
          continue;
        }
        const detail = await taskDetail(db, who, id);
        expect(await taskWorkState(db, who, row)).toEqual({
          approval: detail.approval, permittedActions: detail.permittedActions,
          blockedReason: detail.blockedReason,
        });
      }
    };
    await compare();
    await action(requester, id, "submit");
    await compare();
    await db.prepare("UPDATE assignments SET revoked_at=1 WHERE id=?").bind(head.assignmentId).run();
    await compare();
    expect((await taskWorkState(db, head, await getTask(db, head, id))).permittedActions).not.toContain("approve");
    await db.prepare("UPDATE assignments SET revoked_at=NULL WHERE id=?").bind(head.assignmentId).run();
    await action(head, id, "approve");
    await compare();
    const handoff = await delegateTask(db, requester, id, taskDelegationSchema.parse({
      delegateUserId: hr.userId, expiresAt: new Date(Date.now() + 60_000).toISOString(),
      expectedVersion: (await getTask(db, requester, id)).version, reason: "Cover the receiving lead.",
    }), key());
    await compare();
    await revokeTaskDelegation(db, requester, id, taskDelegationRevokeSchema.parse({
      delegationId: handoff.delegationId, expectedVersion: handoff.version,
      reason: "Receiving lead returned.",
    }), key());
    await compare();
    await action(worker, id, "start");
    await compare();
    await action(worker, id, "complete");
    await compare();
    await action(head, id, "reject");
    await compare();
    await action(worker, id, "complete");
    await action(head, id, "approve");
    await compare();
  });

  it("allows employees and contractors under every business role, and missing levels do not block personal requests", async () => {
    const chart = `chart-${company}`,
      account = `expense-${company}`;
    await db.batch([
      db
        .prepare(
          "INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES(?,?,'Staff test chart','THB','fixture',1,1)",
        )
        .bind(chart, company),
      db
        .prepare(
          "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,'5000','Operating expense','expense','THB',1,1)",
        )
        .bind(account, chart, company),
      db
        .prepare(
          "INSERT INTO commerce_settings VALUES(?,?,?, ?,?,?,?,?,?,?,?,?)",
        )
        .bind(company, chart, "THB", ...Array(9).fill(account)),
    ]);
    for (const workerType of ["employee", "contractor"]) {
      for (const role of [
        "holding_owner",
        "sales_manager",
        "sales_operator",
        "purchasing_manager",
        "purchasing_operator",
        "warehouse_manager",
        "operations_manager",
        "employee",
        "finance_preparer",
        "people_manager",
        "hr_admin",
        "payroll_preparer",
        "payroll_approver",
        "finance_approver",
        "treasury",
        "auditor",
      ]) {
        const person = await staff(
          `role-${workerType}-${role}`,
          role,
          department,
          workerType,
        );
        expect(
          (await activeStaff(db, company, person.userId)).level_id,
        ).toBeNull();
        expect(
          (
            await createLeave(
              { ...person, departmentId: otherDepartment },
              {
                startDate: "2026-09-14",
                days: 1,
                handoverNote: "Coverage arranged",
              },
              key(),
            )
          ).state,
        ).toBe("draft");
        const expense = await saveExpense(
          { ...person, departmentId: otherDepartment },
          null,
          expenseUpdateSchema.parse({
            personId: `person-${person.userId}`,
            expenseDate: "2026-09-07",
            merchant: "Travel provider",
            receiptReference: "T-001",
            businessPurpose: "Visit delivery partner",
            currency: "THB",
            amount: "100",
            categoryAccountId: account,
            expectedVersion: 1,
          }),
          key(),
        );
        const claim = await getExpense(person, String(expense.id));
        expect(claim.department_id).toBe(department);
        expect(claim.status).toBe("draft");
      }
    }
  });
  it("uses the reporting manager and engagement scope, requires ordered independent HR, preserves scope across workspaces", async () => {
    requester = { ...requester, departmentId: otherDepartment };
    const id = await leave();
    await submit(id);
    let detail = await getLeave("h001", id, requester);
    expect(detail.department_id).toBe(department);
    expect(detail.approval.steps.map((s) => s.approver_user_id)).toEqual([
      head.userId,
      hr.userId,
    ]);
    await expect(
      approveLeave(
        "h001",
        id,
        leaveDecisionSchema.parse({ expectedVersion: 2, note: "Too early" }),
        key(),
        hr,
      ),
    ).rejects.toMatchObject({ status: 403 });
    await expect(getLeave("h001", id, outsider)).rejects.toMatchObject({
      status: 404,
    });
    await approveLeave(
      "h001",
      id,
      leaveDecisionSchema.parse({ expectedVersion: 2, note: "Department approved" }),
      key(),
      head,
    );
    detail = await getLeave("h001", id, requester);
    expect(detail.state).toBe("pending_approval");
    expect(detail.permittedActions).toContain("cancel");
    expect(detail.approval.currentApprover?.approver_user_id).toBe(hr.userId);
    await expect(
      approveLeave(
        "h001",
        id,
        leaveDecisionSchema.parse({ expectedVersion: 3, note: "Head again" }),
        key(),
        head,
      ),
    ).rejects.toMatchObject({ status: 403 });
    await approveLeave(
      "h001",
      id,
      leaveDecisionSchema.parse({ expectedVersion: 3, note: "HR approved" }),
      key(),
      hr,
    );
    expect((await getLeave("h001", id, requester)).state).toBe("approved");
    const event = await db
      .prepare(
        "SELECT payload_json FROM outbox_events WHERE aggregate_id=? AND event_type='leave.approve' ORDER BY aggregate_version DESC LIMIT 1",
      )
      .bind(id)
      .first<{ payload_json: string }>();
    expect(JSON.parse(event!.payload_json).departmentId).toBe(department);
  });
  it("keeps drafts when routing is missing, detects cycles and escalates a head's own request", async () => {
    await db
      .prepare("UPDATE hr_engagements SET manager_person_id=NULL WHERE person_id=?")
      .bind(`person-${requester.userId}`)
      .run();
    const id = await leave();
    await submit(id);
    expect((await getLeave("h001", id, requester)).state).toBe("pending_approval");
    await setDepartmentHead(
      db,
      owner,
      department,
      {
        personId: `person-${requester.userId}`,
        expectedVersion: 1,
        reason: "Head is requester",
      },
      key(),
    );
    await db.prepare("UPDATE hr_engagements SET self_service_top_level=true WHERE person_id=?").bind(`person-${requester.userId}`).run();
    let route = await requestRoute(db, company, requester.userId, "leave");
    expect(route.steps[0].userId).toBe(owner.userId);
    await db
      .prepare(
        "UPDATE hr_engagements SET manager_person_id=? WHERE person_id=?",
      )
      .bind(`person-${head.userId}`, `person-${requester.userId}`)
      .run();
    route = await requestRoute(db, company, requester.userId, "leave");
    expect(route.steps[0].userId).toBe(head.userId);
    await db
      .prepare(
        "UPDATE hr_engagements SET manager_person_id=? WHERE person_id=?",
      )
      .bind(`person-${requester.userId}`, `person-${head.userId}`)
      .run();
    expect(
      (await requestRoute(db, company, requester.userId, "leave"))
        .blockedReason,
    ).toMatch(/cycle/);
  });
  it("rejects missing, inactive and ambiguous engagement links", async () => {
    await expect(activeStaff(db, company, "unknown")).rejects.toMatchObject({
      code: "STAFF_LINK_REQUIRED",
    });
    await db
      .prepare("UPDATE hr_engagements SET status='suspended' WHERE person_id=?")
      .bind(`person-${requester.userId}`)
      .run();
    await expect(leave()).rejects.toMatchObject({
      code: "STAFF_LINK_REQUIRED",
    });
    await db
      .prepare(
        "UPDATE hr_engagements SET status='active',end_date='2020-01-02' WHERE person_id=?",
      )
      .bind(`person-${requester.userId}`)
      .run();
    await expect(leave()).rejects.toMatchObject({
      code: "STAFF_LINK_REQUIRED",
    });
  });
  it("blocks missing reporting managers and independent HR explicitly", async () => {
    await db
      .prepare("UPDATE hr_engagements SET manager_person_id=NULL WHERE person_id=?")
      .bind(`person-${requester.userId}`)
      .run();
    expect(
      (await requestRoute(db, company, requester.userId, "leave"))
        .blockedReason,
    ).toMatch(/reporting manager/);
    await db
      .prepare("UPDATE hr_engagements SET manager_person_id=? WHERE person_id=?")
      .bind(`person-${head.userId}`, `person-${requester.userId}`)
      .run();
    await db
      .prepare("UPDATE assignments SET revoked_at=1 WHERE id=?")
      .bind(hr.assignmentId)
      .run();
    expect(
      (await requestRoute(db, company, requester.userId, "leave"))
        .blockedReason,
    ).toMatch(/HR reviewer/);
    expect(
      (await requestRoute(db, company, requester.userId, "expense")).steps,
    ).toHaveLength(1);
  });
  it("starts fresh cycles after rejection and allows cancellation before HR final approval", async () => {
    const id = await leave();
    await submit(id);
    await transitionLeave(
      head,
      id,
      leaveLifecycleSchema.parse({ action: "reject", expectedVersion: 2, note: "Correct handover" }),
      key(),
    );
    await submit(id, 3);
    let detail = await getLeave("h001", id, requester);
    expect(detail.approval.instance?.cycle).toBe(2);
    expect(detail.approval.history).toHaveLength(1);
    await approveLeave(
      "h001",
      id,
      leaveDecisionSchema.parse({ expectedVersion: 4, note: "Department okay" }),
      key(),
      head,
    );
    await transitionLeave(
      requester,
      id,
      leaveLifecycleSchema.parse({ action: "cancel", expectedVersion: 5, note: "Dates changed" }),
      key(),
    );
    detail = await getLeave("h001", id, requester);
    expect(detail.state).toBe("cancelled");
    await expect(
      approveLeave(
        "h001",
        id,
        leaveDecisionSchema.parse({ expectedVersion: 6, note: "Too late" }),
        key(),
        hr,
      ),
    ).rejects.toMatchObject({ status: 403 });
  });
  it("commits one concurrent decision, replays retries, and rejects stale versions", async () => {
    const id = await leave();
    await submit(id);
    const input = { expectedVersion: 2, note: "Department reviewed" },
      command = key();
    const result = await approveLeave("h001", id, leaveDecisionSchema.parse(input), command, head);
    expect(await approveLeave("h001", id, leaveDecisionSchema.parse(input), command, head)).toEqual(
      result,
    );
    const outcomes = await Promise.allSettled([
      approveLeave(
        "h001",
        id,
        leaveDecisionSchema.parse({ expectedVersion: 3, note: "Approved" }),
        key(),
        hr,
      ),
      approveLeave(
        "h001",
        id,
        leaveDecisionSchema.parse({ expectedVersion: 3, note: "Approved" }),
        key(),
        hr,
      ),
    ]);
    expect(outcomes.filter((o) => o.status === "fulfilled")).toHaveLength(1);
    expect(
      (await getLeave("h001", id, requester)).approval.history,
    ).toHaveLength(2);
  });
  it("rolls back a decision when the HR grant is revoked after preparation", async () => {
    const id = await leave();
    await submit(id);
    await approveLeave(
      "h001",
      id,
      leaveDecisionSchema.parse({ expectedVersion: 2, note: "Okay" }),
      key(),
      head,
    );
    const command = key();
    const prepared = await prepareDecision(
      db,
      hr,
      command,
      "leave",
      id,
      true,
      "Okay",
      3,
    );
    await db
      .prepare("UPDATE assignments SET revoked_at=? WHERE id=?")
      .bind(Date.now(), hr.assignmentId)
      .run();
    await expect(
      executeCommand(
        db,
        hr,
        command,
        "leave.approve",
        {},
        id,
        4,
        { state: "approved" },
        prepared.statements,
      ),
    ).rejects.toMatchObject({ status: 409 });
    expect(
      (await getLeave("h001", id, requester)).approval.history,
    ).toHaveLength(1);
    expect(
      await db
        .prepare("SELECT 1 FROM command_receipts WHERE command_id=?")
        .bind(command)
        .first(),
    ).toBeNull();
  });
  it("rejects a route changed between resolution and atomic submission", async () => {
    const id = await leave(),
      command = key(),
      route = await requestRoute(db, company, requester.userId, "leave");
    const prepared = await openApproval(
      db,
      command,
      "leave",
      id,
      "request",
      requester.userId,
      null,
      route,
      2,
    );
    await db.prepare("UPDATE hr_engagements SET manager_person_id=?,version=version+1 WHERE person_id=?").bind(`person-${worker.userId}`,`person-${requester.userId}`).run();
    await expect(
      executeCommand(
        db,
        requester,
        command,
        "leave.submit",
        {},
        id,
        2,
        { state: "pending_approval" },
        prepared.statements,
      ),
    ).rejects.toMatchObject({ status: 409 });
    expect(
      (await approvalView(db, "leave", id, requester)).instance,
    ).toBeNull();
  });
  it("initializes pending legacy routes once, preserving earlier decisions and record scope", async () => {
    const id = await leave();
    await db
      .prepare(
        "UPDATE leave_requests SET state='pending_approval',department_id=?,decision_note='Legacy review note' WHERE id=?",
      )
      .bind(otherDepartment, id)
      .run();
    await db
      .prepare(
        "INSERT INTO approval_instances(id,company_id,record_type,record_id,phase,cycle,requester_user_id,state,blocked_reason,created_at,updated_at) VALUES(?,?,'leave',?,'request',1,?,'blocked','Route configuration required',1,1)",
      )
      .bind(`legacy-${id}`, company, id, requester.userId)
      .run();
    await Promise.all([
      approvalView(db, "leave", id, requester),
      approvalView(db, "leave", id, head),
    ]);
    const row = await getLeave("h001", id, requester);
    expect(row.version).toBe(2);
    expect(row.department_id).toBe(department);
    expect(row.decision_note).toBe("Legacy review note");
    expect(row.approval.steps).toHaveLength(2);
    expect(row.approval.history.map((d) => d.action)).toEqual(["initialize"]);
    await approveLeave(
      "h001",
      id,
      leaveDecisionSchema.parse({ expectedVersion: 2, note: "Department review" }),
      key(),
      head,
    );
    expect((await getLeave("h001", id, requester)).state).toBe(
      "pending_approval",
    );
  });
  it("administratively reroutes unavailable approvers with evidence and still enforces HR eligibility", async () => {
    const id = await leave();
    await submit(id);
    await db
      .prepare("UPDATE hr_engagements SET status='suspended' WHERE person_id=?")
      .bind(`person-${head.userId}`)
      .run();
    let view = await approvalView(db, "leave", id, requester);
    expect(view.blockedReason).toBeTruthy();
    await rerouteApproval(
      db,
      owner,
      view.instance!.id,
      {
        expectedVersion: 2,
        instanceVersion: view.instance!.version,
        approverUserId: worker.userId,
        reason: "Head unavailable; designated replacement",
      },
      key(),
    );
    await db.prepare("UPDATE assignments SET role='people_manager' WHERE id=?").bind(worker.assignmentId).run();
    worker.role = "people_manager";
    await approveLeave(
      "h001",
      id,
      leaveDecisionSchema.parse({ expectedVersion: 3, note: "Replacement approval" }),
      key(),
      worker,
    );
    view = await approvalView(db, "leave", id, requester);
    await expect(
      rerouteApproval(
        db,
        owner,
        view.instance!.id,
        {
          expectedVersion: 4,
          instanceVersion: view.instance!.version,
          approverUserId: outsider.userId,
          reason: "Not an HR administrator",
        },
        key(),
      ),
    ).rejects.toMatchObject({ status: 422 });
    expect(view.history.map((d) => d.action)).toEqual(["reroute", "approve"]);
  });
});
describe("Projects, task phases and evidence boundaries", () => {
  it("creates an ordinary task without a deadline or reviewers and lets the assignee finish it directly", async () => {
    const input = taskSchema.parse({
      title: "Prepare meeting room",
      assigneeUserId: worker.userId,
    });
    expect(input.dueAt).toBeNull();
    const created = await saveTask(db, requester, null, input, key());
    expect(created).toMatchObject({ state: "ready", version: 1 });
    const record = await getTask(db, worker, String(created.id));
    expect(record.due_at).toBeNull();
    expect((await taskDetail(db, worker, record.id)).permittedActions).toContain("complete");
    await transitionTask(db, worker, record.id, taskActionSchema.parse({
      action: "complete", expectedVersion: record.version, note: "",
    }), key());
    expect((await getTask(db, worker, record.id)).state).toBe("completed");
    const approvals = await db.prepare("SELECT COUNT(*)::int count FROM approval_instances WHERE record_type='task' AND record_id=?")
      .bind(record.id).first<{ count: number }>();
    expect(approvals?.count).toBe(0);
  });
  it("binds clean task evidence in the same completion transaction", async () => {
    const created=await saveTask(db,requester,null,taskSchema.parse({title:"Document completed site check",assigneeUserId:worker.userId}),key());
    const evidence=await seedAttachmentSet(db,worker,{targetType:"staff_task",targetId:String(created.id),purposeKey:"task_evidence",componentKind:"action",componentRef:"task:ready",expectedTargetVersion:1});
    const command=key();
    await transitionTask(db,worker,String(created.id),taskActionSchema.parse({action:"complete",expectedVersion:1,note:"Site check completed with attached evidence",attachmentSetIds:[evidence.setId]}),command);
    expect(await attachmentBindingState(db,evidence.setId)).toMatchObject({set_state:"bound",link_state:"bound",binding_type:"task_action",binding_id:`${created.id}:complete:2`});
    expect(await getTask(db,worker,String(created.id))).toMatchObject({state:"completed",version:2});
  });
  it("updates an ordinary task without introducing a review cycle", async () => {
    const created = await saveTask(db, requester, null, taskSchema.parse({
      title: "Prepare display", assigneeUserId: worker.userId,
    }), key());
    const revised = await saveTask(db, requester, String(created.id), taskSchema.parse({
      title: "Prepare entrance display", assigneeUserId: worker.userId,
      dueAt: "2026-10-02T02:00:00Z", expectedVersion: 1,
    }), key(), true);
    expect(revised).toMatchObject({ state: "ready", version: 2 });
    const record = await getTask(db, worker, String(created.id));
    expect(record.due_at).toBe(Date.parse("2026-10-02T02:00:00Z"));
    expect(record.revision).toBe(2);
  });
  it("keeps approval and due date requirements on reviewed tasks and routine templates", async () => {
    expect(taskSchema.safeParse({
      title: "Review delivery", assigneeUserId: worker.userId,
      completionApprovers: [head.userId],
    }).success).toBe(false);
    expect(templateSchema.safeParse({
      title: "Weekly review", assigneeUserId: worker.userId,
    }).success).toBe(false);
    const id = await task();
    const row = await getTask(db, requester, id);
    await expect(saveTask(db, requester, id, taskSchema.parse({
      title: "Remove review", assigneeUserId: worker.userId,
      expectedVersion: row.version,
    }), key())).rejects.toMatchObject({ code: "TASK_REVIEW_REQUIRED" });
  });
  it.each(["before", "after", "both"] as const)(
    "runs %s approval with cross-department staff and denies unrelated readers",
    async (mode) => {
      const id = await task(
        mode === "after" ? [] : [head.userId],
        mode === "before" ? [] : [requester.userId],
      );
      await expect(getTask(db, outsider, id)).rejects.toMatchObject({
        status: 404,
      });
      await expect(
        getTask(db, { ...head, companyId: "other-company" }, id),
      ).rejects.toMatchObject({ status: 404 });
      await action(requester, id, "submit");
      if (mode !== "after") await action(head, id, "approve");
      expect((await getTask(db, worker, id)).state).toBe("ready");
      await action(worker, id, "start");
      await action(
        worker,
        id,
        "complete",
        "Deliveries verified; evidence attached",
      );
      if (mode !== "before") await action(requester, id, "approve");
      expect((await getTask(db, worker, id)).state).toBe("completed");
    },
  );
  it("prevents assignee review and creator start approval; supports rework, revision and reassignment", async () => {
    await expect(task([requester.userId])).rejects.toMatchObject({
      status: 422,
    });
    await expect(task([], [worker.userId])).rejects.toMatchObject({
      status: 422,
    });
    const id = await task([head.userId], [requester.userId]);
    await action(requester, id, "submit");
    await action(head, id, "reject", "Clarify before starting");
    expect((await getTask(db, requester, id)).state).toBe("draft");
    await action(requester, id, "submit");
    await action(head, id, "approve");
    await action(worker, id, "start");
    await action(worker, id, "complete");
    await action(requester, id, "reject", "Recheck the last delivery");
    expect((await getTask(db, worker, id)).state).toBe("rework");
    const row = await getTask(db, requester, id);
    await saveTask(
      db,
      requester,
      id,
      taskSchema.parse({
        title: "Reassigned work",
        assigneeUserId: head.userId,
        startApprovers: [],
        completionApprovers: [requester.userId],
        dueAt: "2026-10-02T02:00:00Z",
        expectedVersion: row.version,
        reason: "New accountable assignee",
      }),
      key(),
      true,
    );
    expect((await getTask(db, requester, id)).state).toBe("draft");
    expect(
      (await taskDetail(db, requester, id)).approval.history.some(
        (d) => d.action === "revise",
      ),
    ).toBe(true);
  });
  it("requires each named step in order and ignores former reviewers becoming unavailable", async () => {
    const id = await task(
      [head.userId, hr.userId],
      [requester.userId, owner.userId],
    );
    await action(requester, id, "submit");
    await expect(action(hr, id, "approve")).rejects.toMatchObject({
      status: 403,
    });
    await action(head, id, "approve");
    expect((await getTask(db, worker, id)).state).toBe("start_approval");
    await action(hr, id, "approve");
    await db
      .prepare("UPDATE hr_engagements SET status='suspended' WHERE person_id=?")
      .bind(`person-${head.userId}`)
      .run();
    await action(worker, id, "start");
    await action(worker, id, "complete");
    await expect(action(owner, id, "approve")).rejects.toMatchObject({
      status: 403,
    });
    await action(requester, id, "approve");
    expect((await getTask(db, worker, id)).state).toBe("completion_approval");
    await action(owner, id, "approve");
    expect((await getTask(db, worker, id)).state).toBe("completed");
  });
  it("allows the creator to cancel work after its assignee becomes unavailable", async () => {
    const id = await task();
    await action(requester, id, "submit");
    await db
      .prepare("UPDATE hr_engagements SET status='suspended' WHERE person_id=?")
      .bind(`person-${worker.userId}`)
      .run();
    await action(requester, id, "cancel", "Assignee unavailable");
    expect((await getTask(db, requester, id)).state).toBe("cancelled");
  });
  it("project owners manage workflows, members read only relevant project tasks", async () => {
    const p = await saveProject(
      db,
      requester,
      null,
      projectSchema.parse({
        title: "Shipping project",
        memberUserIds: [worker.userId],
      }),
      key(),
    );
    const created = await saveTask(
      db,
      worker,
      null,
      taskSchema.parse({
        title: "Member-created task",
        assigneeUserId: head.userId,
        projectId: p.id,
        dueAt: "2026-10-01T02:00:00Z",
        completionApprovers: [requester.userId],
      }),
      key(),
    );
    await expect(action(requester, String(created.id), "submit")).rejects.toMatchObject({code:"FLOW_PRIMARY_REQUIRED"});
    await action(worker, String(created.id), "submit");
    await expect(
      getTask(db, outsider, String(created.id)),
    ).rejects.toMatchObject({ status: 404 });
  });
  it("records versioned project milestones and keeps milestone writes with the project owner", async () => {
    const project = await saveProject(db, requester, null, projectSchema.parse({
      title: "Quarterly delivery",
      memberUserIds: [worker.userId],
    }), key());
    const created = await saveProjectMilestone(db, requester, project.id, null, projectMilestoneSchema.parse({
      title: "Complete supplier validation",
      description: "Confirm required documents and receiving dates.",
      dueAt: "2026-09-20T12:00:00Z",
      ownerUserId: worker.userId,
      reason: "The project owner set the delivery target.",
    }), key());
    expect(created.version).toBe(1);
    expect((await listProjectMilestones(db, worker, project.id)).map(({ title, state, owner_user_id }) => ({ title, state, owner_user_id })))
      .toEqual([{ title: "Complete supplier validation", state: "planned", owner_user_id: worker.userId }]);
    await expect(saveProjectMilestone(db, worker, project.id, null, projectMilestoneSchema.parse({
      title: "Unapproved milestone", dueAt: "2026-09-21T12:00:00Z", ownerUserId: worker.userId,
    }), key())).rejects.toMatchObject({ code: "PROJECT_OWNER_REQUIRED" });
    await expect(listProjectMilestones(db, outsider, project.id)).rejects.toMatchObject({ status: 404 });

    const revised = await saveProjectMilestone(db, requester, project.id, created.id, projectMilestoneSchema.parse({
      title: "Complete supplier validation",
      description: "Documents and receiving dates confirmed.",
      dueAt: "2026-09-22T12:00:00Z",
      ownerUserId: requester.userId,
      expectedVersion: 1,
      reason: "The project owner revised its responsible member and date.",
    }), key());
    expect(revised.version).toBe(2);
    const completed = await transitionProjectMilestone(db, requester, project.id, created.id, {
      action: "complete", expectedVersion: 2, reason: "Supplier validation was completed and checked.",
    }, key());
    expect(completed).toMatchObject({ version: 3, state: "completed" });
    const reopened = await transitionProjectMilestone(db, requester, project.id, created.id, {
      action: "reopen", expectedVersion: 3, reason: "The receiving date requires another confirmation.",
    }, key());
    expect(reopened).toMatchObject({ version: 4, state: "in_progress" });
    const events = (await db.prepare("SELECT action,milestone_version FROM staff_project_milestone_events WHERE milestone_id=? ORDER BY milestone_version")
      .bind(created.id).all<{ action: string; milestone_version: number }>()).results;
    expect(events).toEqual([
      { action: "created", milestone_version: 1 },
      { action: "revised", milestone_version: 2 },
      { action: "completed", milestone_version: 3 },
      { action: "reopened", milestone_version: 4 },
    ]);
    await expect(db.prepare("UPDATE staff_project_milestone_events SET reason='changed' WHERE milestone_id=?")
      .bind(created.id).run()).rejects.toThrow();
  });
  it("blocks task completion when a configured approver is unavailable", async () => {
    const id = await task();
    await action(requester, id, "submit");
    await action(worker, id, "start");
    await db
      .prepare("UPDATE hr_engagements SET status='suspended' WHERE person_id=?")
      .bind(`person-${head.userId}`)
      .run();
    const detail = await taskDetail(db, worker, id);
    expect(detail.blockedReason).toBeTruthy();
    expect(detail.permittedActions).not.toContain("complete");
  });
  it("delegates active work with an expiry, preserves audit/version guards, and restores the owner after expiry", async () => {
    const id = await task([], [head.userId]);
    const delegation = await delegateTask(
      db,
      requester,
      id,
      taskDelegationSchema.parse({
        delegateUserId: hr.userId,
        expiresAt: new Date(Date.now() + 3_600_000).toISOString(),
        expectedVersion: 1,
        reason: "Cover the receiving lead during leave.",
      }),
      key(),
    );
    expect(delegation.version).toBe(2);
    expect((await getTask(db, hr, id)).active_delegate_user_id).toBe(hr.userId);
    expect((await taskDetail(db, requester, id)).delegationHistory).toMatchObject([
      { id: delegation.delegationId, status: "active", reason: "Cover the receiving lead during leave.", revoke_reason: null },
    ]);
    await action(requester, id, "submit");
    expect((await taskDetail(db, hr, id)).permittedActions).toContain("start");
    await action(hr, id, "start");
    const current = await getTask(db, requester, id);
    const replayKey = key();
    await expect(revokeTaskDelegation(db, requester, id, taskDelegationRevokeSchema.parse({
      delegationId: delegation.delegationId,
      expectedVersion: current.version - 1,
      reason: "This stale decision must not be retained.",
    }), key())).rejects.toMatchObject({ status: 409 });
    expect((await taskDetail(db, requester, id)).delegationHistory[0].revoke_reason).toBeNull();
    const revoked = await revokeTaskDelegation(
      db,
      requester,
      id,
      taskDelegationRevokeSchema.parse({
        delegationId: delegation.delegationId,
        expectedVersion: current.version,
        reason: "Receiving lead returned.",
      }),
      replayKey,
    );
    expect(await revokeTaskDelegation(
      db,
      requester,
      id,
      taskDelegationRevokeSchema.parse({
        delegationId: delegation.delegationId,
        expectedVersion: current.version,
        reason: "Receiving lead returned.",
      }),
      replayKey,
    )).toEqual(revoked);
    await expect(getTask(db, hr, id)).rejects.toMatchObject({ status: 404 });
    await expect(taskDetail(db, hr, id)).rejects.toMatchObject({ status: 404 });
    await expect(taskDetail(db, outsider, id)).rejects.toMatchObject({ status: 404 });
    const history = (await taskDetail(db, worker, id)).delegationHistory;
    expect(history).toHaveLength(1);
    expect(history[0]).toMatchObject({ id: delegation.delegationId, status: "revoked", revoke_reason: "Receiving lead returned." });
    expect(history[0].revoked_at).toBeGreaterThan(0);
    await expect(db.prepare("UPDATE staff_task_delegations SET revoke_reason=? WHERE id=?").bind("Altered after revocation.", delegation.delegationId).run()).rejects.toThrow();
    expect((await taskDetail(db, worker, id)).delegationHistory[0].revoke_reason).toBe("Receiving lead returned.");
    expect((await taskDetail(db, worker, id)).permittedActions).toContain("save-progress");

    const expiring = await task([], [head.userId], worker.userId);
    const handoff = await delegateTask(
      db,
      requester,
      expiring,
      taskDelegationSchema.parse({
        delegateUserId: hr.userId,
        expiresAt: new Date(Date.now() + 3_600_000).toISOString(),
        expectedVersion: 1,
        reason: "Short coverage window.",
      }),
      key(),
    );
    await db.prepare("UPDATE staff_task_delegations SET expires_at=? WHERE id=?").bind(Date.now() - 1, handoff.delegationId).run();
    await expect(getTask(db, hr, expiring)).rejects.toMatchObject({ status: 404 });
    expect((await getTask(db, worker, expiring)).active_delegate_user_id).toBeNull();
    expect((await taskDetail(db, worker, expiring)).delegationHistory[0]).toMatchObject({
      id: handoff.delegationId, status: "expired", revoke_reason: null,
    });
  });
});
describe("Reusable levels and routine scheduling", () => {
  it("holds routine occurrences until Operations is enabled in production", async () => {
    const template = await saveTemplate(db, requester, null, templateSchema.parse({
      title: "Pilot room checks", assigneeUserId: worker.userId, completionApprovers: [requester.userId],
    }), key());
    const schedule = await saveSchedule(db, requester, String(template.id), null,
      scheduleSchema.parse({ frequency: "daily", startDate: "2026-01-01" }), key());
    await changeScheduleState(db, requester, String(schedule.id), 1, true, key());
    const at = Date.parse("2026-09-01T02:00:00Z");
    await db.prepare("UPDATE routine_schedules SET next_occurrence_at=? WHERE id=?").bind(at, schedule.id as string).run();
    vi.stubEnv("NODE_ENV", "production");
    try {
      expect(await generateOccurrences(db, at, 1)).toEqual({ generated: 0, examined: 0 });
      await db.prepare("INSERT INTO company_feature_rollouts(company_id,module_id,rollout_state,version,reason,changed_by,changed_at) VALUES(?,'operations','pilot',1,'Test routine generation','test',?)")
        .bind(company,Date.now()).run();
      expect((await generateOccurrences(db, at, 1)).generated).toBe(1);
    } finally { vi.unstubAllEnvs(); }
  });
  it("scopes Administration sections without granting staff approval management or beta ERP tools", async () => {
    const staffAccess = await administrationAccess(worker);
    expect(staffAccess.sections).toEqual(["feedback"]);
    expect(staffAccess.manageOrganization).toBe(false);
    expect((await administrationAccess(hr)).sections).toEqual([
      "people",
      "organization",
      "feedback",
    ]);
    expect((await administrationAccess(owner)).sections).toEqual([
      "people",
      "organization",
      "feedback",
    ]);
    expect(
      (await administrationAccess({ ...worker, beta: true })).sections,
    ).toEqual(["feedback"]);
    expect(
      (await administrationAccess({ ...owner, beta: true })).sections,
    ).toEqual([
      "people",
      "organization",
      "feedback",
    ]);
    expect(
      (
        await administrationAccess({
          ...owner,
          branchId: null,
          departmentId: null,
          scope: {
            kind: "holding",
            holdingId: "h001",
            holdingSlug: "test",
            holdingName: "Test",
            companyId: company,
          },
        })
      ).sections,
    ).toEqual([
      "people",
      "organization",
      "approvals",
      "usage",
      "ai",
      "feedback",
    ]);
  });
  it("retains level history and filters without conferring authority", async () => {
    const level = await saveLevel(
      db,
      owner,
      null,
      { code: "L9", name: "Senior", rank: 9, active: true, expectedVersion: 0 },
      key(),
    );
    await assignLevel(
      db,
      owner,
      `eng-${worker.userId}`,
      {
        levelId: String(level.id),
        expectedVersion: 1,
        reason: "Promotion review",
      },
      key(),
    );
    expect(
      (await staffPeople(db, company, { levelId: String(level.id) })).map(
        (p) => p.user_id,
      ),
    ).toEqual([worker.userId]);
    expect(
      (await staffPeople(db, company, { levelId: "unset" })).some(
        (person) => person.user_id === worker.userId,
      ),
    ).toBe(false);
    expect(
      await db
        .prepare(
          "SELECT count(*) n FROM staff_level_history WHERE engagement_id=?",
        )
        .bind(`eng-${worker.userId}`)
        .first(),
    ).toEqual({ n: 1 });
    const next = await saveLevel(
      db,
      owner,
      null,
      {
        code: "L10",
        name: "Principal",
        rank: 10,
        active: true,
        expectedVersion: 0,
      },
      key(),
    );
    await assignLevel(
      db,
      owner,
      `eng-${worker.userId}`,
      {
        levelId: String(next.id),
        expectedVersion: 2,
        reason: "Further progression",
      },
      key(),
    );
    expect(
      await db
        .prepare(
          "SELECT previous_level_id,level_id FROM staff_level_history WHERE engagement_id=? ORDER BY insertion_order DESC LIMIT 1",
        )
        .bind(`eng-${worker.userId}`)
        .first(),
    ).toEqual({ previous_level_id: level.id, level_id: next.id });
    await expect(
      assignLevel(
        db,
        owner,
        `eng-${worker.userId}`,
        {
          levelId: String(level.id),
          expectedVersion: 2,
          reason: "Stale correction",
        },
        key(),
      ),
    ).rejects.toMatchObject({ status: 409 });

    await expect(
      saveLevel(
        db,
        worker,
        null,
        {
          code: "TOP",
          name: "Top",
          rank: 999,
          active: true,
          expectedVersion: 0,
        },
        key(),
      ),
    ).rejects.toMatchObject({ status: 403 });
  });
  it("generates bounded independent occurrences, deduplicates concurrent runs and skips pauses", async () => {
    const template = await saveTemplate(
      db,
      requester,
      null,
      templateSchema.parse({
        title: "Daily checks",
        assigneeUserId: worker.userId,
        completionApprovers: [requester.userId],
      }),
      key(),
    );
    const schedule = await saveSchedule(
      db,
      requester,
      String(template.id),
      null,
      scheduleSchema.parse({ frequency: "daily", startDate: "2026-01-01" }),
      key(),
    );
    await changeScheduleState(
      db,
      requester,
      String(schedule.id),
      1,
      true,
      key(),
    );
    const at = Date.parse("2026-09-01T02:00:00Z");
    await db
      .prepare("UPDATE routine_schedules SET next_occurrence_at=? WHERE id=?")
      .bind(at, schedule.id as string)
      .run();
    // A paused interval in the backlog must never produce tasks.
    await db
      .prepare(
        "INSERT INTO routine_schedule_pauses(id,schedule_id,started_at,ended_at) VALUES(?,?,?,?)",
      )
      .bind(key(), schedule.id as string, at + 86400000, at + 3 * 86400000)
      .run();
    await Promise.all([
      generateOccurrences(db, at + 5 * 86400000, 4),
      generateOccurrences(db, at + 5 * 86400000, 4),
    ]);
    const tasks = (
      await db
        .prepare(
          "SELECT occurrence_at,state FROM staff_tasks WHERE schedule_id=? ORDER BY occurrence_at",
        )
        .bind(schedule.id as string)
        .all<{ occurrence_at: number; state: string }>()
    ).results;
    expect(tasks.length).toBeGreaterThan(1);
    expect(new Set(tasks.map((t) => t.occurrence_at)).size).toBe(tasks.length);
    expect(
      tasks.every(
        (t) => ![at + 86400000, at + 2 * 86400000].includes(t.occurrence_at),
      ),
    ).toBe(true);
    expect(tasks.every((t) => t.state === "ready")).toBe(true);
  });
  it("clones templates, keeps existing occurrences unchanged and surfaces unavailable assignment", async () => {
    const input = templateSchema.parse({
      title: "Weekly review",
      assigneeUserId: worker.userId,
      completionApprovers: [head.userId],
    });
    const source = await saveTemplate(db, requester, null, input, key());
    await expect(
      saveTemplate(db, outsider, String(source.id), input, key()),
    ).rejects.toMatchObject({ status: 403 });
    const clone = await saveTemplate(
      db,
      outsider,
      null,
      { ...input, title: "My review" },
      key(),
      String(source.id),
    );
    expect(clone.id).not.toBe(source.id);
    const first = await runTemplate(
      db,
      requester,
      String(source.id),
      Date.now() + 86400000,
      key(),
    );
    await saveTemplate(
      db,
      requester,
      String(source.id),
      { ...input, title: "Changed future review", expectedVersion: 1 },
      key(),
    );
    expect((await getTask(db, worker, String(first.id))).title).toBe(
      "Weekly review",
    );
    await db
      .prepare("UPDATE hr_engagements SET status='suspended' WHERE person_id=?")
      .bind(`person-${worker.userId}`)
      .run();
    const blocked = await runTemplate(
      db,
      requester,
      String(source.id),
      Date.now() + 86400000,
      key(),
    );
    expect(blocked.state).toBe("draft");
    expect(
      (await taskDetail(db, requester, String(blocked.id))).blockedReason,
    ).toBeTruthy();
  });
});

it('refreshes a persisted Work snapshot immediately after task submit, approval and reassignment', async () => {
  const { workSnapshot } = await import("@erpx/server-work_projection/public/work-items");
  const { closeRedisCache } = await import("@erpx/server-runtime/adapters/runtime/redis-cache");
  const redisUrl = process.env.ERPX_REDIS_URL;
  vi.stubEnv('NODE_ENV', 'development');
  try {
    const id = await task([head.userId], [head.userId]);
    const before = await workSnapshot(head);
    expect(before.unavailableSources).toEqual([]);
    expect(await workSnapshot(head)).toEqual(before);
    await action(requester, id, 'submit');
    const pending = await workSnapshot(head);
    expect(pending.items.find(item => item.id === id)?.canAct).toBe(true);
    await action(head, id, 'approve');
    const approved = await workSnapshot(head);
    expect(approved.items.find(item => item.id === id)?.canAct).not.toBe(true);
    expect((await workSnapshot(worker)).items.find(item => item.id === id)?.canAct).toBe(true);
    // Direct SQL represents an independently committed writer/job, not Redis invalidation in the API.
    await db.prepare('UPDATE staff_tasks SET assignee_user_id=?,version=version+1 WHERE id=?').bind(requester.userId, id).run();
    expect((await workSnapshot(worker)).items.find(item => item.id === id)?.canAct).not.toBe(true);
    expect((await workSnapshot(requester)).items.find(item => item.id === id)?.canAct).toBe(true);
    // Fail only this test process's ERPX Redis client; keep the shared native service running.
    process.env.ERPX_REDIS_URL = 'redis://127.0.0.1:1';
    await action(requester, id, 'cancel', 'Verify PostgreSQL fallback');
    expect((await workSnapshot(requester)).items.find(item => item.id === id)?.status).toBe('cancelled');
    if (redisUrl) process.env.ERPX_REDIS_URL = redisUrl;
    expect((await workSnapshot(requester)).items.find(item => item.id === id)?.status).toBe('cancelled');
  } finally { if (redisUrl === undefined) delete process.env.ERPX_REDIS_URL; else process.env.ERPX_REDIS_URL = redisUrl; await closeRedisCache(); vi.unstubAllEnvs(); }
});

// Raw Company duties deliberately do not use purchased, rollout or actor Work lists.
async function rawWorkObservation(packageId='work',now=Date.now()){
 const {withPackageSettlementSources}=await import('@erpx/server-commercial/public/package-settlement-sources'),operations=await import('@erpx/server-operations/public/work-package-obligations'),collaboration=await import('@erpx/server-collaboration/public/work-package-obligations');
 return withPackageSettlementSources(db,company,[lock=>operations.lockWorkOperationsPackageObligationSources(db,lock),lock=>collaboration.lockWorkCollaborationPackageObligationSources(db,lock)],async database=>{
  const input={companyId:company,packageId,packageVersion:1,now},operation=packageId==='work-messages'?null:await operations.workOperationsPackageObligations(db,database,{...input,moduleId:'operations'}),social=await collaboration.workCollaborationPackageObligations(db,database,{...input,moduleId:'collaboration'});
  return {operations:operation,collaboration:social,total:(operation?.outstanding??0)+social.outstanding};
 });
}
it('rawWork observes ordinary owned tasks and exact waiting deliveries once while excluding Project tasks and Leave approval foundations',async()=>{
 expect((await rawWorkObservation()).total).toBe(0);
 const ordinary=await task([],[]),project=await saveProject(db,requester,null,projectSchema.parse({title:'Separate purchased Project source',memberUserIds:[worker.userId]}),key()),projectTask=await saveTask(db,requester,null,taskSchema.parse({title:'Only actual Project work',assigneeUserId:worker.userId,projectId:String(project.id),dueAt:'2026-10-01T02:00:00Z'}),key());
 const leaveId=await leave();await submit(leaveId);
 const ordinaryNotice=key(),projectNotice=key(),leaveNotice=key();await db.batch([
  db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'task',?,1,'Actual pending Work delivery',?)").bind(ordinaryNotice,company,worker.userId,ordinary,Date.now()),
  db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'task',?,1,'Project foundation delivery',?)").bind(projectNotice,company,worker.userId,String(projectTask.id),Date.now()),
  db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'leave',?,1,'People foundation delivery',?)").bind(leaveNotice,company,head.userId,leaveId,Date.now())]);
 const current=await rawWorkObservation();expect(current.operations?.outstanding).toBe(1);expect(current.collaboration.outstanding).toBe(2);expect(current.total).toBe(3);expect(await db.prepare('SELECT count(*) n FROM inbox_deliveries WHERE id=?').bind(ordinaryNotice).first('n')).toBe(1);
 await action(requester,ordinary,'cancel','Original owner closes unnecessary ordinary task');const {changeInboxState}=await import('@erpx/server-collaboration/public/inbox');const deliveries=(await db.prepare("SELECT id,user_id FROM staff_notifications WHERE company_id=? AND record_type='task' AND record_id=? AND read_at IS NULL").bind(company,ordinary).all<{id:string;user_id:string}>()).results;for(const delivery of deliveries){const recipient=[requester,head,hr,owner,worker,outsider].find(actor=>actor.userId===delivery.user_id);expect(recipient).toBeDefined();await changeInboxState(db,recipient!,delivery.id,{read:true});}
 const terminal=await rawWorkObservation();expect(terminal.total).toBe(0);expect((await getTask(db,requester,String(projectTask.id))).state).not.toBe('cancelled');expect(await db.prepare('SELECT state FROM leave_requests WHERE id=?').bind(leaveId).first('state')).toBe('pending_approval');expect(terminal.operations?.revision).not.toBe(current.operations?.revision);expect(JSON.stringify(terminal)).not.toContain('Inspect deliveries');
});
it('rawWork keeps a published future routine and paused unfinished schedule as real duties under addon scope and rejects an unminted or foreign source lease',async()=>{
 const {workOperationsPackageObligations}=await import('@erpx/server-operations/public/work-package-obligations'),{withPackageSettlementSources}=await import('@erpx/server-commercial/public/package-settlement-sources'),{lockWorkOperationsPackageObligationSources}=await import('@erpx/server-operations/public/work-package-obligations');const input={companyId:company,packageId:'work-routines',packageVersion:1,moduleId:'operations',now:Date.now()};
 await expect(db.transaction(database=>workOperationsPackageObligations(db,database,input))).rejects.toThrow('WORK_OPERATIONS_OBLIGATION_SOURCE_REQUIRED');await expect(withPackageSettlementSources(db,company,[lock=>lockWorkOperationsPackageObligationSources(db,lock)],database=>workOperationsPackageObligations(db,database,{...input,companyId:'foreign-company'}))).rejects.toThrow('WORK_OPERATIONS_OBLIGATION_SOURCE_REQUIRED');
 const template=await saveTemplate(db,requester,null,templateSchema.parse({title:'Published future real Work routine',assigneeUserId:worker.userId,completionApprovers:[head.userId]}),key()),schedule=await saveSchedule(db,requester,String(template.id),null,scheduleSchema.parse({frequency:'daily',startDate:'2030-01-01'}),key());
 const pending=await rawWorkObservation('work-routines');expect(pending.operations?.outstanding).toBe(2);expect(pending.total).toBe(2);expect(await db.prepare('SELECT state FROM routine_schedules WHERE id=?').bind(String(schedule.id)).first('state')).toBe('paused');
 await db.prepare("UPDATE routine_schedules SET state='ended',next_occurrence_at=NULL,version=version+1 WHERE id=?").bind(String(schedule.id)).run();expect((await rawWorkObservation('work-routines')).operations?.outstanding).toBeGreaterThanOrEqual(2); // A forged ended label is not an original owner settlement; the published source also remains open.
});
it('rawWork messages distinguish an original atomic posted-message history receipt from unresolved native source lineage without reading another product inbox',async()=>{
 await applyMigrations(env.CONTROL_DB,env.CONTROL_TEST_MIGRATIONS);for(const actor of [owner,worker])await env.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,1,1)').bind(actor.userId,actor.name,`${actor.userId}@example.invalid`).run();const {createMessagingRoom,sendMessagingMessage,roomSchema,messageSchema}=await import('@erpx/server-collaboration/public/messaging');const room=await createMessagingRoom(db,owner,roomSchema.parse({kind:'direct',members:[worker.userId]}),key()),message=await sendMessagingMessage(db,owner,String(room.id),messageSchema.parse({body:'Actual atomically posted private message'}),key());
 const posted=await rawWorkObservation('work-messages');expect(posted.total).toBe(0);expect(posted.operations).toBeNull();expect(JSON.stringify(posted)).not.toContain('Actual atomically posted private message');
 await db.prepare('INSERT INTO messaging_messages(id,room_id,seq,sender_id,sender_name,body,created_at) VALUES(?,?,999,?,?,?,?)').bind(key(),String(room.id),owner.userId,owner.name,'Unknown unreceipted source',Date.now()).run();const invalid=await rawWorkObservation('work-messages');expect(invalid.collaboration.outstanding).toBe(1);expect(invalid.collaboration.revision).not.toBe(posted.collaboration.revision);expect(await db.prepare('SELECT id FROM messaging_messages WHERE id=?').bind(String(message.id)).first('id')).toBe(message.id);
});

async function workPackageState(state:'active'|'draining'='active'){
 const {packageGrantStore,readCompanyPackageGrants}=await import('@erpx/server-commercial/public/package-grants'),grants=await readCompanyPackageGrants(db,company),prior=grants.find(grant=>grant.packageId==='work'),store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-work-package-provider'}),obligations:async()=>{throw Error('Work withdrawal requires its real registered owner sources');}});
 return store.apply({expectedVersion:prior?.version??0,idempotencyKey:key(),grant:{companyId:company,packageId:'work',packageVersion:1,state,startsAt:0,endsAt:null,version:(prior?.version??0)+1,reason:'Actual Work owner lifecycle fixture',evidence:'test:work-native-owner-source'}});
}
async function workWithdrawalInput(){const {readCompanyPackageGrants,packageGrantRevision}=await import('@erpx/server-commercial/public/package-grants'),grants=await readCompanyPackageGrants(db,company),prior=grants.find(grant=>grant.packageId==='work')!;return {companyId:company,expectedRevision:packageGrantRevision(company,grants),idempotencyKey:key(),changes:[{expectedVersion:prior.version,grant:{...prior,state:'inactive' as const,version:prior.version+1,reason:'Original Work duties settled',evidence:'test:work-normal-preview-apply'}}]};}
async function workWithdrawalReview(input:Awaited<ReturnType<typeof workWithdrawalInput>>,apply=false){const {withOwnerPackageObligations,ownerPackageObligations}=await import('@erpx/server-commercial/public/owner-package-obligations'),{packageGrantStore}=await import('@erpx/server-commercial/public/package-grants'),store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-work-package-provider'}),obligations:(database,companyId,packageId)=>ownerPackageObligations(db,database,companyId,packageId)});return withOwnerPackageObligations(db,{companyId:company,packageId:'work',packageVersion:1},database=>apply?store.batch.applyIn(database,input,async()=>{}):store.batch.previewIn(database,input));}
async function readOriginalTaskNotices(id:string){const {changeInboxState}=await import('@erpx/server-collaboration/public/inbox'),deliveries=(await db.prepare("SELECT id,user_id FROM staff_notifications WHERE company_id=? AND record_type='task' AND record_id=? AND read_at IS NULL").bind(company,id).all<{id:string;user_id:string}>()).results;for(const delivery of deliveries){const recipient=[requester,head,hr,owner,worker,outsider].find(actor=>actor.userId===delivery.user_id);expect(recipient).toBeDefined();await changeInboxState(db,recipient!,delivery.id,{read:true});}}
it('rawWork normal registered Preview Apply retains history only after actual owner cancellation and pending Work delivery review',async()=>{
 await workPackageState();const ordinary=await task([],[]);await workPackageState('draining');await expect(workWithdrawalReview(await workWithdrawalInput())).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
 await action(requester,ordinary,'cancel','Original Work source no longer required');await expect(workWithdrawalReview(await workWithdrawalInput())).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');await readOriginalTaskNotices(ordinary);expect((await rawWorkObservation()).total).toBe(0);
 const input=await workWithdrawalInput(),preview=await workWithdrawalReview(input),applied=await workWithdrawalReview(input,true);expect(applied).toEqual(preview);expect(await workWithdrawalReview(input,true)).toEqual(applied);
 const {readPurchasedAccess}=await import('@erpx/server-commercial/public/purchase-command-boundary');expect(await readPurchasedAccess(db,'enforced',{companyId:company,capability:'work.actions'})).toMatchObject({newWork:false,settleExisting:false,history:true});expect((await getTask(db,requester,ordinary)).state).toBe('cancelled');
 const {withOwnerPackageObligations,readOwnerPackageObligations}=await import('@erpx/server-commercial/public/owner-package-obligations');const messages=await withOwnerPackageObligations(db,{companyId:company,packageId:'work-messages',packageVersion:1},database=>readOwnerPackageObligations(db,database,{companyId:company,packageId:'work-messages',packageVersion:1,now:Date.now()}));expect(messages.observations.map(row=>row.moduleId)).toEqual(['collaboration']);
});
it('rawWork rejects an actual late deferred ordinary task duty and rolls the normal withdrawal history audit receipt and owner row back before exact retry',async()=>{
 await workPackageState();await workPackageState('draining');expect((await rawWorkObservation()).total).toBe(0);const input=await workWithdrawalInput(),fn=`work_withdrawal_${key().replaceAll('-','')}`,late=`late-work-${key()}`,tables=['company_package_grants','company_package_grant_events','company_package_history_rights','company_package_change_sets','company_package_revisions','company_package_review_evidence','staff_tasks'],snapshot=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT row_to_json(r)::text image FROM ${table} r ORDER BY row_to_json(r)::text`).all()).results)),before=await snapshot();
 await db.exec(`CREATE SEQUENCE ${fn}_attempt;CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fn}_attempt');INSERT INTO staff_tasks(id,company_id,branch_id,department_id,title,creator_user_id,assignee_user_id,due_at,created_at,updated_at) VALUES('${late}','${company}','${branch}','${department}','Actual deferred Work duty','${requester.userId}','${worker.userId}',1,1,1);RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fn} AFTER UPDATE ON company_package_grants DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.company_id='${company}' AND NEW.package_id='work' AND NEW.state='inactive') EXECUTE FUNCTION ${fn}()`);
 try{await expect(workWithdrawalReview(input,true)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');expect(await snapshot()).toEqual(before);expect(await db.prepare(`SELECT is_called FROM ${fn}_attempt`).first('is_called')).toBe(true);}finally{await db.exec(`DROP TRIGGER ${fn} ON company_package_grants;DROP FUNCTION ${fn}();DROP SEQUENCE ${fn}_attempt`);}
 const preview=await workWithdrawalReview(input);expect(await workWithdrawalReview(input,true)).toEqual(preview);expect(await workWithdrawalReview(input,true)).toEqual(preview);
});

it('purchasedWork ordinary task keeps original modules-empty staff and independent owner policy across no purchase active draining inactive and exact history replay',async()=>{
 vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');const input=taskSchema.parse({title:'Actual ordinary purchased Work',assigneeUserId:worker.userId,startApprovers:[],completionApprovers:[]}),command=key();await expect(saveTask(db,requester,null,input,command)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});expect(await db.prepare('SELECT id FROM staff_tasks WHERE id=?').bind(`task-${command}`).first()).toBeNull();
 await workPackageState();const created=await saveTask(db,requester,null,input,command),id=String(created.id);expect(requester.modules).toEqual([]);expect((await getTask(db,worker,id)).state).toBe('ready');await expect(taskDetail(db,outsider,id)).rejects.toMatchObject({status:404});
 await workPackageState('draining');await expect(saveTask(db,requester,id,taskSchema.parse({...input,expectedVersion:1,title:'Unlicensed later amendment'}),key(),true)).rejects.toMatchObject({code:'PACKAGE_DRAINING'});expect((await taskDetail(db,requester,id)).permittedActions).not.toContain('edit');
 await action(worker,id,'start','Start the original retained Work');await action(worker,id,'complete','Original ordinary work is complete');await readOriginalTaskNotices(id);const withdrawal=await workWithdrawalInput();await workWithdrawalReview(withdrawal);await workWithdrawalReview(withdrawal,true);expect((await getTask(db,requester,id)).state).toBe('completed');expect((await taskDetail(db,requester,id)).permittedActions).toEqual([]);expect(await saveTask(db,requester,null,input,command)).toEqual(created);expect(await db.prepare('SELECT count(*) n FROM command_receipts WHERE command_id=?').bind(command).first('n')).toBe(1);
 await expect(saveTask(db,requester,null,{...input,title:'No new inactive Work'},key())).rejects.toMatchObject({code:'PACKAGE_INACTIVE'});
});
it('purchasedWork chooses actual stored Project ownership and keeps Projects-only worker foundation with no Work or People purchase before list limits',async()=>{
 const ordinary=await task([],[]);const {packageGrantStore}=await import('@erpx/server-commercial/public/package-grants'),store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-project-foundation-provider'}),obligations:async()=>{throw Error('Not a withdrawal');}});await store.apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:company,packageId:'projects',packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Only actual Projects foundation',evidence:'test:project-only-work-boundary'}});
 vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');const project=await saveProject(db,requester,null,projectSchema.parse({title:'Actual Project owner only',memberUserIds:[worker.userId]}),key()),projectInput=taskSchema.parse({title:'Included Project task',assigneeUserId:worker.userId,projectId:String(project.id),dueAt:'2026-10-01T02:00:00Z'}),projectTask=await saveTask(db,requester,null,projectInput,key());
 expect((await getTask(db,worker,String(projectTask.id))).project_id).toBe(project.id);await expect(getTask(db,requester,ordinary)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await expect(saveTask(db,requester,ordinary,taskSchema.parse({...projectInput,expectedVersion:1}),key(),true)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await expect(saveTask(db,requester,String(projectTask.id),taskSchema.parse({...projectInput,projectId:null,expectedVersion:1}),key(),true)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 await db.prepare("INSERT INTO staff_tasks(id,company_id,branch_id,department_id,title,creator_user_id,assignee_user_id,due_at,created_at,updated_at) SELECT 'unlicensed-'||?||'-'||generate_series,?,?,?,'Newer unlicensed ordinary Work',?,?,1,1,?::bigint+generate_series FROM generate_series(1,300)").bind(key(),company,branch,department,requester.userId,worker.userId,Date.now()+10000).run();expect((await listTasks(db,requester)).map(row=>row.id)).toEqual([projectTask.id]);expect((await taskDetail(db,requester,String(projectTask.id))).task.id).toBe(projectTask.id);expect(await db.prepare('SELECT package_id FROM company_package_grants WHERE company_id=? ORDER BY package_id').bind(company).all()).toMatchObject({results:[{package_id:'projects'}]});
});
it('purchasedWork repeats actual task source and current IAM after all deferred effects with whole owner rollback before exact retry',async()=>{
 await workPackageState();vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');
 for(const fault of ['source','authority'] as const){const created=await saveTask(db,requester,null,taskSchema.parse({title:`Initial ${fault} Work source`,assigneeUserId:worker.userId,startApprovers:[],completionApprovers:[]}),key()),id=String(created.id),command=key(),input=taskSchema.parse({title:`Reviewed ${fault} task update`,assigneeUserId:worker.userId,startApprovers:[],completionApprovers:[],expectedVersion:1}),fn=`work_task_${key().replaceAll('-','')}`,tables=['staff_tasks','staff_task_delegations','assignments','command_receipts','audit_events','outbox_events','staff_notifications','approval_instances','approval_steps','approval_decisions'],snapshot=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT row_to_json(r)::text image FROM ${table} r ORDER BY row_to_json(r)::text`).all()).results)),before=await snapshot(),effect=fault==='source'?`UPDATE staff_tasks SET title='Unreviewed deferred source' WHERE id='${id}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${requester.assignmentId}'`;
  await db.exec(`CREATE SEQUENCE ${fn}_attempt;CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fn}_attempt');${effect};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fn} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fn}()`);
  try{await expect(saveTask(db,requester,id,input,command,true)).rejects.toMatchObject({code:fault==='source'?'WORK_TASK_SOURCE_CHANGED':'WORK_TASK_AUTHORITY_CHANGED'});expect(await snapshot()).toEqual(before);expect(await db.prepare(`SELECT is_called FROM ${fn}_attempt`).first('is_called')).toBe(true);}finally{await db.exec(`DROP TRIGGER ${fn} ON command_receipts;DROP FUNCTION ${fn}();DROP SEQUENCE ${fn}_attempt`);}
  const accepted=await saveTask(db,requester,id,input,command,true);expect(await saveTask(db,requester,id,input,command,true)).toEqual(accepted);expect(await db.prepare('SELECT count(*) n FROM command_receipts WHERE command_id=?').bind(command).first('n')).toBe(1);expect((await getTask(db,requester,id)).title).toBe(input.title);
 }
});
it('purchasedWork repeats actual capability time after deferred task effects and preserves original grants and exact command before a normal renewal retry',async()=>{
 await workPackageState();const {packageGrantStore,readCompanyPackageGrants}=await import('@erpx/server-commercial/public/package-grants'),prior=(await readCompanyPackageGrants(db,company)).find(grant=>grant.packageId==='work')!,store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-work-expiry-provider'}),obligations:async()=>{throw Error('Expiry fixture extends no settlement');}});await store.apply({expectedVersion:prior.version,idempotencyKey:key(),grant:{...prior,version:prior.version+1,endsAt:Date.now()+5000,reason:'Bounded actual current capability clock',evidence:'test:work-natural-expiry'}});
 vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');const input=taskSchema.parse({title:'Purchase must remain current through original effects',assigneeUserId:worker.userId,startApprovers:[],completionApprovers:[]}),command=key(),id=`task-${command}`,fn=`work_purchase_${key().replaceAll('-','')}`,tables=['company_package_grants','company_package_grant_events','staff_tasks','command_receipts','audit_events','outbox_events','staff_notifications'],snapshot=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT row_to_json(r)::text image FROM ${table} r ORDER BY row_to_json(r)::text`).all()).results)),before=await snapshot();
 await db.exec(`CREATE SEQUENCE ${fn}_attempt;CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fn}_attempt');PERFORM pg_sleep(5.2);RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fn} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fn}()`);
 try{await expect(saveTask(db,requester,null,input,command)).rejects.toMatchObject({code:'PACKAGE_CAPABILITY_CHANGED'});expect(await snapshot()).toEqual(before);expect(await db.prepare(`SELECT is_called FROM ${fn}_attempt`).first('is_called')).toBe(true);}finally{await db.exec(`DROP TRIGGER ${fn} ON command_receipts;DROP FUNCTION ${fn}();DROP SEQUENCE ${fn}_attempt`);}
 await workPackageState();const result=await saveTask(db,requester,null,input,command);expect(result.id).toBe(id);expect(await saveTask(db,requester,null,input,command)).toEqual(result);
});

async function workRoutinePackage(state:'active'|'draining'='active'){
 const {packageGrantStore,readCompanyPackageGrants}=await import('@erpx/server-commercial/public/package-grants'),prior=(await readCompanyPackageGrants(db,company)).find(row=>row.packageId==='work-routines');return packageGrantStore(db,{authorize:async()=>({userId:'isolated-routine-provider'}),obligations:async()=>{throw Error('No synthetic routine closure');}}).apply({expectedVersion:prior?.version??0,idempotencyKey:key(),grant:{companyId:company,packageId:'work-routines',packageVersion:1,state,startsAt:0,endsAt:null,version:(prior?.version??0)+1,reason:'Original native routine source',evidence:'test:work-routine-source'}});
}
async function purchasedRoutine(){const input=templateSchema.parse({title:'Original captured routine authority',assigneeUserId:worker.userId,completionApprovers:[head.userId]}),created=await saveTemplate(db,requester,null,input,key()),createdSchedule=await saveSchedule(db,requester,String(created.id),null,scheduleSchema.parse({frequency:'daily',startDate:'2030-01-01'}),key()),resumed=await changeScheduleState(db,requester,String(createdSchedule.id),1,true,key()),at=await db.prepare('SELECT next_occurrence_at FROM routine_schedules WHERE id=?').bind(createdSchedule.id).first<number>('next_occurrence_at');return {input,created,createdSchedule,resumed,at:at!};}
it('purchasedRoutine captures original modules-empty authority and contiguous native cursor journals then settles future schedules through original end and unpublish while draining',async()=>{
 vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');const input=templateSchema.parse({title:'Original captured routine authority',assigneeUserId:worker.userId,completionApprovers:[head.userId]});await expect(saveTemplate(db,requester,null,input,key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await workPackageState();await expect(saveTemplate(db,requester,null,input,key())).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});await workRoutinePackage();const r=await purchasedRoutine();expect(requester.modules).toEqual([]);expect(await generateOccurrences(db,r.at+86400000,2)).toEqual({generated:2,examined:2});
 const steps=(await db.prepare('SELECT generated,task_id,occurrence_at,next_occurrence_at FROM work_routine_execution_steps WHERE schedule_id=? ORDER BY occurrence_at').bind(r.createdSchedule.id).all<{generated:boolean;task_id:string;occurrence_at:number;next_occurrence_at:number}>()).results;expect(steps).toHaveLength(2);expect(steps.map(row=>row.generated)).toEqual([true,true]);expect(steps[0].occurrence_at).toBe(r.at);expect(steps[1].occurrence_at).toBe(steps[0].next_occurrence_at);expect(await db.prepare("SELECT count(*) n FROM audit_events WHERE action='routine.occurrence' AND record_id IN(SELECT id FROM staff_tasks WHERE schedule_id=?)").bind(r.createdSchedule.id).first('n')).toBe(2);expect(await generateOccurrences(db,r.at+86400000,2)).toEqual({generated:0,examined:0});
 await workRoutinePackage('draining');await expect(runTemplate(db,requester,String(r.created.id),r.at+86400000,key())).rejects.toMatchObject({code:'PACKAGE_DRAINING'});const cursor=await db.prepare('SELECT next_occurrence_at FROM routine_schedules WHERE id=?').bind(r.createdSchedule.id).first<number>('next_occurrence_at');expect(await generateOccurrences(db,cursor!,1)).toEqual({generated:0,examined:0});expect(await db.prepare('SELECT next_occurrence_at FROM routine_schedules WHERE id=?').bind(r.createdSchedule.id).first('next_occurrence_at')).toBe(cursor);
 const {endSchedule,getTemplate}=await import('@erpx/server-operations/public/routines'),endKey=key(),ended=await endSchedule(db,requester,String(r.createdSchedule.id),2,endKey);expect(ended).toMatchObject({state:'ended',version:3});expect(await endSchedule(db,requester,String(r.createdSchedule.id),2,endKey)).toEqual(ended);await saveTemplate(db,requester,String(r.created.id),{...r.input,published:false,expectedVersion:1},key());expect((await getTemplate(db,requester,String(r.created.id))).published).toBe(0);
 for(const step of steps){await action(requester,step.task_id,'cancel','Original future routine task no longer required');await readOriginalTaskNotices(step.task_id);}expect((await rawWorkObservation('work-routines')).total).toBe(0);
 const {readCompanyPackageGrants,packageGrantRevision,packageGrantStore}=await import('@erpx/server-commercial/public/package-grants'),{withOwnerPackageObligations,ownerPackageObligations}=await import('@erpx/server-commercial/public/owner-package-obligations'),grants=await readCompanyPackageGrants(db,company),prior=grants.find(row=>row.packageId==='work-routines')!,withdrawal={companyId:company,expectedRevision:packageGrantRevision(company,grants),idempotencyKey:key(),changes:[{expectedVersion:prior.version,grant:{...prior,state:'inactive' as const,version:prior.version+1,reason:'Native routine and delivery duties settled',evidence:'test:work-routines-normal-withdrawal'}}]},store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-routine-provider'}),obligations:(database,companyId,packageId)=>ownerPackageObligations(db,database,companyId,packageId)}),review=(apply=false)=>withOwnerPackageObligations(db,{companyId:company,packageId:'work-routines',packageVersion:1},database=>apply?store.batch.applyIn(database,withdrawal,async()=>{}):store.batch.previewIn(database,withdrawal));const preview=await review();expect(await review(true)).toEqual(preview);expect((await getTemplate(db,requester,String(r.created.id))).published).toBe(0);await expect(saveTemplate(db,requester,null,r.input,key())).rejects.toMatchObject({code:'PACKAGE_INACTIVE'});
});
it('purchasedRoutine repeats captured exact source and current authorizer after deferred original generation effects and retries the same native cursor once',async()=>{
 await workPackageState();await workRoutinePackage();vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');
 for(const fault of ['source','authority'] as const){const r=await purchasedRoutine(),taskId=`task-${r.createdSchedule.id}-${r.at}`,fn=`routine_fault_${key().replaceAll('-','')}`,mutation=fault==='source'?`UPDATE routine_templates SET description='Late unreviewed source' WHERE id='${r.created.id}'`:`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${requester.assignmentId}'`;
  const snapshot=()=>db.prepare(`SELECT jsonb_build_object('template',(SELECT to_jsonb(t) FROM routine_templates t WHERE id=?),'schedule',(SELECT to_jsonb(s)-ARRAY['last_error','updated_at'] FROM routine_schedules s WHERE id=?),'task',(SELECT to_jsonb(t) FROM staff_tasks t WHERE id=?),'actor',(SELECT to_jsonb(a) FROM assignments a WHERE id=?),'steps',(SELECT jsonb_agg(to_jsonb(s) ORDER BY command_id) FROM work_routine_execution_steps s WHERE schedule_id=?),'outbox',(SELECT jsonb_agg(to_jsonb(o) ORDER BY id) FROM outbox_events o WHERE aggregate_id=?),'notices',(SELECT jsonb_agg(to_jsonb(n) ORDER BY id) FROM staff_notifications n WHERE record_id=?))::text image`).bind(r.created.id,r.createdSchedule.id,taskId,requester.assignmentId,r.createdSchedule.id,taskId,taskId).first<string>('image'),before=await snapshot();await db.exec(`CREATE SEQUENCE ${fn}_fired;CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fn}_fired');${mutation};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fn} AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.aggregate_id='${taskId}' AND NEW.event_type='task.generated') EXECUTE FUNCTION ${fn}()`);
  try{expect(await generateOccurrences(db,r.at,1)).toEqual({generated:0,examined:1});expect(await snapshot()).toBe(before);expect(await db.prepare(`SELECT is_called FROM ${fn}_fired`).first('is_called')).toBe(true);expect(await db.prepare('SELECT last_error FROM routine_schedules WHERE id=?').bind(r.createdSchedule.id).first<string>('last_error')).toContain(fault==='source'?'original generated occurrence':'original routine assignment');}finally{await db.exec(`DROP TRIGGER ${fn} ON outbox_events;DROP FUNCTION ${fn}();DROP SEQUENCE ${fn}_fired`);}
  expect(await generateOccurrences(db,r.at,1)).toEqual({generated:1,examined:1});expect(await db.prepare('SELECT count(*) n FROM work_routine_execution_steps WHERE schedule_id=?').bind(r.createdSchedule.id).first('n')).toBe(1);expect(await db.prepare('SELECT count(*) n FROM staff_tasks WHERE id=?').bind(taskId).first('n')).toBe(1);const {endSchedule}=await import('@erpx/server-operations/public/routines');await endSchedule(db,requester,String(r.createdSchedule.id),2,key());
 }
});
it('purchasedRoutine preserves actual Project-only native generation without Work or People grants and rejects a forged cursor with no contiguous original owner journal',async()=>{
 const {packageGrantStore}=await import('@erpx/server-commercial/public/package-grants');await packageGrantStore(db,{authorize:async()=>({userId:'isolated-project-routine-provider'}),obligations:async()=>{throw Error('No synthetic project closure');}}).apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:company,packageId:'projects',packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Actual Projects included task foundation',evidence:'test:project-only-routine'}});vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');const project=await saveProject(db,requester,null,projectSchema.parse({title:'Actual Projects recurring task source',memberUserIds:[worker.userId]}),key()),source=await saveTemplate(db,requester,null,templateSchema.parse({title:'Actual Project-only routine',projectId:String(project.id),assigneeUserId:worker.userId,completionApprovers:[head.userId]}),key()),s=await saveSchedule(db,requester,String(source.id),null,scheduleSchema.parse({frequency:'daily',startDate:'2030-01-01'}),key());await changeScheduleState(db,requester,String(s.id),1,true,key());const at=await db.prepare('SELECT next_occurrence_at FROM routine_schedules WHERE id=?').bind(s.id).first<number>('next_occurrence_at');expect(await generateOccurrences(db,at!,1)).toEqual({generated:1,examined:1});expect(await db.prepare('SELECT project_id FROM staff_tasks WHERE schedule_id=?').bind(s.id).first('project_id')).toBe(project.id);
 const cursor=await db.prepare('SELECT next_occurrence_at FROM routine_schedules WHERE id=?').bind(s.id).first<number>('next_occurrence_at');await db.prepare('UPDATE routine_schedules SET next_occurrence_at=? WHERE id=?').bind(cursor!+86400000,s.id).run();expect(await generateOccurrences(db,cursor!+86400000,1)).toEqual({generated:0,examined:0});expect(await db.prepare('SELECT count(*) n FROM staff_tasks WHERE schedule_id=?').bind(s.id).first('n')).toBe(1);expect(await db.prepare('SELECT count(*) n FROM work_routine_execution_steps WHERE schedule_id=?').bind(s.id).first('n')).toBe(1);expect((await db.prepare('SELECT package_id FROM company_package_grants WHERE company_id=? ORDER BY package_id').bind(company).all()).results).toEqual([{package_id:'projects'}]);
});
it('purchasedRoutine filters35 actual legacy queues without inferred authority before20 candidates and stops a captured queue after current assignment revocation',async()=>{
 await workPackageState();await workRoutinePackage();const legacy=await saveTemplate(db,requester,null,templateSchema.parse({title:'Original unbound legacy schedule source',assigneeUserId:worker.userId,completionApprovers:[head.userId]}),key());await db.prepare("INSERT INTO routine_schedules(id,template_id,company_id,frequency,timezone,occurrence_time,start_date,next_occurrence_at,state,created_at,updated_at) SELECT 'schedule-legacy-'||?||'-'||n,?,?,'daily','Asia/Bangkok','09:00','2029-01-01',?,'active',1,1 FROM generate_series(1,35)n").bind(key(),legacy.id,company,Date.parse('2029-01-01T02:00:00Z')).run();vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');const r=await purchasedRoutine();expect(await generateOccurrences(db,r.at,1)).toEqual({generated:1,examined:1});expect(await db.prepare('SELECT count(*) n FROM staff_tasks WHERE template_id=?').bind(legacy.id).first('n')).toBe(0);expect(await db.prepare('SELECT count(*) n FROM work_routine_execution_authorizations WHERE template_id=?').bind(legacy.id).first('n')).toBe(0);
 await db.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=?').bind(Date.now(),requester.assignmentId).run();expect(await generateOccurrences(db,r.at+86400000,1)).toEqual({generated:0,examined:0});expect(await db.prepare('SELECT count(*) n FROM staff_tasks WHERE schedule_id=?').bind(r.createdSchedule.id).first('n')).toBe(1);expect(await db.prepare('SELECT count(*) n FROM assignments WHERE company_id=?').bind(company).first('n')).toBe(6);
});
it('purchasedRoutine commits original schedule authority for two Companies with source writer leases and no hidden deadlock retry',async()=>{
 await workPackageState();await workRoutinePackage();vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');const first=await purchasedRoutine();await changeScheduleState(db,requester,String(first.createdSchedule.id),2,false,key());const {expenseFixture}=await import('../../support/helpers/expense-fixture'),other=await expenseFixture();other.owner.modules=[];const {packageGrantStore}=await import('@erpx/server-commercial/public/package-grants'),store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-second-routine-provider'}),obligations:async()=>{throw Error('No unrelated withdrawal');}});for(const packageId of ['work','work-routines'])await store.apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:other.company,packageId,packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Actual separate Company routine purchase',evidence:'test:two-company-routine'}});const source=await saveTemplate(db,other.owner,null,templateSchema.parse({title:'Separate Company actual routine',assigneeUserId:other.claimant.userId,completionApprovers:[other.head.userId]}),key()),second=await saveSchedule(db,other.owner,String(source.id),null,scheduleSchema.parse({frequency:'daily',startDate:'2030-01-01'}),key()),fn=`routine_parallel_${key().replaceAll('-','')}`,commands=[key(),key()];await db.exec(`CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(0.1);RETURN NEW;END $$;CREATE TRIGGER ${fn} BEFORE UPDATE ON routine_schedules FOR EACH ROW WHEN(NEW.company_id IN('${company}','${other.company}') AND NEW.version>OLD.version) EXECUTE FUNCTION ${fn}()`);
 try{const {measureReads}=await import('@erpx/server-runtime/public/runtime/read-metrics'),observed=await measureReads(()=>Promise.all([changeScheduleState(db,requester,String(first.createdSchedule.id),3,true,commands[0]),changeScheduleState(db,other.owner,String(second.id),1,true,commands[1])]));expect(observed.result).toHaveLength(2);expect(observed.metrics.deadlockFailures??0).toBe(0);}finally{await db.exec(`DROP TRIGGER ${fn} ON routine_schedules;DROP FUNCTION ${fn}()`);}for(const [actor,id,version,command] of [[requester,first.createdSchedule.id,4,commands[0]],[other.owner,second.id,2,commands[1]]] as const){expect(await db.prepare('SELECT company_id,state,version FROM routine_schedules WHERE id=?').bind(id).first()).toEqual({company_id:actor.companyId,state:'active',version});expect(await db.prepare('SELECT company_id,actor_id,assignment_id,schedule_version FROM work_routine_execution_authorizations WHERE command_id=?').bind(command).first()).toEqual({company_id:actor.companyId,actor_id:actor.userId,assignment_id:actor.assignmentId,schedule_version:version});const {endSchedule}=await import('@erpx/server-operations/public/routines');await endSchedule(db,actor,String(id),version,key());}
});
it('purchasedRoutine upgrades populated1051 schedules additively without inferring authority and preserves disabled original jobs and schema-compatible rollback',async()=>{
 const {mkdtemp,mkdir,readdir,symlink,rm}=await import('node:fs/promises'),{join,resolve}=await import('node:path'),{migrateDatabase}=await import('@erpx/server-runtime/public/runtime/migrations'),root=await mkdtemp(resolve('.runtime/tests/work-routine-upgrade-')),database=env.H002_DB,companyId=`legacy-routine-${key()}`,templateId=`routine-${key()}`,scheduleId=`schedule-${key()}`,at=Date.parse('2030-01-01T02:00:00Z');
 try{await mkdir(join(root,'holding-write'));for(const file of await readdir('migrations/postgres/holding-write'))if(file.endsWith('.sql')&&Number(file.split('_')[0])<1052)await symlink(resolve('migrations/postgres/holding-write',file),join(root,'holding-write',file));await migrateDatabase(database,'holding-write',root);await database.prepare("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES(?,'company',?,'Populated original routine Company',?)").bind(companyId,companyId,companyId).run();
  for(const name of ['creator','worker','reviewer']){const id=`${companyId}-${name}`;await database.batch([database.prepare("INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES(?,?,'employee','[]',?,0,9999999999999,'holding')").bind(`grant-${id}`,id,companyId),database.prepare("INSERT INTO hr_people(id,user_id,company_id,legal_name,country_code,created_by,created_at,updated_at) VALUES(?,?,?,?,'TH','fixture',1,1)").bind(`person-${id}`,id,companyId,name),database.prepare("INSERT INTO hr_engagements(id,person_id,company_id,worker_type,employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) VALUES(?,?,?,'employee','salaried','active','2026-01-01','Staff','THB','fixture',1,1)").bind(`eng-${id}`,`person-${id}`,companyId)]);}
  await database.prepare('INSERT INTO routine_templates(id,company_id,title,description,creator_user_id,assignee_user_id,start_approvers_json,completion_approvers_json,published,created_at,updated_at) VALUES(?,?,?,?,?,?,?, ?,1,1,1)').bind(templateId,companyId,'Populated immutable source history','Original retained routine body',`${companyId}-creator`,`${companyId}-worker`,'[]',JSON.stringify([`${companyId}-reviewer`])).run();await database.prepare("INSERT INTO routine_schedules(id,template_id,company_id,frequency,timezone,occurrence_time,start_date,next_occurrence_at,state,created_at,updated_at) VALUES(?,?,?,'daily','Asia/Bangkok','09:00','2030-01-01',?,'active',1,1)").bind(scheduleId,templateId,companyId,at).run();expect(await database.prepare("SELECT to_regclass('work_routine_execution_authorizations')").first('to_regclass')).toBeNull();expect(await generateOccurrences(database,at,1)).toEqual({generated:1,examined:1});
  const snapshot=()=>database.prepare(`SELECT jsonb_build_object('template',(SELECT to_jsonb(t) FROM routine_templates t WHERE id=?),'schedule',(SELECT to_jsonb(s) FROM routine_schedules s WHERE id=?),'task',(SELECT jsonb_agg(to_jsonb(t) ORDER BY id) FROM staff_tasks t WHERE schedule_id=?),'roles',(SELECT jsonb_agg(to_jsonb(a) ORDER BY id) FROM assignments a WHERE company_id=?),'audit',(SELECT jsonb_agg(to_jsonb(a) ORDER BY id) FROM audit_events a),'grants',(SELECT jsonb_agg(to_jsonb(g) ORDER BY package_id) FROM company_package_grants g),'rollouts',(SELECT jsonb_agg(to_jsonb(r) ORDER BY company_id,module_id) FROM company_feature_rollouts r))::text image`).bind(templateId,scheduleId,scheduleId,companyId).first<string>('image'),before=await snapshot(),receipts=(await database.prepare('SELECT name,checksum FROM erpx_migrations ORDER BY name').all()).results;const addition=(await readdir('migrations/postgres/holding-write')).filter(file=>file.startsWith('1052_')&&file.endsWith('.sql'));expect(addition).toHaveLength(1);await symlink(resolve('migrations/postgres/holding-write',addition[0]),join(root,'holding-write',addition[0]));await migrateDatabase(database,'holding-write',root);expect(await snapshot()).toBe(before);expect((await database.prepare("SELECT name,checksum FROM erpx_migrations WHERE name NOT LIKE '1052_%' ORDER BY name").all()).results).toEqual(receipts);expect(await database.prepare('SELECT count(*) n FROM work_routine_execution_authorizations').first('n')).toBe(0);expect(await database.prepare('SELECT count(*) n FROM work_routine_execution_steps').first('n')).toBe(0);await migrateDatabase(database,'holding-write',root);expect(await snapshot()).toBe(before);
  vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');expect(await generateOccurrences(database,at+86400000,1)).toEqual({generated:0,examined:0});expect(await snapshot()).toBe(before);expect(await database.prepare('SELECT count(*) n FROM company_package_grants').first('n')).toBe(0);expect(await database.prepare('SELECT count(*) n FROM company_feature_rollouts').first('n')).toBe(0);
 }finally{await rm(root,{recursive:true,force:true});}
});
it('purchasedRoutine repeats actual new-work expiry after original deferred generation and rolls back its native task cursor and private authority step before renewal retry',async()=>{
 await workPackageState();await workRoutinePackage();vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');const r=await purchasedRoutine(),{packageGrantStore,readCompanyPackageGrants}=await import('@erpx/server-commercial/public/package-grants'),prior=(await readCompanyPackageGrants(db,company)).find(row=>row.packageId==='work-routines')!,store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-routine-expiry-provider'}),obligations:async()=>{throw Error('No synthetic closure');}});await store.apply({expectedVersion:prior.version,idempotencyKey:key(),grant:{...prior,version:prior.version+1,endsAt:Date.now()+5000,reason:'Current actual queued routine clock',evidence:'test:routine-actual-expiry'}});const taskId=`task-${r.createdSchedule.id}-${r.at}`,fn=`routine_expiry_${key().replaceAll('-','')}`,snapshot=()=>db.prepare(`SELECT jsonb_build_object('task',(SELECT to_jsonb(t) FROM staff_tasks t WHERE id=?),'cursor',(SELECT to_jsonb(s)-ARRAY['last_error','updated_at'] FROM routine_schedules s WHERE id=?),'steps',(SELECT jsonb_agg(to_jsonb(s) ORDER BY command_id) FROM work_routine_execution_steps s WHERE schedule_id=?),'grants',(SELECT jsonb_agg(to_jsonb(g) ORDER BY package_id) FROM company_package_grants g WHERE company_id=?),'audit',(SELECT jsonb_agg(to_jsonb(a) ORDER BY id) FROM audit_events a WHERE record_id=?),'outbox',(SELECT jsonb_agg(to_jsonb(o) ORDER BY id) FROM outbox_events o WHERE aggregate_id=?))::text image`).bind(taskId,r.createdSchedule.id,r.createdSchedule.id,company,taskId,taskId).first<string>('image'),before=await snapshot();await db.exec(`CREATE SEQUENCE ${fn}_fired;CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fn}_fired');PERFORM pg_sleep(5.2);RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fn} AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.aggregate_id='${taskId}' AND NEW.event_type='task.generated') EXECUTE FUNCTION ${fn}()`);
 try{expect(await generateOccurrences(db,r.at,1)).toEqual({generated:0,examined:1});expect(await snapshot()).toBe(before);expect(await db.prepare(`SELECT is_called FROM ${fn}_fired`).first('is_called')).toBe(true);expect(await db.prepare('SELECT last_error FROM routine_schedules WHERE id=?').bind(r.createdSchedule.id).first<string>('last_error')).toContain('Package access changed');}finally{await db.exec(`DROP TRIGGER ${fn} ON outbox_events;DROP FUNCTION ${fn}();DROP SEQUENCE ${fn}_fired`);}expect(await generateOccurrences(db,r.at,1)).toEqual({generated:0,examined:0});await workRoutinePackage();expect(await generateOccurrences(db,r.at,1)).toEqual({generated:1,examined:1});expect(await db.prepare('SELECT count(*) n FROM staff_tasks WHERE id=?').bind(taskId).first('n')).toBe(1);expect(await db.prepare('SELECT count(*) n FROM work_routine_execution_steps WHERE schedule_id=?').bind(r.createdSchedule.id).first('n')).toBe(1);
});
it('purchasedSocial classifies native linked conversation content by actual source while preserving the original linked-record Task refusal',async()=>{
 const leaveId=await leave(),{createPost,createPostSchema}=await import('@erpx/server-collaboration/public/social'),{sourceSnapshot}=await import('@erpx/server-collaboration/public/social-source'),{taskSourceSchema}=await import('@erpx/server-collaboration/public/social-task'),posted=await createPost('h001',createPostSchema.parse({body:'An actual authorized workplace conversation linked to my Leave',audience:'holding',linkedRecord:{module:'people_payroll',id:leaveId}}),key(),requester),postId=String(posted.id),team=await createPost('h001',createPostSchema.parse({body:'Actual standalone native Task conversation',audience:'holding'}),key(),requester),teamId=String(team.id),snapshot=await sourceSnapshot(db,requester,teamId,[]),nativeSource=taskSourceSchema.parse({postId:teamId,sourceVersion:1,sourceHash:snapshot.hash}),linkedSource=taskSourceSchema.parse({postId,sourceVersion:1,sourceHash:'0'.repeat(64)}),input=taskSchema.parse({title:'Actual native conversation Task',assigneeUserId:worker.userId});
 expect(await db.prepare("SELECT operation,record_id FROM command_receipts WHERE record_id=? AND operation='social.post.publish'").bind(postId).first()).toEqual({operation:'social.post.publish',record_id:postId});await expect(sourceSnapshot(db,requester,postId,[])).rejects.toMatchObject({code:'SOURCE_UNSUPPORTED'});await workPackageState();vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');for(const source of [nativeSource,linkedSource])await expect(saveTask(db,requester,null,input,key(),false,source)).rejects.toMatchObject({code:'PACKAGE_REQUIRED'});
 const {workTaskSocialPurchaseSource,workTaskSocialFileSourceRows}=await import('@erpx/server-collaboration/public/work-task-purchase-source');expect((await workTaskSocialPurchaseSource(db,company,'not-created-task',postId)).workSocial).toBe(true);
 const {packageGrantStore}=await import('@erpx/server-commercial/public/package-grants');await packageGrantStore(db,{authorize:async()=>({userId:'isolated-native-social-provider'}),obligations:async()=>{throw Error('No synthetic social closure');}}).apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:company,packageId:'work-social',packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Actual native Social Task source',evidence:'test:native-people-linked-social'}});await expect(saveTask(db,requester,null,input,key(),false,linkedSource)).rejects.toMatchObject({code:'SOURCE_UNSUPPORTED'});expect(await db.prepare('SELECT id FROM staff_tasks WHERE id=?').bind(`task-post-${postId}`).first()).toBeNull();const created=await saveTask(db,requester,null,input,key(),false,nativeSource),id=String(created.id);expect((await getTask(db,requester,id)).id).toBe(id);expect(await db.prepare(`SELECT work_social FROM (${workTaskSocialFileSourceRows}) native_source WHERE id=? AND company_id=?`).bind(id,company).first('work_social')).toBe(true);expect(JSON.stringify(await taskDetail(db,requester,id))).not.toContain('postsImage');
 const before=await rawWorkObservation('work-social');await db.prepare("INSERT INTO social_posts(id,kind,author_id,author_name,company_id,body,linked_module,linked_record_id,linked_record_label,state,version,last_command_id,created_at,updated_at,audience_kind) SELECT ?,'record',author_id,author_name,company_id,'Unreceipted labelled source must remain pending','people_payroll',linked_record_id,linked_record_label,'published',1,?,created_at,updated_at,audience_kind FROM social_posts WHERE id=?").bind(`unreceipted-social-${key()}`,key(),postId).run();const unknown=await rawWorkObservation('work-social');expect(unknown.collaboration.outstanding).toBe(before.collaboration.outstanding+1);expect(unknown.collaboration.revision).not.toBe(before.collaboration.revision);expect(JSON.stringify(unknown)).not.toContain('Unreceipted labelled source must remain pending');
});

it('purchasedRoutineProject filters25 actual captured queues after native Project membership removal before20 candidates without starving current Work',async()=>{
 await workPackageState();await workRoutinePackage();const {packageGrantStore}=await import('@erpx/server-commercial/public/package-grants');await packageGrantStore(db,{authorize:async()=>({userId:'isolated-project-queue-provider'}),obligations:async()=>{throw Error('No synthetic Project closure');}}).apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:company,packageId:'projects',packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Actual captured Project routine source',evidence:'test:queued-project-prelimit'}});
 vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');const project=await saveProject(db,owner,null,projectSchema.parse({title:'Actual independently managed Project',memberUserIds:[requester.userId,worker.userId]}),key()),template=await saveTemplate(db,requester,null,templateSchema.parse({title:'Current Project member captured routine',projectId:String(project.id),assigneeUserId:worker.userId,completionApprovers:[head.userId]}),key()),ids:string[]=[];
 for(let index=0;index<25;index++){const schedule=await saveSchedule(db,requester,String(template.id),null,scheduleSchema.parse({frequency:'daily',startDate:'2027-01-01'}),key());await changeScheduleState(db,requester,String(schedule.id),1,true,key());ids.push(String(schedule.id));}
 expect(await db.prepare('SELECT count(*) n FROM work_routine_execution_authorizations WHERE template_id=? AND kind=\'resume\'').bind(template.id).first('n')).toBe(25);await saveProject(db,owner,String(project.id),projectSchema.parse({title:'Actual independently managed Project',memberUserIds:[worker.userId],expectedVersion:1}),key());expect(await db.prepare('SELECT user_id FROM staff_project_members WHERE project_id=? AND user_id=?').bind(project.id,requester.userId).first()).toBeNull();
 const source=await saveTemplate(db,requester,null,templateSchema.parse({title:'Valid later ordinary Work queue',assigneeUserId:worker.userId,completionApprovers:[head.userId]}),key()),valid=await saveSchedule(db,requester,String(source.id),null,scheduleSchema.parse({frequency:'daily',startDate:'2028-01-01'}),key());await changeScheduleState(db,requester,String(valid.id),1,true,key());const at=await db.prepare('SELECT next_occurrence_at FROM routine_schedules WHERE id=?').bind(valid.id).first<number>('next_occurrence_at');expect(await generateOccurrences(db,at!,20)).toEqual({generated:1,examined:1});expect(await db.prepare('SELECT count(*) n FROM staff_tasks WHERE template_id=?').bind(template.id).first('n')).toBe(0);expect(await db.prepare('SELECT count(*) n FROM work_routine_execution_steps WHERE schedule_id IN(SELECT id FROM routine_schedules WHERE template_id=?)').bind(template.id).first('n')).toBe(0);expect(await db.prepare('SELECT count(*) n FROM routine_schedules WHERE template_id=? AND last_error IS NOT NULL').bind(template.id).first('n')).toBe(0);expect(await db.prepare('SELECT id FROM staff_tasks WHERE schedule_id=?').bind(valid.id).first('id')).toBe(`task-${valid.id}-${at}`);
 // Actual owner membership restoration makes the same retained captured source
 // eligible again; no queued role, package, or private authorization is rewritten.
 await saveProject(db,owner,String(project.id),projectSchema.parse({title:'Actual independently managed Project',memberUserIds:[requester.userId,worker.userId],expectedVersion:2}),key());const earliest=await db.prepare('SELECT min(next_occurrence_at) at FROM routine_schedules WHERE template_id=?').bind(template.id).first<number>('at');expect(await generateOccurrences(db,earliest!,1)).toEqual({generated:1,examined:1});expect(await db.prepare('SELECT count(*) n FROM staff_tasks WHERE template_id=?').bind(template.id).first('n')).toBe(1);expect(await db.prepare('SELECT count(*) n FROM work_routine_execution_authorizations WHERE template_id=? AND kind=\'resume\'').bind(template.id).first('n')).toBe(25);expect(await db.prepare('SELECT count(*) n FROM company_package_grants WHERE company_id=? AND package_id=\'people\'').bind(company).first('n')).toBe(0);
 const {endSchedule}=await import('@erpx/server-operations/public/routines');for(const id of [...ids,String(valid.id)])await endSchedule(db,requester,id,2,key());
});

it('purchasedRoutineProject repeats original Project membership after deferred native generation and rolls all effects back before the same cursor retry without Work or People grants',async()=>{
 const {packageGrantStore}=await import('@erpx/server-commercial/public/package-grants');await packageGrantStore(db,{authorize:async()=>({userId:'isolated-project-final-member-provider'}),obligations:async()=>{throw Error('No synthetic Project closure');}}).apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:company,packageId:'projects',packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Actual independent Project routine source',evidence:'test:project-routine-final-member'}});vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PROJECTS_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');const project=await saveProject(db,owner,null,projectSchema.parse({title:'Independent Project source retained at finality',memberUserIds:[requester.userId,worker.userId]}),key()),template=await saveTemplate(db,requester,null,templateSchema.parse({title:'Project-only captured independent source',projectId:String(project.id),assigneeUserId:worker.userId,completionApprovers:[head.userId]}),key()),schedule=await saveSchedule(db,requester,String(template.id),null,scheduleSchema.parse({frequency:'daily',startDate:'2025-01-01'}),key());await changeScheduleState(db,requester,String(schedule.id),1,true,key());const at=await db.prepare('SELECT next_occurrence_at FROM routine_schedules WHERE id=?').bind(schedule.id).first<number>('next_occurrence_at'),taskId=`task-${schedule.id}-${at}`,fn=`routine_project_member_${key().replaceAll('-','')}`,tables=['staff_tasks','routine_schedules','work_routine_execution_steps','staff_project_members','command_receipts','audit_events','outbox_events','staff_notifications','approval_instances','approval_steps'],snapshot=()=>Promise.all(tables.map(async table=>(await db.prepare(`SELECT ${table==='routine_schedules'?"(to_jsonb(r)-ARRAY['last_error','updated_at'])::text":"to_jsonb(r)::text"} image FROM ${table} r ORDER BY image`).all()).results)),before=await snapshot();
 await db.exec(`CREATE SEQUENCE ${fn}_fired;CREATE FUNCTION ${fn}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${fn}_fired');DELETE FROM staff_project_members WHERE project_id='${project.id}' AND user_id='${requester.userId}';RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${fn} AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.aggregate_id='${taskId}' AND NEW.event_type='task.generated') EXECUTE FUNCTION ${fn}()`);
 try{expect(await generateOccurrences(db,at!,1)).toEqual({generated:0,examined:1});expect(await snapshot()).toEqual(before);expect(await db.prepare(`SELECT is_called FROM ${fn}_fired`).first('is_called')).toBe(true);expect(await db.prepare('SELECT last_error FROM routine_schedules WHERE id=?').bind(schedule.id).first<string>('last_error')).toContain('Project not found');}finally{await db.exec(`DROP TRIGGER ${fn} ON outbox_events;DROP FUNCTION ${fn}();DROP SEQUENCE ${fn}_fired`);}
 expect(await generateOccurrences(db,at!,1)).toEqual({generated:1,examined:1});expect(await db.prepare('SELECT count(*) n FROM staff_tasks WHERE id=?').bind(taskId).first('n')).toBe(1);expect(await db.prepare('SELECT count(*) n FROM work_routine_execution_steps WHERE schedule_id=?').bind(schedule.id).first('n')).toBe(1);expect((await db.prepare('SELECT package_id FROM company_package_grants WHERE company_id=? ORDER BY package_id').bind(company).all()).results).toEqual([{package_id:'projects'}]);const {endSchedule}=await import('@erpx/server-operations/public/routines');await endSchedule(db,requester,String(schedule.id),2,key());
});


it('rawWorkRoutine attributes only actual routine-source handover acknowledgement and waiting notices while ordinary Social keeps its own addon',async()=>{
 await workPackageState();await workRoutinePackage();const {packageGrantStore}=await import('@erpx/server-commercial/public/package-grants');await packageGrantStore(db,{authorize:async()=>({userId:'isolated-routine-social-provider'}),obligations:async()=>{throw Error('No synthetic Social closure');}}).apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:company,packageId:'work-social',packageVersion:1,state:'active',startsAt:0,endsAt:null,version:1,reason:'Actual independent Social control conversation',evidence:'test:routine-handover-raw'}});vi.stubEnv('ERPX_APPS_PACKAGE_ENFORCEMENT','enforced');
 const publisher:WorkContext={...requester,branchId:null,departmentId:null,scope:{kind:'holding',holdingId:'h001',holdingSlug:'test',holdingName:'Test',companyId:company},inherited:false},receiver:WorkContext={...worker,branchId:null,departmentId:null,scope:{kind:'holding',holdingId:'h001',holdingSlug:'test',holdingName:'Test',companyId:company},inherited:false};
 const {savePostDraft,publishPost}=await import('@erpx/server-collaboration/public/social-home'),{publishHandover,acknowledgeHandover}=await import('@erpx/server-operations/public/handover'),{changeInboxState}=await import('@erpx/server-collaboration/public/inbox'),date=new Date(Date.now()+7*3600000).toISOString().slice(0,10),ordinary=await task([],[]),controlDraft=await savePostDraft(db,publisher,null,{body:'Ordinary Social handover is not routine work',audience:{kind:'participants',userIds:[worker.userId]},purpose:'handover',sources:[{kind:'staff_task',id:ordinary}],expectedVersion:1},key()),control=await publishPost(db,publisher,controlDraft.id,controlDraft.version,key());
 expect((await rawWorkObservation('work-routines')).collaboration.outstanding).toBe(0);expect((await rawWorkObservation('work-social')).collaboration.outstanding).toBeGreaterThan(0);
 const r=await purchasedRoutine(),run=await runTemplate(db,requester,String(r.created.id),Date.now()+86400000,key()),id=String(run.id),row=await getTask(db,requester,id),snapshot:import("@erpx/server-work_projection/public/work-items").WorkSnapshot={unavailableSources:[],items:[{id,type:'staff_task',title:row.title,detail:row.description,href:`/staff-tasks/${id}`,status:row.state,updatedAt:row.updated_at,queue:'todo'}]},before=(await rawWorkObservation('work-routines')).collaboration.outstanding,socialBefore=(await rawWorkObservation('work-social')).collaboration.outstanding,post=await publishHandover(db,publisher,snapshot,{date,body:'Actual original routine handover remains due until its recipient acknowledges',sources:[{kind:'staff_task',id,sourceVersion:row.version}],recipientIds:[worker.userId]},key());const {publishInboxEvent}=await import('@erpx/server-collaboration/public/inbox');await publishInboxEvent(db,{key:key(),companyId:company,sourceType:'post',sourceId:post.id,sourceVersion:1,category:'work',preview:'Actual retained routine handover delivery',recipientIds:[receiver.userId]});const waiting=await rawWorkObservation('work-routines');
 expect(waiting.collaboration.outstanding).toBe(before+2);expect((await rawWorkObservation('work-social')).collaboration.outstanding).toBe(socialBefore);const acknowledgement=key();expect((await acknowledgeHandover(db,receiver,post.id,acknowledgement)).acknowledged).toBe(true);expect((await rawWorkObservation('work-routines')).collaboration.outstanding).toBe(before+1);const notices=(await db.prepare("SELECT delivery.id FROM inbox_deliveries delivery JOIN inbox_events event ON event.id=delivery.event_id AND event.company_id=delivery.company_id WHERE event.company_id=? AND event.source_type='post' AND event.source_id=? AND delivery.user_id=? AND delivery.read_at IS NULL").bind(company,post.id,worker.userId).all<{id:string}>()).results;expect(notices).toHaveLength(1);await changeInboxState(db,receiver,notices[0].id,{read:true});expect((await rawWorkObservation('work-routines')).collaboration.outstanding).toBe(before);expect((await acknowledgeHandover(db,receiver,post.id,acknowledgement)).acknowledged).toBe(true);expect(control.id).not.toBe(post.id);
});
