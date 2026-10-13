import { env } from "../../support/runtime-env";
import { applyMigrations, type TestMigration } from "../../support/runtime-env";
import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "../../support/helpers/expense-fixture";
import {
  saveExpense,
  canEditExpense,
  canReviewExpense,
  canPostExpenses,
  getExpense,
  transitionExpense,
  postExpense,
  expenseDetail,
  revokeExpenseAttachment,
  expenseUpdateSchema,
} from "../../../src/server/modules/people_payroll/public/expenses";
import {
  expenseRoute,
  expenseAuthoritySnapshot,
} from "../../../src/server/modules/people_payroll/public/expense-routing";
import { saveExpenseAccounting } from "../../../src/server/modules/people_payroll/public/expense-workflow";
import {
  approvalView,
  prepareDecision,
  rerouteApproval,
} from "../../../src/server/modules/workflow/public/approvals";
import { executeCommand } from "../../../src/server/modules/workflow/public/commands";
import {
  createExpensePayment,
  confirmExpensePayment,
} from "../../../src/server/modules/people_payroll/public/expense-payments";
import { settleInvoice } from "../../../src/server/modules/commerce/public/commerce";
import { bindRecordAttachmentSet } from "../../../src/server/modules/files/public/attachments/service";
import {nativeExpenseReads} from "../../../src/server/modules/people_payroll/adapters/expenses";
import {createDomainWork} from "../../../src/server/modules/work_projection/application/domain-work";
import {measureReads} from "../../../src/server/modules/runtime/adapters/runtime/read-metrics";
import {withPolicyReadSnapshot} from "../../../src/server/modules/identity_access/public/iam-read-snapshot";
import { seedAttachmentSet } from "../../support/helpers/attachment-v2";
vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({
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
it("matches the original Expense list across roles, draft requirements and revoked assignments with fewer workflow queries",async()=>{
  const submitted=await f.claim();await f.attach(submitted);await f.action(f.claimant,submitted,"submit");
  await f.claim();await f.claim();
  for(const actor of [f.claimant,f.head,f.reviewer,f.payer,f.owner,f.it]){
    const original=await measureReads(()=>nativeExpenseReads(db,actor,{batch:false}).list());
    const batch=await measureReads(()=>nativeExpenseReads(db,actor).list());
    expect(batch.result).toEqual(original.result);
    if(actor.userId===f.claimant.userId)expect(batch.metrics.queries).toBeLessThan(original.metrics.queries);
  }
  await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(),f.head.assignmentId).run();
  expect(await nativeExpenseReads(db,f.head).list()).toEqual(await nativeExpenseReads(db,f.head,{batch:false}).list());
});
it("keeps draft employment reads bounded per claimant and rechecks employment on the next list", async () => {
  const first = await f.claim();
  const read = () => measureReads(() => withPolicyReadSnapshot(() => nativeExpenseReads(db, f.claimant).list()));
  const single = await read();
  for (let index = 0; index < 11; index++) await f.claim();
  await db.prepare("UPDATE finance_expenses SET expense_date='1900-01-01' WHERE id=?").bind(first).run();
  const many = await read();
  expect(many.result).toHaveLength(12);
  expect(many.result).toEqual(await nativeExpenseReads(db, f.claimant, {batch:false}).list());
  expect(many.result.find(row => row.id === first)?.setup_requirements_json).toContain("engagement dates");
  expect(many.metrics.queries).toBe(single.metrics.queries);

  const otherClaim = await f.claim(f.head);
  const unavailableClaimant = await f.member("batch-missing-staff", "employee", 1, f.department, false);
  const unavailableClaim = await f.claim(unavailableClaimant);
  // Model owner-prepared drafts so the owner may read them after claimant revocation.
  await db.prepare("UPDATE finance_expenses SET created_by=?,prepared_by=? WHERE company_id=?")
    .bind(f.owner.userId, f.owner.userId, f.company).run();
  const mixed = await withPolicyReadSnapshot(() => nativeExpenseReads(db, f.owner).list());
  expect(mixed).toHaveLength(14);
  expect(mixed).toEqual(await nativeExpenseReads(db, f.owner, {batch:false}).list());
  expect(mixed.find(row => row.id === unavailableClaim)?.setup_requirements_json).toContain("active employee");
  expect(mixed.find(row => row.id === otherClaim)?.setup_requirements_json).not.toContain("active employee");

  await db.prepare("UPDATE hr_engagements SET level_id=NULL WHERE company_id=? AND person_id=?")
    .bind(f.company, `person-${f.claimant.userId}`).run();
  const noLevel = await read();
  expect(noLevel.result).toEqual(await nativeExpenseReads(db, f.claimant, {batch:false}).list());
  expect(noLevel.result.every(row => String(row.setup_requirements_json).includes("active staff level"))).toBe(true);
  expect((await getExpense(f.claimant, first)).setup_requirements_json).toContain("active staff level");

  await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(), f.claimant.assignmentId).run();
  const revoked = await withPolicyReadSnapshot(() => nativeExpenseReads(db, f.owner).list());
  expect(revoked).toEqual(await nativeExpenseReads(db, f.owner, {batch:false}).list());
  expect(revoked.filter(row => row.claimant_user_id === f.claimant.userId)).toHaveLength(12);
  expect(revoked.filter(row => row.claimant_user_id === f.claimant.userId)
    .every(row => String(row.setup_requirements_json).includes("active employee"))).toBe(true);
  expect(revoked.find(row => row.id === otherClaim)?.setup_requirements_json).not.toContain("active employee");
});
it("keeps rollback reads and receipt guards on clean bound links only",async()=>{
  const previousGate=process.env.ERPX_ATTACHMENTS_V2,previousReadOnly=process.env.ERPX_ATTACHMENTS_READ_ONLY;
  process.env.ERPX_ATTACHMENTS_V2="true";delete process.env.ERPX_ATTACHMENTS_READ_ONLY;
  try{
    const id=await f.claim();
    const bound=await seedAttachmentSet(db,f.claimant,{targetType:"expense_claim",targetId:id,componentKind:"record",componentRef:"legacy-upload-adapter",purposeKey:"receipt",expectedTargetVersion:1});
    await bindRecordAttachmentSet(db,f.claimant,bound.setId,1,key());
    const unbound=await seedAttachmentSet(db,f.claimant,{targetType:"expense_claim",targetId:id,componentKind:"record",componentRef:"legacy-upload-adapter",purposeKey:"receipt",expectedTargetVersion:2});
    const raw=await f.attach(id,"expense_claim","raw-unlinked");
    process.env.ERPX_ATTACHMENTS_V2="false";process.env.ERPX_ATTACHMENTS_READ_ONLY="true";
    expect((await expenseDetail(f.claimant,id)).attachments.map(row=>row.id)).toEqual([bound.sourceId]);
    await expect(revokeExpenseAttachment(f.claimant,id,unbound.sourceId,2,key())).rejects.toMatchObject({status:503,code:"ATTACHMENTS_READ_ONLY"});
    await expect(revokeExpenseAttachment(f.claimant,id,raw,2,key())).rejects.toMatchObject({status:503,code:"ATTACHMENTS_READ_ONLY"});
    await expect(transitionExpense(f.claimant,id,"submit",{expectedVersion:2},key())).resolves.toMatchObject({status:"in_review"});
  }finally{
    if(previousGate===undefined)delete process.env.ERPX_ATTACHMENTS_V2;else process.env.ERPX_ATTACHMENTS_V2=previousGate;
    if(previousReadOnly===undefined)delete process.env.ERPX_ATTACHMENTS_READ_ONLY;else process.env.ERPX_ATTACHMENTS_READ_ONLY=previousReadOnly;
  }
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
it("keeps the appointed department head while validating the reporting graph", async () => {
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
  ).toBe(f.head.userId);
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
        attachmentSetIds: [],
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

it("keeps appointment authority when a separate work assignment changes scope", async () => {
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
  ).toEqual([f.head.userId]);
  await db
    .prepare("UPDATE organization_nodes SET status='suspended' WHERE id=?")
    .bind(f.branch)
    .run();
  expect(
    (await expenseRoute(db, f.company, f.claimant.userId)).blockedReason,
  ).toContain("branch");
});

it("rolls back the provided Finance reversal and Expense command on a final outbox fault, then retries and replays the same key", async () => {
  const id = await f.claim();
  await f.throughAccounts(id);
  await f.action(f.manager, id, "approve");
  const approved = await getExpense(f.manager, id);
  await postExpense(f.manager, id, { expectedVersion: Number(approved.version), periodId: f.period, entryDate: "2026-09-02" }, key());
  const posted = await getExpense(f.manager, id);
  const command = key(), suffix = key().replaceAll("-", ""), failure = `expense_owner_outbox_${suffix}`;
  const snapshot = async () => (await db.prepare(`SELECT jsonb_build_object(
    'expense',to_jsonb(x),'invoice',to_jsonb(i),'journal',to_jsonb(j),
    'originalLines',(SELECT jsonb_agg(to_jsonb(l) ORDER BY l.ordinal) FROM finance_journal_lines l WHERE l.journal_id=j.id),
    'reversals',(SELECT count(*) FROM finance_journals r WHERE r.company_id=x.company_id AND r.source_type='expense_reversal' AND r.source_id=x.id),
    'receipts',(SELECT count(*) FROM command_receipts WHERE command_id=?),
    'audits',(SELECT count(*) FROM audit_events WHERE command_id=?),
    'outbox',(SELECT count(*) FROM outbox_events WHERE command_id=?))::text snapshot
    FROM finance_expenses x JOIN finance_invoices i ON i.id=x.invoice_id JOIN finance_journals j ON j.id=x.journal_id WHERE x.id=?`)
    .bind(command, command, command, id).first<{ snapshot: string }>())!.snapshot;
  const input = { expectedVersion: Number(posted.version), periodId: f.period, entryDate: "2026-09-03", reason: "Independent correction of the original unclaimed expense" };
  const before = await snapshot();
  await db.prepare(`CREATE FUNCTION ${failure}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'EXPENSE_FINANCE_FINAL_OUTBOX'; END $$`).run();
  await db.prepare(`CREATE TRIGGER ${failure} AFTER INSERT ON outbox_events FOR EACH ROW WHEN (NEW.command_id='${command}') EXECUTE FUNCTION ${failure}()`).run();
  const { reverseExpense } = await import("../../../src/server/modules/people_payroll/public/expenses");
  try {
    await expect(reverseExpense(f.manager, id, input, command)).rejects.toThrow("EXPENSE_FINANCE_FINAL_OUTBOX");
    expect(await snapshot()).toBe(before);
  } finally {
    await db.prepare(`DROP TRIGGER ${failure} ON outbox_events`).run();
    await db.prepare(`DROP FUNCTION ${failure}()`).run();
  }
  const reversed = await reverseExpense(f.manager, id, input, command);
  expect(reversed).toMatchObject({ id, status: "cancelled", version: Number(posted.version) + 1 });
  const after = await snapshot();
  expect(await reverseExpense(f.manager, id, input, command)).toEqual(reversed);
  expect(await snapshot()).toBe(after);
  expect(await db.prepare("SELECT status,outstanding_minor FROM finance_invoices WHERE id=?").bind(posted.invoice_id).first()).toMatchObject({ status: "voided", outstanding_minor: 0 });
  expect(await db.prepare("SELECT status FROM finance_journals WHERE id=?").bind(posted.journal_id).first()).toMatchObject({ status: "reversed" });
  expect(await db.prepare("SELECT count(*) n FROM command_receipts WHERE command_id=?").bind(command).first<number>("n")).toBe(1);
});

it.each(["batch-first", "original-first"] as const)("preserves mixed legacy/current Expense rows and lazy legacy initialization (%s)", async order => {
  // Workflow v1 resolves the legacy named-head table; v2 uses authority seats.
  await db.prepare("INSERT INTO department_heads(department_id,company_id,person_id,updated_by,updated_at) VALUES(?,?,?,?,?)")
    .bind(f.department, f.company, `person-${f.head.userId}`, f.owner.userId, Date.now()).run();
  const legacyDraft = await f.claim();
  const legacyPending = await f.claim();
  const legacyBlocked = await f.claim();
  const legacyApproved = await f.claim();
  const currentDraft = await f.claim();
  const currentReview = await f.claim();
  await f.attach(currentReview);
  await f.action(f.claimant, currentReview, "submit");
  for (const [id, status] of [[legacyDraft, "draft"], [legacyPending, "in_review"], [legacyBlocked, "in_review"], [legacyApproved, "approved"]]) {
    await db.prepare("UPDATE finance_expenses SET workflow_version=1,workflow_stage='draft',status=? WHERE company_id=? AND id=?")
      .bind(status, f.company, id).run();
  }
  await db.prepare("UPDATE finance_expenses SET approved_by=? WHERE id=? AND company_id=?")
    .bind(f.head.userId, legacyApproved, f.company).run();
  const now = Date.now();
  const pendingInstance = `approval-${key()}`, blockedInstance = `approval-${key()}`;
  for (const [instanceId, recordId, state] of [[pendingInstance, legacyPending, "pending"], [blockedInstance, legacyBlocked, "blocked"]]) {
    await db.prepare("INSERT INTO approval_instances(id,company_id,branch_id,department_id,record_type,record_id,phase,cycle,requester_user_id,state,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)")
      .bind(instanceId, f.company, f.branch, f.department, "expense", recordId, "request", 1, f.claimant.userId, state, now, now).run();
  }
  await db.prepare("INSERT INTO approval_steps(id,instance_id,ordinal,approver_user_id,authority) VALUES(?,?,?,?,?)")
    .bind(`${pendingInstance}-0`, pendingInstance, 0, f.head.userId, "department").run();
  const selectedIds = [legacyDraft, legacyPending, legacyBlocked, legacyApproved, currentDraft, currentReview];
  for (const actor of [f.claimant, f.head, f.reviewer, f.payer, f.owner, f.it]) {
    const originalRead = () => nativeExpenseReads(db, actor, {batch:false}).list();
    const batchRead = () => nativeExpenseReads(db, actor).list();
    const first = await (order === "batch-first" ? batchRead() : originalRead());
    const second = await (order === "batch-first" ? originalRead() : batchRead());
    expect(first).toEqual(second);
    expect(first.map(row => row.id)).toEqual(second.map(row => row.id));
    expect(first).toHaveLength(second.length);
    const unused = async () => { throw new Error("UNEXPECTED_OTHER_DOMAIN_READ"); };
    const expenseWork = (batch: boolean) => createDomainWork({
      listExpenses: context => nativeExpenseReads(db, context, {batch}).list(),
      canEditExpense, canReviewExpense, canPostExpenses,
      listTasks: unused, taskWorkState: unused,
      listCounts: unused, listTransfers: unused,
      canReadCounts: () => false, canEditCount: () => false, canReviewCount: () => false,
      canEditTransfer: () => false, canReviewTransfer: () => false,
    }).getDomainWork(actor, "expense");
    const originalWork = await expenseWork(false), batchWork = await expenseWork(true);
    expect(batchWork).toEqual(originalWork);
    const countWork = (rows: typeof originalWork) => ({
      total: rows.length,
      todo: rows.filter(row => row.queue === "todo").length,
      waiting: rows.filter(row => row.queue === "waiting").length,
      completed: rows.filter(row => row.queue === "completed").length,
    });
    expect(countWork(batchWork)).toEqual(countWork(originalWork));
    if (actor.userId === f.claimant.userId) {
      expect(batchWork).toHaveLength(selectedIds.length);
      expect(first).toHaveLength(selectedIds.length);
      expect(new Set(first.map(row => String(row.id)))).toEqual(new Set(selectedIds));
      expect(first.find(row => row.id === legacyDraft)?.workflow_version).toBe(1);
      expect(first.find(row => row.id === currentDraft)?.workflow_version).toBe(2);
    }
  }
  const initialized = await db.prepare("SELECT state,version FROM approval_instances WHERE id=? AND company_id=?")
    .bind(blockedInstance, f.company).first<{state:string;version:number}>();
  expect(initialized?.state).toBe("pending");
  expect(Number(initialized?.version)).toBeGreaterThan(1);
  expect((await db.prepare("SELECT id FROM approval_steps WHERE instance_id=?").bind(blockedInstance).all()).results.length).toBeGreaterThan(0);
  await db.prepare("UPDATE assignments SET revoked_at=? WHERE id=?").bind(Date.now(), f.head.assignmentId).run();
  expect(await nativeExpenseReads(db, f.head).list()).toEqual(await nativeExpenseReads(db, f.head, {batch:false}).list());
});
