import { env } from "./runtime-env";
import { applyMigrations, type TestMigration } from "./runtime-env";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import {
  activeStaff,
  requestRoute,
  staffPeople,
} from "../src/server/staff-policy";
import {
  assignLevel,
  saveLevel,
  setDepartmentHead,
} from "../src/server/staff-settings";
import {
  approvalView,
  prepareDecision,
  rerouteApproval,
  openApproval,
} from "../src/server/approvals";
import {
  projectSchema,
  saveProject,
  saveTask,
  taskSchema,
  taskDetail,
  transitionTask,
  getTask,
} from "../src/server/staff-tasks";
import {
  generateOccurrences,
  saveSchedule,
  saveTemplate,
  scheduleSchema,
  templateSchema,
  changeScheduleState,
  runTemplate,
} from "../src/server/routines";
import {
  createLeave,
  submitLeave,
  approveLeave,
  transitionLeave,
  getLeave,
  leaveDraftSchema,
} from "../src/server/leave";
import {
  saveExpense,
  expenseUpdateSchema,
  getExpense,
} from "../src/server/expenses";
import { executeCommand } from "../src/server/commands";
import { administrationAccess } from "../src/server/administration";
import type { WorkContext } from "../src/server/authorization";
vi.mock("../src/server/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
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
    { action: act, expectedVersion: row.version, note },
    key(),
  );
}

beforeAll(async () => {
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
        { expectedVersion: 2, note: "Too early" },
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
      { expectedVersion: 2, note: "Department approved" },
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
        { expectedVersion: 3, note: "Head again" },
        key(),
        head,
      ),
    ).rejects.toMatchObject({ status: 403 });
    await approveLeave(
      "h001",
      id,
      { expectedVersion: 3, note: "HR approved" },
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
    await expect(submit(id)).rejects.toMatchObject({
      code: "APPROVAL_ROUTE_BLOCKED",
    });
    expect((await getLeave("h001", id, requester)).state).toBe("draft");
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
      { action: "reject", expectedVersion: 2, note: "Correct handover" },
      key(),
    );
    await submit(id, 3);
    let detail = await getLeave("h001", id, requester);
    expect(detail.approval.instance?.cycle).toBe(2);
    expect(detail.approval.history).toHaveLength(1);
    await approveLeave(
      "h001",
      id,
      { expectedVersion: 4, note: "Department okay" },
      key(),
      head,
    );
    await transitionLeave(
      requester,
      id,
      { action: "cancel", expectedVersion: 5, note: "Dates changed" },
      key(),
    );
    detail = await getLeave("h001", id, requester);
    expect(detail.state).toBe("cancelled");
    await expect(
      approveLeave(
        "h001",
        id,
        { expectedVersion: 6, note: "Too late" },
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
    const result = await approveLeave("h001", id, input, command, head);
    expect(await approveLeave("h001", id, input, command, head)).toEqual(
      result,
    );
    const outcomes = await Promise.allSettled([
      approveLeave(
        "h001",
        id,
        { expectedVersion: 3, note: "Approved" },
        key(),
        hr,
      ),
      approveLeave(
        "h001",
        id,
        { expectedVersion: 3, note: "Approved" },
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
      { expectedVersion: 2, note: "Okay" },
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
      { expectedVersion: 2, note: "Department review" },
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
      { expectedVersion: 3, note: "Replacement approval" },
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
});
describe("Reusable levels and routine scheduling", () => {
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
  const { workSnapshot } = await import('../src/server/work-items');
  const { closeRedisCache } = await import('../src/server/runtime/redis-cache');
  const previous = process.env.NODE_ENV, redisUrl = process.env.ERPX_REDIS_URL;
  process.env.NODE_ENV = 'development';
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
  } finally { if (redisUrl === undefined) delete process.env.ERPX_REDIS_URL; else process.env.ERPX_REDIS_URL = redisUrl; await closeRedisCache(); if (previous === undefined) delete process.env.NODE_ENV; else process.env.NODE_ENV = previous; }
});
