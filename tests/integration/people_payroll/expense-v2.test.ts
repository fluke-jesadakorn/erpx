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

// The Expense addon owns actual claims and their source payables. This does
// not withdraw base People or require ending unrelated Payroll/Benefits work.
async function expenseAddonGrant(packageId:'people'|'people-payroll'|'people-expenses',state:'active'|'draining'='active'){
 const {packageGrantStore,readCompanyPackageGrants}=await import('../../../src/server/modules/commercial/public/package-grants'),store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-expense-addon-provider'}),obligations:async()=>{throw Error('Concrete owner withdrawal required');}}),current=(await readCompanyPackageGrants(db,f.company)).find(row=>row.packageId===packageId);
 return store.apply({expectedVersion:current?.version??0,idempotencyKey:key(),grant:{companyId:f.company,packageId,packageVersion:1,state,startsAt:0,endsAt:null,version:(current?.version??0)+1,reason:'Reviewed disposable Expense addon lifecycle',evidence:'test:expense-addon-owner'}});
}
function expenseAddonScope(){return {companyId:f.company,packageId:'people-expenses',packageVersion:1};}
async function expenseAddonObservation(){const {withOwnerPackageObligations,readOwnerPackageObligations}=await import('../../../src/server/modules/commercial/public/owner-package-obligations');return withOwnerPackageObligations(db,expenseAddonScope(),tx=>readOwnerPackageObligations(db,tx,{...expenseAddonScope(),now:Date.now()}));}
async function expenseAddonWithdrawal(){const {readCompanyPackageGrants,packageGrantRevision}=await import('../../../src/server/modules/commercial/public/package-grants'),grants=await readCompanyPackageGrants(db,f.company),current=grants.find(row=>row.packageId==='people-expenses')!;return {companyId:f.company,expectedRevision:packageGrantRevision(f.company,grants),idempotencyKey:key(),changes:[{expectedVersion:current.version,grant:{...current,state:'inactive' as const,version:current.version+1,reason:'Actual Expense claims and original Finance sources settled',evidence:'test:expense-addon-owner'}}]};}
async function expenseAddonReview(request:Awaited<ReturnType<typeof expenseAddonWithdrawal>>,apply=false){const {packageGrantStore}=await import('../../../src/server/modules/commercial/public/package-grants'),{withOwnerPackageObligations,ownerPackageObligations}=await import('../../../src/server/modules/commercial/public/owner-package-obligations'),store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-expense-addon-provider'}),obligations:(tx,companyId,packageId)=>ownerPackageObligations(db,tx,companyId,packageId)});return withOwnerPackageObligations(db,expenseAddonScope(),tx=>apply?store.batch.applyIn(tx,request,async()=>{}):store.batch.previewIn(tx,request));}
async function expenseAddonImage(){return db.prepare(`SELECT
 (SELECT COALESCE(jsonb_agg(to_jsonb(e) ORDER BY id),'[]') FROM finance_expenses e WHERE company_id=?) expenses,
 (SELECT COALESCE(jsonb_agg(to_jsonb(d) ORDER BY id),'[]') FROM expense_payment_drafts d WHERE company_id=?) drafts,
 (SELECT COALESCE(jsonb_agg(to_jsonb(p) ORDER BY id),'[]') FROM finance_payments p WHERE company_id=?) payments,
 (SELECT COALESCE(jsonb_agg(to_jsonb(a) ORDER BY id),'[]') FROM finance_allocations a WHERE company_id=?) allocations,
 (SELECT COALESCE(jsonb_agg(to_jsonb(j) ORDER BY id),'[]') FROM finance_journals j WHERE company_id=?) journals,
 (SELECT COALESCE(jsonb_agg(to_jsonb(l) ORDER BY l.id),'[]') FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=?) lines,
 (SELECT COALESCE(jsonb_agg(to_jsonb(i) ORDER BY id),'[]') FROM finance_invoices i WHERE company_id=?) invoices,
 (SELECT COALESCE(jsonb_agg(to_jsonb(m) ORDER BY source_type,source_id,amount_kind),'[]') FROM money_transaction_snapshots m WHERE company_id=?) money,
 (SELECT COALESCE(jsonb_agg(to_jsonb(c) ORDER BY command_id),'[]') FROM command_receipts c WHERE authority_json::jsonb->>'companyId'=?) receipts,
 (SELECT COALESCE(jsonb_agg(to_jsonb(a) ORDER BY id),'[]') FROM audit_events a WHERE a.authority_json::jsonb->>'companyId'=?) audit,
 (SELECT COALESCE(jsonb_agg(to_jsonb(o) ORDER BY id),'[]') FROM outbox_events o JOIN command_receipts c ON c.command_id=o.command_id WHERE c.authority_json::jsonb->>'companyId'=?) outbox,
 (SELECT COALESCE(jsonb_agg(to_jsonb(g) ORDER BY package_id),'[]') FROM company_package_grants g WHERE company_id=?) grants,
 (SELECT COALESCE(jsonb_agg(to_jsonb(e) ORDER BY id),'[]') FROM company_package_grant_events e WHERE company_id=?) events,
 (SELECT COALESCE(jsonb_agg(to_jsonb(s) ORDER BY id),'[]') FROM company_package_change_sets s WHERE company_id=?) changes`).bind(...Array(14).fill(f.company)).first();}
async function postExpenseAddonClaim(id:string){await f.action(f.manager,id,'approve');const row=await getExpense(f.manager,id),input={expectedVersion:Number(row.version),periodId:f.period,entryDate:'2026-09-02'},command=key(),posted=await postExpense(f.manager,id,input,command);return {posted,input,command};}
it('withdraws only Expenses after independent rejection, cancellation, exact partial reimbursements and original unpaid reversal settle',async()=>{
 const {readCompanyPackageGrants}=await import('../../../src/server/modules/commercial/public/package-grants'),{reverseExpense}=await import('../../../src/server/modules/people_payroll/public/expenses');
 await expenseAddonGrant('people');await expenseAddonGrant('people-expenses');await expenseAddonGrant('people-payroll');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 try{
 expect((await expenseAddonObservation()).observations.map(row=>[row.moduleId,row.outstanding])).toEqual([['people_payroll',0],['finance',0]]);
 const cancelled=await f.claim(),rejected=await f.claim(),paidId=await f.claim(),reversedId=await f.claim();
 await f.attach(rejected);await f.action(f.claimant,rejected,'submit');await f.action(f.head,rejected,'reject');await f.throughAccounts(paidId);await f.throughAccounts(reversedId);
 await expenseAddonGrant('people-expenses','draining');const blocked=await expenseAddonWithdrawal(),before=await expenseAddonImage();await expect(expenseAddonReview(blocked)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');await expect(expenseAddonReview(blocked,true)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');expect(await expenseAddonImage()).toEqual(before);
 const cancelInput={expectedVersion:Number((await getExpense(f.claimant,cancelled)).version),reason:'Cancelled through the original claimant owner'},cancelKey=key(),cancelledResult=await transitionExpense(f.claimant,cancelled,'cancel',cancelInput,cancelKey);expect(await transitionExpense(f.claimant,cancelled,'cancel',cancelInput,cancelKey)).toEqual(cancelledResult);
 const paidSource=await postExpenseAddonClaim(paidId),reversedSource=await postExpenseAddonClaim(reversedId);
 const payments: {draftId:string;input:{expectedVersion:number;date:string;amount:string;reference:string};command:string;result:Record<string,unknown>}[]=[];
 for(const amount of ['40','67']){const row=await getExpense(f.payer,paidId),draftId=String((await createExpensePayment(f.payer,paidId,Number(row.version),key())).draftId);await f.attach(draftId,'expense_payment');const input={expectedVersion:1,date:'2026-09-03',amount,reference:key()},command=key(),result=await confirmExpensePayment(f.payer,draftId,input,command);expect(await confirmExpensePayment(f.payer,draftId,input,command)).toEqual(result);payments.push({draftId,input,command,result});}
 expect((await getExpense(f.claimant,paidId))).toMatchObject({status:'reimbursed',outstanding_minor:0});
 const reverseInput={expectedVersion:Number((await getExpense(f.manager,reversedId)).version),periodId:f.period,entryDate:'2026-09-03',reason:'Independent unpaid claim reversal retains original journals'},reverseKey=key(),reversal=await reverseExpense(f.manager,reversedId,reverseInput,reverseKey);expect(await reverseExpense(f.manager,reversedId,reverseInput,reverseKey)).toEqual(reversal);
 const final=await expenseAddonObservation();expect(final.observations.map(row=>[row.moduleId,row.outstanding])).toEqual([['people_payroll',0],['finance',0]]);
 const retained=await expenseAddonImage(),request=await expenseAddonWithdrawal(),preview=await expenseAddonReview(request);expect(await expenseAddonReview(request,true)).toEqual(preview);expect(await expenseAddonReview(request,true)).toEqual(preview);
 // Native committed command replays remain original after the addon becomes
 // inactive; no money, claim, journal or immutable command is applied twice.
 expect(await postExpense(f.manager,paidId,paidSource.input,paidSource.command)).toEqual(paidSource.posted);expect(await postExpense(f.manager,reversedId,reversedSource.input,reversedSource.command)).toEqual(reversedSource.posted);for(const payment of payments)expect(await confirmExpensePayment(f.payer,payment.draftId,payment.input,payment.command)).toEqual(payment.result);expect(await reverseExpense(f.manager,reversedId,reverseInput,reverseKey)).toEqual(reversal);
 const after=await expenseAddonImage();for(const field of ['expenses','drafts','payments','allocations','journals','lines','invoices','money','receipts','audit','outbox'] as const)expect(after?.[field]).toEqual(retained?.[field]);expect((await readCompanyPackageGrants(db,f.company)).filter(row=>['people','people-payroll'].includes(row.packageId)).map(row=>row.state)).toEqual(['active','active']);
 }finally{vi.unstubAllEnvs();}
});
it('holds absent Expense addon duties and rejects a diagnostic deferred cancelled-claim source change atomically',async()=>{
 const {withOwnerPackageObligations,ownerPackageObligations}=await import('../../../src/server/modules/commercial/public/owner-package-obligations'),{packageGrantStore}=await import('../../../src/server/modules/commercial/public/package-grants'),{peopleAddonPackageObligations}=await import('../../../src/server/modules/people_payroll/public/addon-package-obligations');
 await expenseAddonGrant('people');await expenseAddonGrant('people-expenses');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 try{
 await expect(db.transaction(tx=>peopleAddonPackageObligations(db,tx,{...expenseAddonScope(),moduleId:'people_payroll',now:Date.now()}))).rejects.toThrow('PEOPLE_ADDON_PACKAGE_OBLIGATION_SOURCE_REQUIRED');
 await withOwnerPackageObligations(db,expenseAddonScope(),async tx=>{await expect(peopleAddonPackageObligations(db,tx,{...expenseAddonScope(),moduleId:'finance',now:Date.now()})).rejects.toThrow('PEOPLE_ADDON_PACKAGE_OBLIGATION_SCOPE_REQUIRED');for(const table of ['finance_expenses','expense_payment_drafts','expense_ai_proposals','finance_allocations'])await expect(db.transaction(async writer=>{await writer.prepare("SET LOCAL lock_timeout='50ms'").run();await writer.prepare(`LOCK TABLE ${table} IN ROW EXCLUSIVE MODE`).run();})).rejects.toMatchObject({code:'55P03'});});
 const cancelled=await f.claim();await f.action(f.claimant,cancelled,'cancel');
 await expenseAddonGrant('people-expenses','draining');expect((await expenseAddonObservation()).total).toBe(0);const request=await expenseAddonWithdrawal(),before=await expenseAddonImage(),store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-expense-addon-provider'}),obligations:(tx,companyId,packageId)=>ownerPackageObligations(db,tx,companyId,packageId)}),name='expense_addon_fault_'+key().replaceAll('-','');
 // Adversarial source-state fault on an actually normally cancelled claim.
 // This is not a supported reopen, new-work purchase bypass or normal withdrawal.
 await db.prepare(`CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.company_id='${f.company}' AND NEW.package_id='people-expenses' AND NEW.state='inactive' THEN UPDATE finance_expenses SET status='draft',workflow_stage='draft',cancelled_at=NULL,version=version+1 WHERE id='${cancelled}'; END IF; RETURN NEW; END $$`).run();
 await db.prepare(`CREATE CONSTRAINT TRIGGER ${name} AFTER UPDATE ON company_package_grants DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION ${name}()`).run();
 try{await expect(withOwnerPackageObligations(db,expenseAddonScope(),tx=>store.batch.applyIn(tx,request,async()=>{}))).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');expect(await expenseAddonImage()).toEqual(before);}finally{await db.prepare(`DROP TRIGGER ${name} ON company_package_grants`).run();await db.prepare(`DROP FUNCTION ${name}()`).run();}
 const preview=await expenseAddonReview(request);expect(await expenseAddonReview(request,true)).toEqual(preview);expect(await expenseAddonReview(request,true)).toEqual(preview);
 }finally{vi.unstubAllEnvs();}
});

// Additive owner proof for the people.expenses purchase boundary. Existing
// Expense fixtures, native versions, Money/Files services and original policy
// remain authoritative; this does not claim zero-ledger startup coverage.
async function expenseBoundaryGrant(packageId:'people'|'people-expenses',state:'active'|'draining'='active',endsAt:number|null=null,companyId=f.company){
 const {packageGrantStore,readCompanyPackageGrants}=await import('../../../src/server/modules/commercial/public/package-grants');
 const store=packageGrantStore(db,{authorize:async()=>({userId:'isolated-expense-boundary-provider'}),obligations:async()=>{throw Error('Concrete owner withdrawal is required');}}),current=(await readCompanyPackageGrants(db,companyId)).find(row=>row.packageId===packageId);
 return store.apply({expectedVersion:current?.version??0,idempotencyKey:key(),grant:{companyId,packageId,packageVersion:1,state,startsAt:0,endsAt,version:(current?.version??0)+1,reason:'Reviewed Expense purchase boundary fixture',evidence:'test:expense-boundary'}});
}
function expenseBoundaryInput(){return expenseUpdateSchema.parse({expectedVersion:1,expenseDate:'2026-09-01',merchant:'Actual Expense boundary merchant',receiptReference:key(),businessPurpose:'Original Expense purchase and finality proof',currency:'THB',amount:'100',tax:'7',categoryAccountId:f.accounts.expense});}
async function expenseBoundaryImage(){return db.prepare(`SELECT jsonb_build_object(
 'expenses',(SELECT COALESCE(jsonb_agg(to_jsonb(e) ORDER BY e.id),'[]') FROM finance_expenses e WHERE e.company_id=?),
 'drafts',(SELECT COALESCE(jsonb_agg(to_jsonb(d) ORDER BY d.id),'[]') FROM expense_payment_drafts d WHERE d.company_id=?),
 'invoices',(SELECT COALESCE(jsonb_agg(to_jsonb(i) ORDER BY i.id),'[]') FROM finance_invoices i WHERE i.company_id=?),
 'journals',(SELECT COALESCE(jsonb_agg(to_jsonb(j) ORDER BY j.id),'[]') FROM finance_journals j WHERE j.company_id=?),
 'money',(SELECT COALESCE(jsonb_agg(to_jsonb(m) ORDER BY m.source_type,m.source_id,m.amount_kind),'[]') FROM money_transaction_snapshots m WHERE m.company_id=?),
 'receipts',(SELECT COALESCE(jsonb_agg(to_jsonb(c) ORDER BY c.command_id),'[]') FROM command_receipts c WHERE c.authority_json::jsonb->>'companyId'=?),
 'audit',(SELECT COALESCE(jsonb_agg(to_jsonb(a) ORDER BY a.id),'[]') FROM audit_events a WHERE a.authority_json::jsonb->>'companyId'=?),
 'outbox',(SELECT COALESCE(jsonb_agg(to_jsonb(o) ORDER BY o.id),'[]') FROM outbox_events o JOIN command_receipts c ON c.command_id=o.command_id WHERE c.authority_json::jsonb->>'companyId'=?),
 'files',(SELECT COALESCE(jsonb_agg(to_jsonb(s) ORDER BY s.id),'[]') FROM attachment_sets s WHERE s.company_id=?),
 'assignments',(SELECT COALESCE(jsonb_agg(to_jsonb(a) ORDER BY a.id),'[]') FROM assignments a WHERE a.company_id=?)
 )::text image`).bind(...Array(10).fill(f.company)).first<string>('image');}
it('requires the actual Expenses addon for native commands reads and Files while disabled and shadow retain original behavior',async()=>{
 const {listExpenses,expenseOptions}=await import('../../../src/server/modules/people_payroll/public/expenses'),{nativeExpenseDrafts}=await import('../../../src/server/modules/people_payroll/adapters/expense-drafts'),{createAttachmentSet}=await import('../../../src/server/modules/files/public/attachments/service');
 const original=await f.claim(),input=expenseBoundaryInput(),file={targetType:'expense_claim',targetId:original,componentKind:'record' as const,purposeKey:'receipt',expectedTargetVersion:1};
 await expenseBoundaryGrant('people');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_ATTACHMENTS_V2','true');
 try{
  const before=await expenseBoundaryImage();
  await expect(saveExpense(f.claimant,null,input,key())).rejects.toMatchObject({status:403,code:'PACKAGE_REQUIRED'});
  await expect(db.transaction(tx=>nativeExpenseDrafts(tx)(f.claimant,null,input,key()))).rejects.toMatchObject({status:403,code:'PACKAGE_REQUIRED'});
  await expect(listExpenses(f.claimant)).rejects.toMatchObject({status:403,code:'PACKAGE_REQUIRED'});
  await expect(getExpense(f.claimant,original)).rejects.toMatchObject({status:403,code:'PACKAGE_REQUIRED'});
  await expect(expenseOptions(f.claimant)).rejects.toMatchObject({status:403,code:'PACKAGE_REQUIRED'});
  const {getWorkCapabilities}=await import('../../../src/server/modules/work_projection/public/work-capabilities');const work=await getWorkCapabilities(db,f.claimant);expect(work.creation.some(entry=>entry.kind==='expense')).toBe(false);expect(work.creation.some(entry=>entry.kind==='service_request')).toBe(true);
  await expect(createAttachmentSet(db,f.claimant,file,key())).rejects.toMatchObject({status:403,code:'PACKAGE_REQUIRED'});
  expect(await expenseBoundaryImage()).toBe(before);
  vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','shadow');const log=vi.spyOn(console,'info').mockImplementation(()=>{});
  try{expect(await saveExpense(f.claimant,null,input,key())).toMatchObject({status:'draft',version:1});expect(log.mock.calls.some(([value])=>String(value).includes('commercial.purchase.shadow'))).toBe(true);}finally{log.mockRestore();}
  vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','disabled');expect(await saveExpense(f.claimant,null,{...input,receiptReference:key()},key())).toMatchObject({status:'draft',version:1});
 }finally{vi.unstubAllEnvs();}
});
it('keeps exact Expense replay and original cancellation while draining but denies genuinely new draft amendment submission and AI work',async()=>{
 const {expensePurchaseControls}=await import('../../../src/server/modules/people_payroll/public/expense-purchases'),{requestExpenseAi}=await import('../../../src/server/modules/people_payroll/expense');
 await expenseBoundaryGrant('people');await expenseBoundaryGrant('people-expenses');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 try{
  const input=expenseBoundaryInput(),command=key(),created=await saveExpense(f.claimant,null,input,command),id=String(created.id);
  await expenseBoundaryGrant('people-expenses','draining');expect(await expensePurchaseControls(f.claimant)).toMatchObject({newWork:false,settleExisting:true,history:true});
  const {getWorkCapabilities}=await import('../../../src/server/modules/work_projection/public/work-capabilities');expect((await getWorkCapabilities(db,f.claimant)).creation.some(entry=>entry.kind==='expense')).toBe(false);
  const before=await expenseBoundaryImage();expect(await saveExpense(f.claimant,null,input,command)).toEqual(created);
  await expect(saveExpense(f.claimant,null,{...input,merchant:'Different original request'},command)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});
  await expect(saveExpense(f.claimant,null,input,key())).rejects.toMatchObject({status:403,code:'PACKAGE_DRAINING'});
  await expect(saveExpense(f.claimant,id,{...input,expectedVersion:1},key())).rejects.toMatchObject({status:403,code:'PACKAGE_DRAINING'});
  await expect(transitionExpense(f.claimant,id,'submit',{expectedVersion:1},key())).rejects.toMatchObject({status:403,code:'PACKAGE_DRAINING'});
  await expect(requestExpenseAi(f.claimant,id,{expectedVersion:1,paymentId:null,retry:false},key())).rejects.toMatchObject({status:403,code:'PACKAGE_DRAINING'});
  expect(await expenseBoundaryImage()).toBe(before);
  const cancelInput={expectedVersion:1,reason:'Original claimant settles retained draft'},cancelKey=key(),cancelled=await transitionExpense(f.claimant,id,'cancel',cancelInput,cancelKey);
  expect(cancelled).toMatchObject({id,status:'cancelled',version:2});const settled=await expenseBoundaryImage();
  expect(await transitionExpense(f.claimant,id,'cancel',cancelInput,cancelKey)).toEqual(cancelled);expect(await saveExpense(f.claimant,null,input,command)).toEqual(created);expect(await expenseBoundaryImage()).toBe(settled);
  expect(await getExpense(f.claimant,id)).toMatchObject({id,status:'cancelled',expense_purchase_new_work:0});
 }finally{vi.unstubAllEnvs();}
});
it('rolls back deferred actual Expense assignment revocation and native expiry after owner cleanup without losing the original retry',async()=>{
 await expenseBoundaryGrant('people');await expenseBoundaryGrant('people-expenses');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 const suffix=key().replaceAll('-',''),fault=`expense_boundary_${suffix}`,input=expenseBoundaryInput(),command=key();
 try{
  const before=await expenseBoundaryImage();
  await db.prepare(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN UPDATE assignments SET revoked_at=floor(extract(epoch FROM clock_timestamp())*1000)::bigint,version=version+1 WHERE id='${f.claimant.assignmentId}'; RETURN NEW; END $$`).run();
  await db.prepare(`CREATE CONSTRAINT TRIGGER ${fault} AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${fault}()`).run();
  try{await expect(saveExpense(f.claimant,null,input,command)).rejects.toMatchObject({status:403,code:'EXPENSE_ACCESS_CHANGED'});expect(await expenseBoundaryImage()).toBe(before);}finally{await db.prepare(`DROP TRIGGER ${fault} ON outbox_events`).run();await db.prepare(`DROP FUNCTION ${fault}()`).run();}
  const created=await saveExpense(f.claimant,null,input,command);expect(created).toMatchObject({id:`expense-${command}`,status:'draft',version:1});const retried=await expenseBoundaryImage();expect(await saveExpense(f.claimant,null,input,command)).toEqual(created);expect(await expenseBoundaryImage()).toBe(retried);
  const expiryKey=key(),expiryInput={...input,receiptReference:key()},expiryFault=`${fault}_expiry`;await expenseBoundaryGrant('people-expenses','active',Date.now()+2000);const expiring=await expenseBoundaryImage();
  await db.prepare(`CREATE FUNCTION ${expiryFault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(2.2); RETURN NEW; END $$`).run();
  await db.prepare(`CREATE CONSTRAINT TRIGGER ${expiryFault} AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${expiryKey}') EXECUTE FUNCTION ${expiryFault}()`).run();
  try{await expect(saveExpense(f.claimant,null,expiryInput,expiryKey)).rejects.toMatchObject({status:403,code:'PACKAGE_DRAINING'});expect(await expenseBoundaryImage()).toBe(expiring);}finally{await db.prepare(`DROP TRIGGER ${expiryFault} ON outbox_events`).run();await db.prepare(`DROP FUNCTION ${expiryFault}()`).run();}
  expect(await transitionExpense(f.claimant,String(created.id),'cancel',{expectedVersion:1,reason:'Original expiry permits existing settlement'},key())).toMatchObject({id:created.id,status:'cancelled',version:2});
 }finally{vi.unstubAllEnvs();}
});
it('serializes actual cross-Company Expense and Commercial writers before their snapshots with no deadlock serialization or retry',async()=>{
 const other=await expenseFixture(),{nativeExpenseDrafts}=await import('../../../src/server/modules/people_payroll/adapters/expense-drafts'),{packageGrantStore,readCompanyPackageGrants}=await import('../../../src/server/modules/commercial/public/package-grants');
 for(const company of [f.company,other.company]){await expenseBoundaryGrant('people','active',null,company);await expenseBoundaryGrant('people-expenses','active',null,company);}
 vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 let release:()=>void=()=>{},entered:()=>void=()=>{};const heldGate=new Promise<void>(resolve=>{release=resolve;}),ready=new Promise<void>(resolve=>{entered=resolve;});
 try{
  const observed=await measureReads(async()=>{
   const input=expenseBoundaryInput(),firstKey=key(),held=db.transaction(async transaction=>{const result=await nativeExpenseDrafts(transaction)(f.claimant,null,input,firstKey);entered();await heldGate;return result;});
   // Propagate preparation failure instead of hanging on an unfulfilled barrier.
   await Promise.race([ready,held.then(()=>{throw Error('Held Expense unit ended before its barrier');})]);
   const secondKey=key(),sourceWriter=nativeExpenseDrafts(db)(other.claimant,null,{...input,categoryAccountId:other.accounts.expense},secondKey),grantKey=key(),grant={companyId:other.company,packageId:'people-expenses',packageVersion:1,state:'active' as const,startsAt:0,endsAt:null,version:2,reason:'Actual independent Company Commercial source writer',evidence:'test:expense-cross-company-fence'};
   sourceWriter.catch(()=>{});
   const commercialWriter=packageGrantStore(db,{authorize:async()=>({userId:'isolated-expense-cross-company-provider'}),obligations:async()=>{throw Error('No withdrawal in this actual writer');}}).apply({expectedVersion:1,idempotencyKey:grantKey,grant});
   commercialWriter.catch(()=>{});let blocked=false,observationError:unknown;
   try{
    for(let attempt=0;attempt<100&&!blocked;attempt++){
     blocked=!!await db.prepare("SELECT 1 FROM pg_locks WHERE database=(SELECT oid FROM pg_database WHERE datname=current_database()) AND relation='company_package_grants'::regclass AND mode IN ('RowExclusiveLock','ShareRowExclusiveLock') AND NOT granted LIMIT 1").first();
     if(!blocked)await new Promise(resolve=>setTimeout(resolve,10));
    }
   }catch(error){observationError=error;}finally{release();}
   const completed=await Promise.allSettled([held,sourceWriter,commercialWriter]);
   if(observationError)throw observationError;expect(blocked).toBe(true);for(const result of completed)if(result.status==='rejected')throw result.reason;
   expect(completed.map(result=>result.status)).toEqual(['fulfilled','fulfilled','fulfilled']);
   expect((await db.prepare('SELECT id,company_id FROM finance_expenses WHERE id IN(?,?) ORDER BY company_id').bind(`expense-${firstKey}`,`expense-${secondKey}`).all()).results).toHaveLength(2);
   expect((await readCompanyPackageGrants(db,other.company)).find(row=>row.packageId==='people-expenses')).toMatchObject({version:2,state:'active'});
   return completed;
  });
  expect({deadlockFailures:observed.metrics.deadlockFailures??0,serializationFailures:observed.metrics.serializationFailures??0,transactionRetries:observed.metrics.transactionRetries??0}).toEqual({deadlockFailures:0,serializationFailures:0,transactionRetries:0});
 }finally{release();vi.unstubAllEnvs();}
});
it('retains the real captured queue author and reaches the original Expense AI readiness guard inside the purchased command unit',async()=>{
 const {queueExpenseAi}=await import('../../../src/server/modules/people_payroll/public/expense-ai'),{ApiError}=await import('../../../src/server/modules/runtime/public/http');
 const id=await f.claim(),input={expectedVersion:1,paymentId:null,retry:false};
 // This source has no receipt. The original readiness/receipt guard must
 // refuse before creating a provider job; no provider request is executed.
 const guard=async()=>{try{await queueExpenseAi(db,f.claimant,{},id,input,key());throw Error('The original no-evidence queue guard was bypassed');}catch(error){if(!(error instanceof ApiError))throw error;return {status:error.status,code:error.code};}};
 const original=await guard();expect(original.code).not.toBe('EXPENSE_TRANSACTION_REQUIRED');
 await expenseBoundaryGrant('people');await expenseBoundaryGrant('people-expenses');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 try{const before=await expenseBoundaryImage();expect(await guard()).toEqual(original);expect(await expenseBoundaryImage()).toBe(before);expect(await db.prepare('SELECT count(*) n FROM expense_ai_proposals WHERE company_id=? AND expense_id=?').bind(f.company,id).first<number>('n')).toBe(0);}finally{vi.unstubAllEnvs();}
});

it('uses original Expense visibility inside the same before-limit record read and rejects a late native source change',async()=>{
 const {withExpenseWorkspaceRecordRead}=await import('../../../src/server/modules/people_payroll/public/record-purchases'),{payrollSourceReadScope}=await import('../../../src/server/modules/people_payroll/public/record-purchases');
 await expenseBoundaryGrant('people');await expenseBoundaryGrant('people-expenses');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 try{
  const id=await f.claim();await expect(getExpense(f.it,id)).rejects.toMatchObject({status:404,code:'EXPENSE_NOT_FOUND'});
  // Deliberately adverse imported/shared projection. It creates NO Expense,
  // approval or receipt and must not grant its title to a generic reader.
  await db.prepare("INSERT INTO workspace_records(module_id,id,section_id,title,description,status,fields_json,company_id,branch_id,department_id,version,created_by,updated_by,created_at,updated_at) SELECT 'people_payroll',id,'expense-projection-fixture',merchant,business_purpose,'draft','[]',company_id,branch_id,department_id,1,created_by,created_by,created_at,updated_at FROM finance_expenses WHERE id=? AND company_id=?").bind(id,f.company).run();
  const rows=(actor:typeof f.claimant)=>withExpenseWorkspaceRecordRead(db,actor,async transaction=>{const scope=await payrollSourceReadScope(transaction,actor,'records');return (await transaction.prepare(`SELECT workspace_records.id FROM workspace_records WHERE workspace_records.company_id=? AND ${scope.where} ORDER BY workspace_records.updated_at DESC LIMIT 12`).bind(f.company,...scope.args).all<{id:string}>()).results;});
  expect(await rows(f.it)).toEqual([]);expect(await rows(f.claimant)).toEqual([{id}]);
  const before=await expenseBoundaryImage();await expect(withExpenseWorkspaceRecordRead(db,f.claimant,async transaction=>{
   const scope=await payrollSourceReadScope(transaction,f.claimant,'records');expect((await transaction.prepare(`SELECT workspace_records.id FROM workspace_records WHERE workspace_records.company_id=? AND ${scope.where} LIMIT 12`).bind(f.company,...scope.args).all()).results).toEqual([{id}]);
   // A labelled adverse inner-owner effect tests the final source fence;
   // it is not an authorized shared-record mutation or positive setup.
   await transaction.prepare("UPDATE finance_expenses SET merchant='Adverse late shared-record mutation' WHERE id=? AND company_id=?").bind(id,f.company).run();
  })).rejects.toMatchObject({status:409,code:'EXPENSE_RECORD_SOURCE_CHANGED'});expect(await expenseBoundaryImage()).toBe(before);expect(await rows(f.claimant)).toEqual([{id}]);
 }finally{vi.unstubAllEnvs();}
});

it('keeps the complete actual Expense preview in its native read unit through a late original staff-authority change',async()=>{
 const {previewExpense}=await import('../../../src/server/modules/people_payroll/expense'),staffOwner=await import('../../../src/server/modules/operations/public/staff-policy');
 await expenseBoundaryGrant('people');await expenseBoundaryGrant('people-expenses');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 let intercepted=false;const nativeActiveStaff=staffOwner.activeStaff;
 try{
  const ready=await previewExpense(f.claimant,{});expect(ready.steps.length).toBeGreaterThan(0);expect(ready.blockers).toEqual([]);
  const before=await expenseBoundaryImage(),read=vi.spyOn(staffOwner,'activeStaff').mockImplementation(async(database,companyId,userId)=>{
   const actual=await nativeActiveStaff(database,companyId,userId);
   if(!intercepted&&companyId===f.company&&userId===f.claimant.userId){
    intercepted=true;expect(database).not.toBe(db);
    // Deliberately adverse fact immediately after the real original staff
    // read; all original claimant/route policy still executes afterward.
    await database.prepare('UPDATE assignments SET revoked_at=?,version=version+1 WHERE id=? AND company_id=?').bind(Date.now(),f.claimant.assignmentId,f.company).run();
   }
   return actual;
  });
  try{await expect(previewExpense(f.claimant,{})).rejects.toMatchObject({status:403,code:'EXPENSE_ACCESS_CHANGED'});expect(intercepted).toBe(true);expect(await expenseBoundaryImage()).toBe(before);}finally{read.mockRestore();}
  expect(await previewExpense(f.claimant,{})).toEqual(ready);
 }finally{vi.unstubAllEnvs();}
});

it('closes an actual unconfirmed Expense payment preparation after lawful parent reversal and retains exact Treasury history through addon withdrawal',async()=>{
 const {reverseExpense}=await import('../../../src/server/modules/people_payroll/public/expenses');await expenseAddonGrant('people');await expenseAddonGrant('people-expenses');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 const oldGate=process.env.ERPX_ATTACHMENTS_V2;
 try{
  const id=await f.claim();await f.throughAccounts(id);await postExpenseAddonClaim(id);const posted=await getExpense(f.payer,id),draftId=String((await createExpensePayment(f.payer,id,Number(posted.version),key())).draftId);
  // Original Files record binding advances only the native draft version; no
  // payment, payable or positive Finance source is fabricated by this fixture.
  process.env.ERPX_ATTACHMENTS_V2='true';const file=await seedAttachmentSet(db,f.payer,{targetType:'expense_payment',targetId:draftId,componentKind:'action',componentRef:'legacy-upload-adapter',purposeKey:'payment_slip',expectedTargetVersion:1});await (await import('../../../src/server/modules/files/public/attachments/service')).bindCompatibilityAttachmentSet(db,f.payer,file.setId,1,key());
  const original=await db.prepare('SELECT * FROM expense_payment_drafts WHERE id=?').bind(draftId).first<{version:number;payment_id:string|null;status:string}>();expect(original).toMatchObject({version:2,status:'draft',payment_id:null});
  const reversal=await reverseExpense(f.manager,id,{expectedVersion:Number((await getExpense(f.manager,id)).version),periodId:f.period,entryDate:'2026-09-03',reason:'Original wholly unpaid claim is no longer owed'},key());expect(reversal.status).toBe('cancelled');
  // Genuine old-source recovery gap: original Treasury could no longer open
  // this reversed parent, although its native unconfirmed draft remained due.
  const parent=await getExpense(f.payer,id);expect(parent).toMatchObject({status:'cancelled'});expect((await expenseDetail(f.payer,id)).paymentDrafts.find(draft=>draft.id===draftId)).toMatchObject({can_cancel_payment:1});await expect(getExpense(f.it,id)).rejects.toMatchObject({status:404});
  const {cancelExpensePayment}=await import('../../../src/server/modules/people_payroll/public/expense-payments'),input={expectedVersion:2,expenseVersion:Number(parent.version),reason:'Independent Treasury verified that this preparation was never paid',paymentNotExecuted:true as const},command=key();
  await expenseAddonGrant('people-expenses','draining');expect((await expenseAddonObservation()).observations.find(row=>row.moduleId==='people_payroll')!.outstanding).toBeGreaterThan(0);const request=await expenseAddonWithdrawal();await expect(expenseAddonReview(request,true)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
  const unchanged=await expenseAddonImage();await expect(cancelExpensePayment(f.manager,id,draftId,input,key())).rejects.toMatchObject({status:403});await expect(cancelExpensePayment(f.payer,'another-claim',draftId,input,key())).rejects.toMatchObject({status:404});await expect(cancelExpensePayment(f.payer,id,draftId,{...input,expectedVersion:1},key())).rejects.toMatchObject({code:'EXPENSE_PAYMENT_CHANGED'});expect(await expenseAddonImage()).toEqual(unchanged);
  const result=await cancelExpensePayment(f.payer,id,draftId,input,command);expect(result).toEqual({id:draftId,expenseId:id,status:'cancelled',version:3,expenseVersion:input.expenseVersion,reason:input.reason,paymentNotExecuted:true});expect(await cancelExpensePayment(f.payer,id,draftId,input,command)).toEqual(result);
  const detail=await expenseDetail(f.payer,id);expect(detail.expense).toMatchObject({status:'cancelled'});expect(detail.paymentDrafts.find(draft=>draft.id===draftId)).toMatchObject({status:'cancelled',version:3,can_cancel_payment:0,cancellation_reason:input.reason});expect(detail.paymentAttachments.map(file=>file.id)).toContain(file.sourceId);
  expect(await db.prepare('SELECT action,record_id,record_version FROM audit_events WHERE command_id=?').bind(command).first()).toEqual({action:'expense.payment.cancel',record_id:draftId,record_version:3});expect((await expenseAddonObservation()).observations.map(row=>[row.moduleId,row.outstanding])).toEqual([['people_payroll',0],['finance',0]]);
  const before=await expenseAddonImage(),withdrawal=await expenseAddonWithdrawal();await expenseAddonReview(withdrawal,true);expect(await cancelExpensePayment(f.payer,id,draftId,input,command)).toEqual(result);expect((await expenseDetail(f.payer,id)).paymentDrafts.find(draft=>draft.id===draftId)).toMatchObject({status:'cancelled',version:3});const after=await expenseAddonImage();for(const field of ['expenses','drafts','payments','allocations','journals','lines','invoices','money','receipts','audit','outbox'] as const)expect(after?.[field]).toEqual(before?.[field]);
 }finally{if(oldGate===undefined)delete process.env.ERPX_ATTACHMENTS_V2;else process.env.ERPX_ATTACHMENTS_V2=oldGate;vi.unstubAllEnvs();}
});
it('cancelling an abandoned payment preparation leaves its genuine payable due and cannot cancel an actual confirmed bank payment',async()=>{
 const {cancelExpensePayment}=await import('../../../src/server/modules/people_payroll/public/expense-payments');await expenseAddonGrant('people');await expenseAddonGrant('people-expenses');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');
 try{const id=await f.claim();await f.throughAccounts(id);await postExpenseAddonClaim(id);const parent=await getExpense(f.payer,id),abandoned=String((await createExpensePayment(f.payer,id,Number(parent.version),key())).draftId),input={expectedVersion:1,expenseVersion:Number(parent.version),reason:'Independent Treasury verified this unused bank preparation',paymentNotExecuted:true as const};await cancelExpensePayment(f.payer,id,abandoned,input,key());
  expect(await getExpense(f.payer,id)).toMatchObject({status:'posted',outstanding_minor:10700,version:parent.version});await expenseAddonGrant('people-expenses','draining');const withdrawal=await expenseAddonWithdrawal();await expect(expenseAddonReview(withdrawal,true)).rejects.toThrow('PACKAGE_OBLIGATIONS_REMAIN');
  const actual=String((await createExpensePayment(f.payer,id,Number(parent.version),key())).draftId);await f.attach(actual,'expense_payment');await confirmExpensePayment(f.payer,actual,{expectedVersion:1,date:'2026-09-03',amount:'107',reference:key()},key());const current=await getExpense(f.payer,id),before=await expenseAddonImage();await expect(cancelExpensePayment(f.payer,id,actual,{...input,expenseVersion:Number(current.version),expectedVersion:2},key())).rejects.toMatchObject({code:'EXPENSE_PAYMENT_NOT_CANCELLABLE'});expect(await expenseAddonImage()).toEqual(before);expect((await expenseAddonObservation()).total).toBe(0);await expenseAddonReview(await expenseAddonWithdrawal(),true);
 }finally{vi.unstubAllEnvs();}
});
it('payment cancellation checks its exact original parent and child again after deferred effects in enforced and ordinary disabled mode',async()=>{
 const {cancelExpensePayment}=await import('../../../src/server/modules/people_payroll/public/expense-payments');await expenseAddonGrant('people');await expenseAddonGrant('people-expenses');
 for(const mode of ['disabled','enforced'] as const){vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT',mode);
  try{const id=await f.claim();await f.throughAccounts(id);await postExpenseAddonClaim(id);const parent=await getExpense(f.payer,id),draftId=String((await createExpensePayment(f.payer,id,Number(parent.version),key())).draftId),input={expectedVersion:1,expenseVersion:Number(parent.version),reason:'Verified unexecuted preparation with exact final source',paymentNotExecuted:true as const};
   let retryCommand='';for(const target of ['parent','draft'] as const){const command=key(),name=`expense_cancel_final_${key().replaceAll('-','')}`;retryCommand=command;await db.exec(`CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN ${target==='parent'?`UPDATE finance_expenses SET business_purpose='Adverse deferred parent mutation' WHERE id='${id}' AND company_id='${f.company}';`:`UPDATE expense_payment_drafts SET reference='Adverse deferred draft mutation' WHERE id='${draftId}' AND company_id='${f.company}';`} RETURN NEW;END $$; CREATE CONSTRAINT TRIGGER ${name} AFTER INSERT ON command_receipts DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${name}()`);
    try{const before=await expenseAddonImage();await expect(cancelExpensePayment(f.payer,id,draftId,input,command)).rejects.toMatchObject({code:'EXPENSE_PAYMENT_CHANGED'});expect(await expenseAddonImage()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${name} ON command_receipts;DROP FUNCTION ${name}()`);}
   }
   const cancelled=await cancelExpensePayment(f.payer,id,draftId,input,retryCommand);expect(cancelled).toMatchObject({id:draftId,status:'cancelled',version:2});const terminal=await expenseAddonImage();expect(await cancelExpensePayment(f.payer,id,draftId,input,retryCommand)).toEqual(cancelled);expect(await expenseAddonImage()).toEqual(terminal);
  }finally{vi.unstubAllEnvs();}
 }
});

// Additive closed Expense startup proof. Organization, workers and assignments
// are explicitly imported fixture foundations; this Company starts with ZERO
// Finance charts/accounts/periods/rates/mapping. Every new financial reference,
// mapping, claim, review and posting below uses its real original owner command.
async function expenseSetupCounts(){return db.prepare(`SELECT
 (SELECT count(*) FROM finance_charts WHERE company_id=?) charts,
 (SELECT count(*) FROM finance_accounts WHERE company_id=?) accounts,
 (SELECT count(*) FROM finance_periods WHERE company_id=?) periods,
 (SELECT count(*) FROM company_fx_rates WHERE company_id=?) rates,
 (SELECT count(*) FROM commerce_settings WHERE company_id=?) mappings,
 (SELECT count(*) FROM finance_journals WHERE company_id=?) journals,
 (SELECT count(*) FROM money_transaction_snapshots WHERE company_id=?) money`).bind(...Array(7).fill(f.company)).first();}
async function expenseSetupImage(){
 const tables=['finance_charts','finance_accounts','finance_periods','company_fx_rates','company_money_settings','company_currency_scales','commerce_settings','finance_sales_defaults','finance_expenses','expense_payment_drafts','finance_invoices','finance_payments','finance_allocations','finance_journals','finance_financial_events','finance_book_postings','money_transaction_snapshots','assignments','company_package_grants','company_package_grant_events','company_package_change_sets'] as const;
 return {rows:await Promise.all(tables.map(async table=>({table,rows:(await db.prepare(`SELECT to_jsonb(r)::text image FROM ${table} r WHERE r.company_id=? ORDER BY image`).bind(f.company).all()).results}))),
 lines:(await db.prepare('SELECT to_jsonb(l)::text image FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? ORDER BY image').bind(f.company).all()).results,
 receipts:(await db.prepare("SELECT to_jsonb(r)::text image FROM command_receipts r WHERE r.authority_json::jsonb->>'companyId'=? ORDER BY image").bind(f.company).all()).results,
 audit:(await db.prepare("SELECT to_jsonb(r)::text image FROM audit_events r WHERE r.authority_json::jsonb->>'companyId'=? ORDER BY image").bind(f.company).all()).results,
 outbox:(await db.prepare("SELECT to_jsonb(o)::text image FROM outbox_events o JOIN command_receipts r ON r.command_id=o.command_id WHERE r.authority_json::jsonb->>'companyId'=? ORDER BY image").bind(f.company).all()).results};
}
async function expenseSetupZero(purchased=true){
 f=await expenseFixture(false,false,false);
 const {mapSpendingOrganization}=await import('../../support/helpers/spending-organization');await mapSpendingOrganization(f);
 expect(await expenseSetupCounts()).toEqual({charts:0,accounts:0,periods:0,rates:0,mappings:0,journals:0,money:0});
 await expenseBoundaryGrant('people');if(purchased)await expenseBoundaryGrant('people-expenses');
 vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');
 const {readCompanyPackageGrants}=await import('../../../src/server/modules/commercial/public/package-grants');
 expect((await readCompanyPackageGrants(db,f.company)).map(row=>row.packageId).sort()).toEqual(purchased?['people','people-expenses']:['people']);
 return {api:await import('../../../src/server/modules/finance/public/expense-posting-prerequisites'),mapping:await import('../../../src/server/modules/commerce/public/expense-posting-setup')};
}
async function expenseSetupEnvelope(actor=f.reviewer){const {expensePostingPrerequisites}=await import('../../../src/server/modules/finance/public/expense-posting-prerequisites'),view=await expensePostingPrerequisites(actor);if(!view.available||!view.contextKey)throw Error('Actual current Company Expense prerequisite context required');return {expectedVersion:0 as const,contextKey:view.contextKey};}
async function expenseSetupReferences(){
 const api=await import('../../../src/server/modules/finance/public/expense-posting-prerequisites'),initial=await api.expensePostingPrerequisites(f.reviewer);expect(initial).toMatchObject({available:true,configured:false,defaultCurrency:'USD',canCreateChart:true,canCreateAccount:true,canCreatePeriod:true,canConfirmRate:false,charts:[],accounts:[],periods:[],rates:[]});
 const chartBody={...await expenseSetupEnvelope(),input:{name:'Actual first Expense functional chart',baseCurrency:'THB',accountingBasis:'IFRS'}},chartKey=key(),chart=await api.createExpensePostingChart(f.reviewer,chartBody,chartKey);expect(chart).toMatchObject({companyId:f.company,baseCurrency:'THB',version:1});expect(chart.id).toMatch(/^chart-/);
 const accounts={receivableAccountId:'',payableAccountId:'',revenueAccountId:'',inventoryAccountId:'',clearingAccountId:'',cogsAccountId:'',inputTaxAccountId:'',outputTaxAccountId:'',cashAccountId:''};
 for(const [field,code,category,controlKind] of [['receivableAccountId','1100','asset','receivable'],['payableAccountId','2000','liability','payable'],['revenueAccountId','4000','revenue','none'],['inventoryAccountId','1200','asset','none'],['clearingAccountId','2100','liability','none'],['cogsAccountId','5000','expense','none'],['inputTaxAccountId','1300','asset','none'],['outputTaxAccountId','2200','liability','none'],['cashAccountId','1000','asset','bank']] as const){
  const body={...await expenseSetupEnvelope(),input:{chartId:chart.id,code,name:`Actual original ${field}`,category,currency:'THB',controlKind,isControl:controlKind!=='none'}},command=key(),account=await api.createExpensePostingAccount(f.reviewer,body,command);expect(account).toMatchObject({companyId:f.company,chartId:chart.id,currency:'THB',version:1});accounts[field]=account.id;
 }
 const periodBody={...await expenseSetupEnvelope(),input:{name:'Actual first Expense period',startDate:'2026-01-01',endDate:'2026-12-31'}},periodKey=key(),period=await api.createExpensePostingPeriod(f.reviewer,periodBody,periodKey);expect(period).toMatchObject({companyId:f.company,state:'open',version:0});
 const rateBody={...await expenseSetupEnvelope(f.manager),input:{currency:'THB',effectiveDate:'2026-01-01',currencyPerUsd:'36',note:'Explicit fictional manual quote confirmed through original Finance owner'}},rateKey=key(),rate=await api.confirmExpensePostingRate(f.manager,rateBody,rateKey);expect(rate).toMatchObject({currency:'THB',currencyPerUsd:'36'});expect(rate).not.toHaveProperty('version');
 expect(await expenseSetupCounts()).toEqual({charts:1,accounts:9,periods:1,rates:1,mappings:0,journals:0,money:0});
 return {api,chart,chartBody,chartKey,accounts,period,periodBody,periodKey,rate,rateBody,rateKey};
}
async function expenseSetupMappingInput(refs:Awaited<ReturnType<typeof expenseSetupReferences>>){const api=await import('../../../src/server/modules/commerce/public/expense-posting-setup'),view=await api.expensePostingSetup(f.reviewer);if(!view.contextKey)throw Error('Actual initial mapping context required');return api.expensePostingSetupSchema.parse({expectedVersion:0,contextKey:view.contextKey,chartId:refs.chart.id,currency:'THB',...refs.accounts,expectedSalesDefaults:view.expectedSalesDefaults,reason:'Current Company Finance prepares all nine original Expense posting roles'});}

it('starts with zero Finance references and only People plus Expenses, then uses closed original constructors and independent review before actual claim posting',async()=>{
 try{
  const source=await expenseSetupZero(),refs=await expenseSetupReferences(),mappingBody=await expenseSetupMappingInput(refs),mappingKey=key();await expenseSetupFault(mappingKey,`UPDATE commerce_settings SET currency='EUR' WHERE company_id='${f.company}'`,()=>source.mapping.initializeExpensePostingSetup(f.reviewer,mappingBody,mappingKey),{status:409,code:'EXPENSE_POSTING_SOURCE_CHANGED'});const configured=await source.mapping.initializeExpensePostingSetup(f.reviewer,mappingBody,mappingKey);
  expect(configured).toEqual({id:f.company,status:'configured',version:1});expect((await source.mapping.expensePostingSetup(f.reviewer))).toMatchObject({configured:true,canConfigure:false,configuration:{chartId:refs.chart.id,currency:'THB',...refs.accounts}});
  expect(new Set([f.claimant.userId,f.head.userId,f.reviewer.userId,f.manager.userId]).size).toBe(4);
  const claimInput=expenseUpdateSchema.parse({expectedVersion:1,expenseDate:'2026-09-01',merchant:'Actual zero-reference startup merchant',receiptReference:key(),businessPurpose:'First legitimate claim after included Finance setup',currency:'THB',amount:'100',tax:'7',categoryAccountId:refs.accounts.cogsAccountId}),claimKey=key(),claim=await saveExpense(f.claimant,null,claimInput,claimKey),id=String(claim.id);
  await f.attach(id);await f.action(f.claimant,id,'submit');await f.action(f.head,id,'approve');
  let row=await getExpense(f.reviewer,id);await saveExpenseAccounting(f.reviewer,id,{expectedVersion:Number(row.version),categoryAccountId:refs.accounts.cogsAccountId,tax:'7',periodId:refs.period.id,entryDate:'2026-09-02',note:'Original independent Accounts preparer verifies current receipt and native account'},key());
  await f.action(f.reviewer,id,'approve');await f.action(f.manager,id,'approve');row=await getExpense(f.manager,id);expect(row).toMatchObject({workflow_stage:'gl_posting',journal_id:null});
  const postBody={expectedVersion:Number(row.version),periodId:refs.period.id,entryDate:'2026-09-02'},postKey=key(),posted=await postExpense(f.manager,id,postBody,postKey),after=await getExpense(f.claimant,id);expect(after).toMatchObject({status:'posted',workflow_stage:'payment',outstanding_minor:10700});expect(after.journal_id).toBeTruthy();expect(posted).toMatchObject({id,status:'posted',version:postBody.expectedVersion+1,journalId:after.journal_id});
  const gl=(await db.prepare('SELECT account_id,debit_minor,credit_minor FROM finance_journal_lines WHERE journal_id=? ORDER BY account_id').bind(after.journal_id).all()).results;expect(gl).toEqual([{account_id:refs.accounts.cogsAccountId,debit_minor:10000,credit_minor:0},{account_id:refs.accounts.inputTaxAccountId,debit_minor:700,credit_minor:0},{account_id:refs.accounts.payableAccountId,debit_minor:0,credit_minor:10700}].sort((a,b)=>a.account_id.localeCompare(b.account_id)));
  expect(await db.prepare('SELECT id,status,chart_id,period_id,source_type,source_id,is_opening FROM finance_journals WHERE id=?').bind(after.journal_id).first()).toMatchObject({id:after.journal_id,status:'posted',chart_id:refs.chart.id,period_id:refs.period.id,source_type:'expense_claim',source_id:id,is_opening:0});
  const money=(await db.prepare('SELECT currency,fx_rate_id FROM money_transaction_snapshots WHERE company_id=?').bind(f.company).all<{currency:string;fx_rate_id:string}>()).results;expect(money.length).toBeGreaterThan(0);expect(money.every(row=>row.currency==='THB'&&row.fx_rate_id===refs.rate.id)).toBe(true);
  const stable=await expenseSetupImage();expect(await postExpense(f.manager,id,postBody,postKey)).toEqual(posted);expect(await refs.api.createExpensePostingChart(f.reviewer,refs.chartBody,refs.chartKey)).toEqual(refs.chart);expect(await refs.api.createExpensePostingPeriod(f.reviewer,refs.periodBody,refs.periodKey)).toEqual(refs.period);expect(await refs.api.confirmExpensePostingRate(f.manager,refs.rateBody,refs.rateKey)).toEqual(refs.rate);expect(await source.mapping.initializeExpensePostingSetup(f.reviewer,mappingBody,mappingKey)).toEqual(configured);
  expect(await refs.api.expensePostingPrerequisiteConfirmation(f.reviewer,{kind:'chart',request:refs.chartBody},refs.chartKey)).toEqual({kind:'chart',commandId:refs.chartKey,companyId:f.company,id:refs.chart.id,status:'active',version:1});expect(await refs.api.expensePostingPrerequisiteConfirmation(f.reviewer,{kind:'period',request:refs.periodBody},refs.periodKey)).toEqual({kind:'period',commandId:refs.periodKey,companyId:f.company,id:refs.period.id,status:'open',version:0});expect(await refs.api.expensePostingPrerequisiteConfirmation(f.manager,{kind:'rate',request:refs.rateBody},refs.rateKey)).toEqual({kind:'rate',commandId:refs.rateKey,companyId:f.company,id:refs.rate.id,status:'confirmed',version:null});expect(await expenseSetupImage()).toEqual(stable);
  await expect(refs.api.createExpensePostingChart(f.reviewer,{...refs.chartBody,input:{...refs.chartBody.input,name:'Different original chart request'}},refs.chartKey)).rejects.toMatchObject({code:'IDEMPOTENCY_CONFLICT'});
  const current=await source.mapping.expensePostingSetup(f.reviewer);if(!current.contextKey)throw Error('Retained Company mapping context required');await expect(source.mapping.initializeExpensePostingSetup(f.reviewer,{...mappingBody,contextKey:current.contextKey,reason:'New request cannot overwrite retained mapping'},key())).rejects.toMatchObject({code:'COMMERCE_POSTING_ALREADY_CONFIGURED'});expect(await expenseSetupImage()).toEqual(stable);
  expect((await db.prepare("SELECT package_id,state FROM company_package_grants WHERE company_id=? ORDER BY package_id").bind(f.company).all()).results).toEqual([{package_id:'people',state:'active'},{package_id:'people-expenses',state:'active'}]);
 }finally{vi.unstubAllEnvs();}
});

it('requires actual Expenses purchase even in legacy-disabled mode and rejects real Branch setup, stale context and invalid manual FX without effects',async()=>{
 try{
  const source=await expenseSetupZero(false);vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','disabled');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','disabled');
  const absent=await expenseSetupImage(),fake={expectedVersion:0 as const,contextKey:'0'.repeat(64),input:{name:'No purchase cannot authorize a Finance chart',baseCurrency:'THB',accountingBasis:'IFRS'}};expect(await source.api.expensePostingPrerequisites(f.reviewer)).toMatchObject({available:false,unavailableReason:'purchase-required'});await expect(source.api.createExpensePostingChart(f.reviewer,fake,key())).rejects.toMatchObject({status:403,code:'PACKAGE_REQUIRED'});await expect(source.mapping.initializeExpensePostingSetup(f.reviewer,{expectedVersion:0,contextKey:'0'.repeat(64),chartId:'not-a-native-reference',currency:'THB',receivableAccountId:'not-native-ar',payableAccountId:'not-native-ap',revenueAccountId:'not-native-revenue',inventoryAccountId:'not-native-inventory',clearingAccountId:'not-native-clearing',cogsAccountId:'not-native-cogs',inputTaxAccountId:'not-native-input-tax',outputTaxAccountId:'not-native-output-tax',cashAccountId:'not-native-bank',reason:'Strict purchase precedes all source adoption'},key())).rejects.toMatchObject({status:403,code:'PACKAGE_REQUIRED'});expect(await expenseSetupImage()).toEqual(absent);
  await expenseBoundaryGrant('people-expenses');vi.stubEnv('ERPX_PEOPLE_PACKAGE_ENFORCEMENT','enforced');vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');
  const old=await expenseSetupEnvelope(),chart=await source.api.createExpensePostingChart(f.reviewer,{...old,input:{name:'Actual context-changing chart',baseCurrency:'THB',accountingBasis:'IFRS'}},key()),before=await expenseSetupImage();
  await expect(source.api.createExpensePostingAccount(f.reviewer,{...old,input:{chartId:chart.id,code:'5000',name:'Stale source cannot create an account',category:'expense',currency:'THB',controlKind:'none',isControl:false}},key())).rejects.toMatchObject({status:409,code:'EXPENSE_POSTING_CONTEXT_CHANGED'});expect(await expenseSetupImage()).toEqual(before);
  const branch:typeof f.reviewer={...f.reviewer,assignmentId:await f.grant(f.reviewer,'finance_preparer','branch',f.branch),assignmentScope:'branch',branchId:f.branch,scope:{kind:'branch',holdingId:f.reviewer.holdingId,holdingSlug:f.reviewer.holdingSlug,holdingName:f.reviewer.holdingName,companyId:f.company,branchId:f.branch,branchSlug:f.branch,branchName:'Actual imported Branch'}};
  const scoped=await expenseSetupImage();expect(await source.api.expensePostingPrerequisites(branch)).toMatchObject({available:false,unavailableReason:'authority-required'});await expect(source.api.createExpensePostingPeriod(branch,{...await expenseSetupEnvelope(),input:{name:'Branch cannot initialize Company periods',startDate:'2026-01-01',endDate:'2026-12-31'}},key())).rejects.toMatchObject({status:403,code:'COMPANY_FINANCE_REQUIRED'});expect(await expenseSetupImage()).toEqual(scoped);
  const invalidRate={...await expenseSetupEnvelope(f.manager),input:{currency:'THB',effectiveDate:'2026-01-01',currencyPerUsd:'0',note:'Original owner must reject a nonpositive quote'}};await expect(source.api.confirmExpensePostingRate(f.manager,invalidRate,key())).rejects.toMatchObject({status:422,code:'INVALID_FX_RATE'});await expect(source.api.confirmExpensePostingRate(f.reviewer,{...invalidRate,...await expenseSetupEnvelope(),input:{...invalidRate.input,currencyPerUsd:'36'}},key())).rejects.toMatchObject({status:403});expect(await expenseSetupImage()).toEqual(scoped);
 }finally{vi.unstubAllEnvs();}
});

async function expenseSetupFault(command:string,sql:string,run:()=>Promise<unknown>,expected:{status:number;code:string}){
 const name='expense_setup_fault_'+key().replaceAll('-',''),before=await expenseSetupImage();
 // This is an explicitly adverse deferred source mutation, never a supported
 // owner write, business fixture, source receipt or authorization bypass.
 await db.exec(`CREATE SEQUENCE ${name};CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${name}');${sql};RETURN NEW;END $$;CREATE CONSTRAINT TRIGGER ${name} AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW WHEN(NEW.command_id='${command}') EXECUTE FUNCTION ${name}()`);
 try{await expect(run()).rejects.toMatchObject(expected);expect(await db.prepare(`SELECT is_called FROM ${name}`).first('is_called')).toBe(true);expect(await expenseSetupImage()).toEqual(before);}finally{await db.exec(`DROP TRIGGER ${name} ON outbox_events;DROP FUNCTION ${name}();DROP SEQUENCE ${name}`);}
}
it('rolls back deferred Expense prerequisite Finance IAM and purchase source faults after cleanup, then preserves the exact original retry',async()=>{
 try{
  const source=await expenseSetupZero(),chart=await source.api.createExpensePostingChart(f.reviewer,{...await expenseSetupEnvelope(),input:{name:'Actual deferred-source chart',baseCurrency:'THB',accountingBasis:'IFRS'}},key());
  for(const [index,sql,expected] of [[0,`UPDATE finance_charts SET name=name||' adverse deferred source',version=version+1 WHERE id='${chart.id}'`,{status:409,code:'EXPENSE_POSTING_SOURCE_CHANGED'}],[1,`UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.reviewer.assignmentId}'`,{status:403,code:'QUEUED_AUTHORITY_CHANGED'}],[2,'PERFORM pg_sleep(2.6)',{status:403,code:'PACKAGE_DRAINING'}]] as const){
   // The purchase probe uses a lawful audited expiry, not an unaudited grant-row mutation.
   if(index===2)await expenseBoundaryGrant('people-expenses','active',Date.now()+2500);
   const body={...await expenseSetupEnvelope(),input:{chartId:chart.id,code:`59${index}0`,name:`Actual account ${index} needs complete finality`,category:'expense' as const,currency:'THB',controlKind:'none' as const,isControl:false}},command=key();await expenseSetupFault(command,sql,()=>source.api.createExpensePostingAccount(f.reviewer,body,command),expected);if(index===2)await expenseBoundaryGrant('people-expenses','active',null);const created=await source.api.createExpensePostingAccount(f.reviewer,body,command),committed=await expenseSetupImage();expect(created).toMatchObject({companyId:f.company,chartId:chart.id,version:1});expect(await source.api.createExpensePostingAccount(f.reviewer,body,command)).toEqual(created);expect(await source.api.expensePostingPrerequisiteConfirmation(f.reviewer,{kind:'account',request:body},command)).toEqual({kind:'account',commandId:command,companyId:f.company,id:created.id,status:'active',version:1});expect(await expenseSetupImage()).toEqual(committed);
  }
 }finally{vi.unstubAllEnvs();}
});

it('preserves independently approved legacy Sales defaults including no VAT while actual Expense initial mapping rejects another valid receivable role and a late edition fault',async()=>{
 try{
  const source=await expenseSetupZero(),refs=await expenseSetupReferences(),alternate=await source.api.createExpensePostingAccount(f.reviewer,{...await expenseSetupEnvelope(),input:{chartId:refs.chart.id,code:'1110',name:'Other actual valid trade receivable',category:'asset',currency:'THB',controlKind:'receivable',isControl:true}},key()),sales=await import('../../../src/server/modules/finance/public/sales');
  // Explicit legacy parity control: these are ORIGINAL Finance prepare/review
  // services under their original disabled policy, before the strict new Expense
  // initializer. No Sales/full-Finance grant or positive SQL default is minted.
  vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','disabled');const draft=await sales.prepareSalesDefaults(f.reviewer,{chartId:refs.chart.id,currency:'THB',receivableAccountId:refs.accounts.receivableAccountId,revenueAccountId:refs.accounts.revenueAccountId,reason:'Original Finance intent predates this Expense mapping and explicitly has no VAT'},key()),approved=await sales.decideSalesDefaults(f.manager,String(draft.id),{expectedVersion:1,action:'approve',reason:'A different original Finance approver reviewed the retained no-VAT edition'},key());vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','enforced');expect(approved).toMatchObject({id:draft.id,status:'approved',version:2});
  const {commerceFinanceJoinedFacts}=await import('../../../src/server/modules/finance/public/commerce-joined-facts'),effective=()=>commerceFinanceJoinedFacts(db).readCommerceSettings1({companyId:f.company}),beforeEffective=await effective(),view=await source.mapping.expensePostingSetup(f.reviewer);expect(view).toMatchObject({expectedSalesDefaults:{id:draft.id,version:2},approvedSalesDefaults:{taxAccountId:null,chartId:refs.chart.id,currency:'THB'}});expect(beforeEffective?.output_tax_account_id).toBeNull();
  const body=await expenseSetupMappingInput(refs),wrong={...body,receivableAccountId:alternate.id},before=await expenseSetupImage();await (await import('../../../src/server/modules/finance/public/commerce-posting-setup')).validateCommercePostingFinanceReferences(db,f.reviewer,wrong);await expect(source.mapping.initializeExpensePostingSetup(f.reviewer,wrong,key())).rejects.toMatchObject({status:409,code:'EXPENSE_POSTING_DEFAULTS_CHANGED'});expect(await expenseSetupImage()).toEqual(before);
  const command=key();await expenseSetupFault(command,`UPDATE finance_sales_defaults SET review_reason=review_reason||' adverse late edition' WHERE id='${String(draft.id)}'`,()=>source.mapping.initializeExpensePostingSetup(f.reviewer,body,command),{status:409,code:'EXPENSE_POSTING_SOURCE_CHANGED'});
  const configured=await source.mapping.initializeExpensePostingSetup(f.reviewer,body,command);expect(configured).toEqual({id:f.company,status:'configured',version:1});const afterEffective=await effective();for(const field of ['chart_id','currency','receivable_account_id','revenue_account_id','output_tax_account_id'] as const)expect(afterEffective?.[field]).toEqual(beforeEffective?.[field]);expect(afterEffective?.output_tax_account_id).toBeNull();expect(await db.prepare('SELECT output_tax_account_id FROM commerce_settings WHERE company_id=?').bind(f.company).first('output_tax_account_id')).toBe(refs.accounts.outputTaxAccountId);
  const stable=await expenseSetupImage();expect(await source.mapping.initializeExpensePostingSetup(f.reviewer,body,command)).toEqual(configured);expect(await expenseSetupImage()).toEqual(stable);expect((await db.prepare('SELECT created_by,reviewed_by,version,tax_account_id FROM finance_sales_defaults WHERE id=?').bind(draft.id).first())).toEqual({created_by:f.reviewer.userId,reviewed_by:f.manager.userId,version:2,tax_account_id:null});
 }finally{vi.unstubAllEnvs();}
});

it('waits for an actual disabled ordinary Finance receipt-first writer before reading the closed Expense source and rejects its stale context without transaction retries',async()=>{
 try{
  const source=await expenseSetupZero(),chart=await source.api.createExpensePostingChart(f.reviewer,{...await expenseSetupEnvelope(),input:{name:'Actual old/new writer overlap chart',baseCurrency:'THB',accountingBasis:'IFRS'}},key()),native=await import('../../../src/server/modules/finance/public/finance'),oldBody={chartId:chart.id,code:'5800',name:'Original disabled owner account',category:'expense' as const,currency:'THB',isControl:false,controlKind:'none' as const},body={...await expenseSetupEnvelope(),input:{...oldBody,code:'5810',name:'New closed Expense account'}},writerKey=key(),newKey=key(),name='expense_setup_writer_'+key().replaceAll('-','');
  vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT','disabled');await db.exec(`CREATE SEQUENCE ${name};CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${name}');PERFORM pg_sleep(0.5);RETURN NEW;END $$;CREATE TRIGGER ${name} AFTER INSERT ON command_receipts FOR EACH ROW WHEN(NEW.command_id='${writerKey}') EXECUTE FUNCTION ${name}()`);
  try{
   const observed=await measureReads(async()=>{let done=false;const writing=native.createAccount(f.reviewer,oldBody,writerKey).finally(()=>{done=true;}),oldOutcome=Promise.allSettled([writing]);let atReceipt=false;for(let attempt=0;attempt<100;attempt++){atReceipt=!!await db.prepare(`SELECT is_called FROM ${name}`).first('is_called');if(atReceipt||done)break;await new Promise(resolve=>setTimeout(resolve,5));}if(!atReceipt){const [result]=await oldOutcome;if(result.status==='rejected')throw result.reason;throw Error('Actual original writer did not reach its native receipt barrier');}expect(done).toBe(false);
    const current=Promise.allSettled([source.api.createExpensePostingAccount(f.reviewer,body,newKey)]);let waiting=false;for(let attempt=0;attempt<100&&!waiting&&!done;attempt++){waiting=!!await db.prepare("SELECT 1 FROM pg_locks WHERE database=(SELECT oid FROM pg_database WHERE datname=current_database()) AND relation='command_receipts'::regclass AND mode='ShareRowExclusiveLock' AND NOT granted LIMIT 1").first();if(!waiting)await new Promise(resolve=>setTimeout(resolve,5));}const [oldResult,newResult]=await Promise.all([oldOutcome,current]);expect(waiting).toBe(true);return {old:oldResult[0],closed:newResult[0]};});
   expect(observed.result.old.status).toBe('fulfilled');expect(observed.result.closed.status).toBe('rejected');if(observed.result.closed.status!=='rejected')throw Error('Original writer must change the captured Expense reference context');expect(observed.result.closed.reason).toMatchObject({status:409,code:'EXPENSE_POSTING_CONTEXT_CHANGED'});expect({deadlockFailures:observed.metrics.deadlockFailures??0,serializationFailures:observed.metrics.serializationFailures??0,transactionRetries:observed.metrics.transactionRetries??0}).toEqual({deadlockFailures:0,serializationFailures:0,transactionRetries:0});
   expect(await db.prepare('SELECT 1 FROM command_receipts WHERE command_id=?').bind(newKey).first()).toBeNull();expect(await db.prepare('SELECT count(*) n FROM finance_accounts WHERE company_id=?').bind(f.company).first('n')).toBe(1);expect(await db.prepare('SELECT count(*) n FROM finance_journals WHERE company_id=?').bind(f.company).first('n')).toBe(0);expect(await db.prepare('SELECT count(*) n FROM money_transaction_snapshots WHERE company_id=?').bind(f.company).first('n')).toBe(0);
   const retried=await source.api.createExpensePostingAccount(f.reviewer,{...body,...await expenseSetupEnvelope()},newKey),stable=await expenseSetupImage();expect(retried).toMatchObject({companyId:f.company,chartId:chart.id,code:'5810',version:1});expect(await native.createAccount(f.reviewer,oldBody,writerKey)).toEqual(observed.result.old.status==='fulfilled'?observed.result.old.value:undefined);expect(await expenseSetupImage()).toEqual(stable);
  }finally{await db.exec(`DROP TRIGGER ${name} ON command_receipts;DROP FUNCTION ${name}();DROP SEQUENCE ${name}`);}
 }finally{vi.unstubAllEnvs();}
});

// Additive ADR-038 proof. Starts from the same real ZERO-Finance Company
// fixture as setup, but intentionally leaves its manual FX quotes empty.
async function expenseMoneyQuoteFreeReferences() {
  const api = await import('../../../src/server/modules/finance/public/expense-posting-prerequisites');
  const chart = await api.createExpensePostingChart(f.reviewer, {
    ...await expenseSetupEnvelope(),
    input: { name: 'Expense Money evidence functional chart', baseCurrency: 'THB', accountingBasis: 'IFRS' },
  }, key());
  const accounts = { receivableAccountId: '', payableAccountId: '', revenueAccountId: '', inventoryAccountId: '', clearingAccountId: '', cogsAccountId: '', inputTaxAccountId: '', outputTaxAccountId: '', cashAccountId: '' };
  for (const [field, code, category, controlKind] of [
    ['receivableAccountId', '1100', 'asset', 'receivable'], ['payableAccountId', '2000', 'liability', 'payable'],
    ['revenueAccountId', '4000', 'revenue', 'none'], ['inventoryAccountId', '1200', 'asset', 'none'],
    ['clearingAccountId', '2100', 'liability', 'none'], ['cogsAccountId', '5000', 'expense', 'none'],
    ['inputTaxAccountId', '1300', 'asset', 'none'], ['outputTaxAccountId', '2200', 'liability', 'none'],
    ['cashAccountId', '1000', 'asset', 'bank'],
  ] as const) {
    const account = await api.createExpensePostingAccount(f.reviewer, {
      ...await expenseSetupEnvelope(),
      input: { chartId: chart.id, code, name: `Money source ${field}`, category, currency: 'THB', controlKind, isControl: controlKind !== 'none' },
    }, key());
    accounts[field] = account.id;
  }
  const period = await api.createExpensePostingPeriod(f.reviewer, {
    ...await expenseSetupEnvelope(), input: { name: 'Expense Money evidence business period', startDate: '2026-01-01', endDate: '2026-12-31' },
  }, key());
  expect(await expenseSetupCounts()).toEqual({ charts: 1, accounts: 9, periods: 1, rates: 0, mappings: 0, journals: 0, money: 0 });
  return { api, chart, accounts, period };
}
async function expenseMoneyImage() {
  return { ...await expenseSetupImage(), parties: (await db.prepare('SELECT to_jsonb(p)::text image FROM finance_parties p WHERE p.company_id=? ORDER BY image').bind(f.company).all()).results };
}
async function expenseCanonicalMoney() {
  return (await db.prepare(`SELECT source_type,source_id,amount_kind,transaction_date::text,currency,currency_scale,
    amount_units::text,usd_units::text,fx_rate_id,used_latest_fallback,
    original_source_type,original_source_id,original_amount_kind
    FROM money_transaction_snapshots WHERE company_id=? ORDER BY source_type,source_id,amount_kind`).bind(f.company).all()).results;
}
async function expenseMoneyAuthorityFault(run: () => Promise<unknown>) {
  const name = 'expense_money_fault_' + key().replaceAll('-', ''), before = await expenseMoneyImage();
  // Explicitly adverse native mutation AFTER a real canonical Money INSERT,
  // never a supported authorization command or positive business fixture.
  await db.exec(`CREATE SEQUENCE ${name};CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$
    BEGIN PERFORM nextval('${name}');UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.manager.assignmentId}';RETURN NEW;END $$;
    CREATE CONSTRAINT TRIGGER ${name} AFTER INSERT ON money_transaction_snapshots DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW WHEN(NEW.company_id='${f.company}') EXECUTE FUNCTION ${name}()`);
  try {
    await expect(run()).rejects.toMatchObject({ status: 403, code: 'EXPENSE_ACCESS_CHANGED' });
    expect(await db.prepare(`SELECT is_called FROM ${name}`).first('is_called')).toBe(true);
    expect(await expenseMoneyImage()).toEqual(before);
  } finally { await db.exec(`DROP TRIGGER ${name} ON money_transaction_snapshots;DROP FUNCTION ${name}();DROP SEQUENCE ${name}`); }
}
it('captures new Expense journal and invoice Money atomically, retains the original quote across replay and reverses its exact evidence without rerating', async () => {
  try {
    const source = await expenseSetupZero(), refs = await expenseMoneyQuoteFreeReferences();
    const view = await source.mapping.expensePostingSetup(f.reviewer);
    if (!view.contextKey) throw Error('Actual zero-history Expense mapping context required');
    const mapping = source.mapping.expensePostingSetupSchema.parse({ expectedVersion: 0, contextKey: view.contextKey, chartId: refs.chart.id, currency: 'THB', ...refs.accounts, expectedSalesDefaults: view.expectedSalesDefaults, reason: 'Current Company Finance prepares real native roles before the first Money capture' });
    await source.mapping.initializeExpensePostingSetup(f.reviewer, mapping, key());
    const claim = await saveExpense(f.claimant, null, expenseUpdateSchema.parse({
      expectedVersion: 1, expenseDate: '2026-09-01', merchant: 'Actual original-currency receipt', receiptReference: key(),
      businessPurpose: 'Reviewed claim must retain its native amount and dated USD quote', currency: 'THB', amount: '100', tax: '7', categoryAccountId: refs.accounts.cogsAccountId,
    }), key()), id = String(claim.id);
    await f.attach(id);await f.action(f.claimant, id, 'submit');await f.action(f.head, id, 'approve');
    let row = await getExpense(f.reviewer, id);
    await saveExpenseAccounting(f.reviewer, id, { expectedVersion: Number(row.version), categoryAccountId: refs.accounts.cogsAccountId, tax: '7', periodId: refs.period.id, entryDate: '2026-09-02', note: 'Independent Accounts verifies the original receipt, actual role and business date' }, key());
    await f.action(f.reviewer, id, 'approve');await f.action(f.manager, id, 'approve');row = await getExpense(f.manager, id);
    expect(row).toMatchObject({ status: 'approved', workflow_stage: 'gl_posting', journal_id: null, invoice_id: null });
    const postBody = { expectedVersion: Number(row.version), periodId: refs.period.id, entryDate: '2026-09-02' }, postKey = key(), noQuote = await expenseMoneyImage();
    await expect(postExpense(f.manager, id, postBody, postKey)).rejects.toMatchObject({ status: 422, code: 'MANUAL_RATE_REQUIRED' });
    expect(await expenseMoneyImage()).toEqual(noQuote);
    expect(await db.prepare('SELECT 1 FROM command_receipts WHERE command_id=?').bind(postKey).first()).toBeNull();
    expect(await expenseSetupCounts()).toMatchObject({ journals: 0, money: 0, rates: 0 });

    const rate = await refs.api.confirmExpensePostingRate(f.manager, { ...await expenseSetupEnvelope(f.manager), input: { currency: 'THB', effectiveDate: '2026-01-01', currencyPerUsd: '36', note: 'Actual fictional dated quote for this first posted claim' } }, key());
    await expenseMoneyAuthorityFault(() => postExpense(f.manager, id, postBody, postKey));
    const posted = await postExpense(f.manager, id, postBody, postKey), current = await getExpense(f.claimant, id);
    expect(posted).toMatchObject({ id, status: 'posted', version: postBody.expectedVersion + 1, journalId: current.journal_id, invoiceId: current.invoice_id });
    expect(current).toMatchObject({ status: 'posted', workflow_stage: 'payment', outstanding_minor: 10700 });
    const common = { transaction_date: '2026-09-02', currency: 'THB', currency_scale: 2, amount_units: '10700', usd_units: '297', fx_rate_id: rate.id, used_latest_fallback: false, original_source_type: null, original_source_id: null, original_amount_kind: null };
    const originalMoney = await expenseCanonicalMoney();
    expect(originalMoney).toEqual([
      { ...common, source_type: 'finance_invoice', source_id: current.invoice_id, amount_kind: 'total' },
      { ...common, source_type: 'finance_journal', source_id: current.journal_id, amount_kind: 'debit_total' },
    ]);
    const originalGl = (await db.prepare('SELECT to_jsonb(l)::text image FROM finance_journal_lines l WHERE l.journal_id=? ORDER BY l.ordinal').bind(current.journal_id).all()).results;
    const laterRate = await refs.api.confirmExpensePostingRate(f.manager, { ...await expenseSetupEnvelope(f.manager), input: { currency: 'THB', effectiveDate: '2026-01-01', currencyPerUsd: '40', note: 'A later lawful quote revision cannot rerate this posted receipt' } }, key());
    expect(laterRate.id).not.toBe(rate.id);
    const afterRevision = await expenseMoneyImage();
    expect(await postExpense(f.manager, id, postBody, postKey)).toEqual(posted);
    expect(await expenseMoneyImage()).toEqual(afterRevision);
    expect(await expenseCanonicalMoney()).toEqual(originalMoney);

    const { reverseExpense } = await import('../../../src/server/modules/people_payroll/public/expenses');
    const reversalBody = { expectedVersion: Number(current.version), periodId: refs.period.id, entryDate: '2026-09-03', reason: 'Original unpaid liability is independently corrected without changing its captured quote' }, reversalKey = key();
    await expenseMoneyAuthorityFault(() => reverseExpense(f.manager, id, reversalBody, reversalKey));
    const reversed = await reverseExpense(f.manager, id, reversalBody, reversalKey);
    expect(reversed).toMatchObject({ id, status: 'cancelled', version: reversalBody.expectedVersion + 1 });
    expect(await db.prepare('SELECT status,outstanding_minor FROM finance_invoices WHERE id=?').bind(current.invoice_id).first()).toEqual({ status: 'voided', outstanding_minor: 0 });
    expect(await db.prepare('SELECT status FROM finance_journals WHERE id=?').bind(current.journal_id).first('status')).toBe('reversed');
    expect(await db.prepare('SELECT source_type,source_id,entry_date,currency,fx_rate_micros FROM finance_journals WHERE id=?').bind(reversed.reversalJournalId).first()).toEqual({ source_type: 'expense_reversal', source_id: id, entry_date: '2026-09-03', currency: 'THB', fx_rate_micros: 1000000 });
    expect((await db.prepare('SELECT account_id,debit_minor,credit_minor FROM finance_journal_lines WHERE journal_id=? ORDER BY account_id').bind(reversed.reversalJournalId).all()).results).toEqual([
      { account_id: refs.accounts.cogsAccountId, debit_minor: 0, credit_minor: 10000 },
      { account_id: refs.accounts.inputTaxAccountId, debit_minor: 0, credit_minor: 700 },
      { account_id: refs.accounts.payableAccountId, debit_minor: 10700, credit_minor: 0 },
    ].sort((a, b) => a.account_id.localeCompare(b.account_id)));
    const reversalMoney = (await expenseCanonicalMoney()).filter(item => item.source_id === reversed.reversalJournalId);
    expect(reversalMoney).toEqual([{ ...common, source_type: 'finance_journal', source_id: reversed.reversalJournalId, amount_kind: 'debit_total', transaction_date: '2026-09-03', amount_units: '-10700', usd_units: '-297', original_source_type: 'finance_journal', original_source_id: current.journal_id, original_amount_kind: 'debit_total' }]);
    expect((await expenseCanonicalMoney()).filter(item => item.source_id !== reversed.reversalJournalId)).toEqual(originalMoney);
    expect((await db.prepare('SELECT to_jsonb(l)::text image FROM finance_journal_lines l WHERE l.journal_id=? ORDER BY l.ordinal').bind(current.journal_id).all()).results).toEqual(originalGl);
    const reversedImage = await expenseMoneyImage();
    expect(await reverseExpense(f.manager, id, reversalBody, reversalKey)).toEqual(reversed);
    expect(await postExpense(f.manager, id, postBody, postKey)).toEqual(posted);
    expect(await expenseMoneyImage()).toEqual(reversedImage);
    expect((await db.prepare('SELECT package_id,state FROM company_package_grants WHERE company_id=? ORDER BY package_id').bind(f.company).all()).results).toEqual([{ package_id: 'people', state: 'active' }, { package_id: 'people-expenses', state: 'active' }]);
  } finally { vi.unstubAllEnvs(); }
});
