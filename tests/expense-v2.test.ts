import { env } from "./runtime-env";
import { applyMigrations, type TestMigration } from "./runtime-env";
import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "./helpers/expense-fixture";
import {
  saveExpense,
  getExpense,
  transitionExpense,
  postExpense,
  expenseUpdateSchema,
} from "../src/server/expenses";
import {
  expenseRoute,
  expenseAuthoritySnapshot,
} from "../src/server/expense-routing";
import { saveExpenseAccounting } from "../src/server/expense-workflow";
import {
  approvalView,
  prepareDecision,
  rerouteApproval,
} from "../src/server/approvals";
import { executeCommand } from "../src/server/commands";
import {
  createExpensePayment,
  confirmExpensePayment,
} from "../src/server/expense-payments";
import { settleInvoice } from "../src/server/commerce";
vi.mock("../src/server/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
let f: Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS as TestMigration[]);
});
beforeEach(async () => {
  f = await expenseFixture();
});
it("preserves incomplete owner and beta member drafts and reports missing setup", async () => {
  for (const role of ["employee", "holding_owner"]) {
    const actor = await f.member(
      `missing-${role}`,
      role,
      1,
      f.department,
      false,
    );
    actor.beta = true;
    const id = String(
      (await saveExpense(actor, null, { expectedVersion: 1 }, key())).id,
    );
    const row = await getExpense(actor, id);
    expect(row.claimant_user_id).toBe(actor.userId);
    expect(row.amount_minor).toBeNull();
    expect(row.setup_requirements_json).toContain("active employee");
    await expect(
      transitionExpense(actor, id, "submit", { expectedVersion: 1 }, key()),
    ).rejects.toMatchObject({ code: "EXPENSE_SETUP_REQUIRED" });
    expect((await getExpense(actor, id)).status).toBe("draft");
  }
});
it("only the configured primary can decide and concurrent decisions complete one step", async () => {
  const id = await f.claim();
  await f.attach(id);
  await f.action(f.claimant, id, "submit");
  const row = await getExpense(f.head, id);
  expect(JSON.parse(String(row.eligible_approvers_json))).toEqual([f.head.userId]);
  const results = await Promise.allSettled(
    [f.head, f.tie].map((actor) =>
      transitionExpense(
        actor,
        id,
        "approve",
        { expectedVersion: Number(row.version), reason: "Evidence checked" },
        key(),
      ),
    ),
  );
  expect(results.filter((r) => r.status === "fulfilled")).toHaveLength(1);
  expect((await getExpense(f.claimant, id)).workflow_stage).toBe(
    "accounts_review",
  );
  expect(
    (await approvalView(db, "expense", id, f.head)).history.filter(
      (d) => d.action === "approve",
    ),
  ).toHaveLength(1);
});
it("uses the reporting superior and blocks missing supervisors and cycles", async () => {
  await db
    .prepare(
      "UPDATE hr_engagements SET level_id=?,manager_person_id=? WHERE person_id=?",
    )
    .bind(
      `org-level-${f.company}-3`,
      `person-${f.owner.userId}`,
      `person-${f.claimant.userId}`,
    )
    .run();
  expect(
    (await expenseRoute(db, f.company, f.claimant.userId)).steps[0].userId,
  ).toBe(f.owner.userId);
  await db
    .prepare(
      "UPDATE hr_engagements SET manager_person_id=NULL WHERE person_id=?",
    )
    .bind(`person-${f.claimant.userId}`)
    .run();
  expect(
    (await expenseRoute(db, f.company, f.claimant.userId)).blockedReason,
  ).toContain("reporting manager");
  await db
    .prepare(
      "UPDATE hr_engagements SET manager_person_id=person_id WHERE person_id=?",
    )
    .bind(`person-${f.claimant.userId}`)
    .run();
  expect(
    (await expenseRoute(db, f.company, f.claimant.userId)).blockedReason,
  ).toContain("circular");
});
it("revalidates department ranks and revoked Accounts roles at commit", async () => {
  const id = await f.claim();
  await f.attach(id);
  await f.action(f.claimant, id, "submit");
  const row = await getExpense(f.head, id),
    command = key(),
    decision = await prepareDecision(
      db,
      f.head,
      command,
      "expense",
      id,
      true,
      "Checked",
      Number(row.version),
    );
  await db
    .prepare("UPDATE staff_levels SET rank=20 WHERE id=?")
    .bind(`${f.company}-level-1`)
    .run();
  await expect(
    executeCommand(
      db,
      f.head,
      command,
      "test.decision",
      {},
      id,
      3,
      {},
      decision.statements,
    ),
  ).rejects.toMatchObject({ status: 409 });
  expect((await getExpense(f.claimant, id)).workflow_stage).toBe(
    "department_approval",
  );
  const snapshot = await expenseAuthoritySnapshot(db, f.company);
  await db
    .prepare(
      "UPDATE assignments SET revoked_at=1 WHERE user_id=? AND role='finance_preparer'",
    )
    .bind(f.reviewer.userId)
    .run();
  const guardKey = key();
  await expect(
    executeCommand(db, f.owner, guardKey, "test.snapshot", {}, id, 1, {}, [
      snapshot.guard(guardKey),
    ]),
  ).rejects.toMatchObject({ status: 409 });
});
it("separates approval from atomic GL posting, preserves partial payments and requires unique slips", async () => {
  const id = await f.claim();
  await f.throughAccounts(id);
  const evidence = await db
    .prepare(
      "SELECT d.authority_json FROM approval_decisions d JOIN approval_steps s ON s.id=d.step_id JOIN approval_instances i ON i.id=d.instance_id WHERE i.record_id=? AND s.stage='accounts_review' AND d.action='approve'",
    )
    .bind(id)
    .first<{ authority_json: string }>();
  const authority = JSON.parse(evidence!.authority_json);
  expect(authority.actorId).toBe(f.reviewer.userId);
  expect(authority.requiredRole).toBe("finance_preparer");
  expect(
    authority.assignments.some(
      (a: { role: string }) => a.role === "finance_preparer",
    ),
  ).toBe(true);
  expect(authority.receipts).toHaveLength(1);
  await db
    .prepare("UPDATE finance_periods SET state='closed' WHERE id=?")
    .bind(f.period)
    .run();
  await f.action(f.manager, id, "approve");
  let approved = await getExpense(f.manager,id);
  expect(approved.workflow_stage).toBe("gl_posting");
  expect(approved.journal_id).toBeNull();
  await expect(postExpense(f.manager,id,{expectedVersion:Number(approved.version),periodId:f.period,entryDate:"2026-09-02"},key())).rejects.toMatchObject({code:"PERIOD_CLOSED"});
  expect((await getExpense(f.manager,id)).workflow_stage).toBe("gl_posting");
  await db.prepare("UPDATE finance_periods SET state='open' WHERE id=?").bind(f.period).run();
  approved = await getExpense(f.manager,id);
  await postExpense(f.manager,id,{expectedVersion:Number(approved.version),periodId:f.period,entryDate:"2026-09-02"},key());
  let row = await getExpense(f.payer, id);
  expect(row.status).toBe("posted");
  expect(row.can_pay).toBe(1);
  const payment = await createExpensePayment(
      f.payer,
      id,
      Number(row.version),
      key(),
    ),
    draftId = String(payment.draftId),
    input = {
      expectedVersion: 1,
      date: "2026-09-03",
      amount: "40",
      reference: key(),
    };
  await expect(
    confirmExpensePayment(f.payer, draftId, input, key()),
  ).rejects.toMatchObject({ code: "PAYMENT_SLIP_REQUIRED" });
  const checksum = key();
  await f.attach(draftId, "expense_payment", checksum);
  const command = key(),
    paid = await confirmExpensePayment(f.payer, draftId, input, command);
  expect(paid.outstandingMinor).toBe(6700);
  expect(await confirmExpensePayment(f.payer, draftId, input, command)).toEqual(
    paid,
  );
  row = await getExpense(f.payer, id);
  expect(row.status).toBe("posted");
  const second = String(
    (await createExpensePayment(f.payer, id, Number(row.version), key()))
      .draftId,
  );
  await f.attach(second, "expense_payment", checksum);
  await expect(
    confirmExpensePayment(
      f.payer,
      second,
      { ...input, amount: "67", reference: key() },
      key(),
    ),
  ).rejects.toMatchObject({ status: 409 });
  await db
    .prepare("UPDATE attachments SET revoked_at=1 WHERE owner_id=?")
    .bind(second)
    .run();
  await f.attach(second, "expense_payment");
  await confirmExpensePayment(
    f.payer,
    second,
    { ...input, amount: "67", reference: key() },
    key(),
  );
  row = await getExpense(f.claimant, id);
  expect(row.status).toBe("reimbursed");
  expect(row.workflow_stage).toBe("completed");
  expect(row.outstanding_minor).toBe(0);
});
it("requires four different people and routes correction through department approval again", async () => {
  const id = await f.claim();
  await f.throughAccounts(id);
  await f.action(f.manager, id, "reject");
  let row = await getExpense(f.claimant, id);
  expect(row.workflow_stage).toBe("draft");
  await saveExpense(
    f.claimant,
    id,
    expenseUpdateSchema.parse({
      expectedVersion: row.version,
      expenseDate: "2026-09-01",
      merchant: "Corrected supplier",
      receiptReference: key(),
      businessPurpose: "Corrected travel description",
      currency: "THB",
      amount: "100",
      tax: "7",
      categoryAccountId: f.accounts.expense,
    }),
    key(),
  );
  await f.action(f.claimant, id, "submit");
  row = await getExpense(f.claimant, id);
  expect(row.workflow_stage).toBe("department_approval");
  expect((await approvalView(db, "expense", id, f.head)).instance?.cycle).toBe(
    2,
  );
  await f.action(f.head, id, "approve");
  await f.grant(f.head, "finance_preparer");
  await expect(
    saveExpenseAccounting(
      f.head,
      id,
      {
        expectedVersion: Number((await getExpense(f.head, id)).version),
        categoryAccountId: f.accounts.expense,
        tax: "0",
        periodId: f.period,
        entryDate: "2026-09-02",
        note: "",
      },
      key(),
    ),
  ).rejects.toMatchObject({ status: 403 });
});
it("denies alternate posting, settlement, other company access and ineligible repair", async () => {
  const id = await f.claim();
  await f.attach(id);
  await f.action(f.claimant, id, "submit");
  const view = await approvalView(db, "expense", id, f.owner),
    row = await getExpense(f.owner, id);
  await expect(
    rerouteApproval(
      db,
      f.owner,
      view.instance!.id,
      {
        expectedVersion: Number(row.version),
        instanceVersion: view.instance!.version,
        approverUserId: f.claimant.userId,
        reason: "Repair",
      },
      key(),
    ),
  ).rejects.toBeDefined();
  await expect(
    getExpense({ ...f.owner, companyId: "unrelated" }, id),
  ).rejects.toMatchObject({ status: 404 });
  await expect(
    postExpense(
      f.owner,
      id,
      {
        expectedVersion: Number(row.version),
        entryDate: "2026-09-02",
        periodId: f.period,
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "GL_POSTER_REQUIRED" });
  await f.action(f.head, id, "approve");
  let current = await getExpense(f.reviewer, id);
  await saveExpenseAccounting(
    f.reviewer,
    id,
    {
      expectedVersion: Number(current.version),
      categoryAccountId: f.accounts.expense,
      tax: "7",
      periodId: f.period,
      entryDate: "2026-09-02",
      note: "",
    },
    key(),
  );
  await f.action(f.reviewer, id, "approve");
  await f.action(f.manager, id, "approve");
  const approved = await getExpense(f.manager,id);
  await postExpense(f.manager,id,{expectedVersion:Number(approved.version),periodId:f.period,entryDate:"2026-09-02"},key());
  current = await getExpense(f.owner, id);
  await expect(
    settleInvoice(
      f.owner,
      String(current.invoice_id),
      {
        expectedVersion: 1,
        date: "2026-09-03",
        amount: "107",
        reference: key(),
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "EXPENSE_PAYMENT_REQUIRED" });
});

it("removes unacted pool access after role revocation and reroutes to a currently eligible replacement", async () => {
  const id = await f.claim();
  await f.attach(id);
  await f.action(f.claimant, id, "submit");
  await f.action(f.head, id, "approve");
  expect((await getExpense(f.reviewer, id)).can_decide).toBe(1);
  await db
    .prepare(
      "UPDATE assignments SET revoked_at=1 WHERE user_id=? AND role='finance_preparer'",
    )
    .bind(f.reviewer.userId)
    .run();
  await expect(getExpense(f.reviewer, id)).rejects.toMatchObject({
    status: 404,
  });
  const replacement = await f.member(
    "replacement",
    "employee",
    1,
    f.accountsDepartment,
  );
  replacement.assignmentId = await f.grant(replacement, "finance_preparer");
  replacement.role = "finance_preparer";
  await db.prepare("UPDATE hr_engagements SET level_id=(SELECT level_id FROM hr_engagements WHERE person_id=?),position_id=(SELECT position_id FROM hr_engagements WHERE person_id=?),manager_person_id=? WHERE person_id=?").bind(`person-${f.reviewer.userId}`,`person-${f.reviewer.userId}`,`person-${f.owner.userId}`,`person-${replacement.userId}`).run();
  const view = await approvalView(db, "expense", id, f.owner),
    row = await getExpense(f.owner, id);
  await rerouteApproval(
    db,
    f.owner,
    view.instance!.id,
    {
      expectedVersion: Number(row.version),
      instanceVersion: view.instance!.version,
      approverUserId: replacement.userId,
      reason: "Original Accounts reviewer left the team.",
    },
    key(),
  );
  expect((await getExpense(replacement, id)).can_decide).toBe(1);
  expect(
    (await approvalView(db, "expense", id, f.owner)).history.at(-1)?.reason,
  ).toContain("left the team");
});

it("does not offer a department decision that would leave no independent Accounts reviewer", async () => {
  await db
    .prepare(
      "UPDATE assignments SET revoked_at=1 WHERE user_id=? AND role='finance_preparer'",
    )
    .bind(f.reviewer.userId)
    .run();
  await f.grant(f.head, "finance_preparer");
  const route = await expenseRoute(db, f.company, f.claimant.userId);
  expect(route.blockedReason).toContain("independent Accounting");
  expect(route.steps).toEqual([]);
});

it("excludes reviewers whose current assignments do not cover the claim branch", async () => {
  const otherBranch = `${f.company}-other`;
  await db
    .prepare(
      "INSERT INTO organization_nodes(id,company_id,kind,slug,display_name,parent_id,branch_id) VALUES(?,?,'branch',?,'Other branch',?,?)",
    )
    .bind(otherBranch, f.company, otherBranch, f.company, otherBranch)
    .run();
  await db
    .prepare(
      "UPDATE assignments SET scope_kind='branch',scope_node_id=?,branch_id=? WHERE id=?",
    )
    .bind(otherBranch, otherBranch, f.head.assignmentId)
    .run();
  expect(
    (await expenseRoute(db, f.company, f.claimant.userId)).steps[0]
      .candidateUserIds,
  ).toEqual([f.owner.userId]);
  await db
    .prepare("UPDATE organization_nodes SET status='suspended' WHERE id=?")
    .bind(f.branch)
    .run();
  expect(
    (await expenseRoute(db, f.company, f.claimant.userId)).blockedReason,
  ).toContain("branch");
});
