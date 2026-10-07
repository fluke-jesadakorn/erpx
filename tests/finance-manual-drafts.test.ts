import { beforeAll, beforeEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "./runtime-env";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "./helpers/expense-fixture";
import {
  createJournal,
  createJournalSchema,
  postJournal,
  trialBalance,
} from "../src/server/finance";
import {
  editManualDraft,
  manualDraftEditSchema,
} from "../src/server/finance-manual-drafts";
import { createFinanceDimension } from "../src/server/finance-books";
vi.mock("../src/server/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
let f: Awaited<ReturnType<typeof expenseFixture>>;
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
});
beforeEach(async () => {
  f = await expenseFixture();
  for (const [a, r] of [
    [f.owner, "finance_preparer"],
    [f.manager, "finance_approver"],
  ] as const) {
    a.role = r;
    a.assignmentId = await f.grant(a, r);
  }
});
function input() {
  return {
    chartId: f.chart,
    periodId: f.period,
    entryDate: "2026-09-30",
    currency: "THB",
    memo: "Original manual draft",
    lines: [
      { accountId: f.accounts.expense, debit: "100.01", credit: "0" },
      { accountId: f.accounts.payable, debit: "0", credit: "100.01" },
    ],
  };
}
it("fills a draft created before required dimensions, retains its original amounts and independently posts the edited values", async () => {
  const old = await createJournal(
    f.owner,
    createJournalSchema.parse(input()),
    key(),
  );
  await createFinanceDimension(
    f.owner,
    {
      code: "project",
      name: "Project",
      required: true,
      values: [{ code: "P1", name: "Fictional project" }],
    },
    key(),
  );
  await expect(
    postJournal(f.manager, String(old.id), key(), { expectedVersion: 1 }),
  ).rejects.toMatchObject({ code: "DIMENSION_VALUE_REQUIRED" });
  const request = manualDraftEditSchema.parse({
      ...input(),
      expectedVersion: 1,
      memo: "Reviewed correction of the draft amount",
      reason:
        "Corrected amount and selected the required project before approval",
      lines: [
        {
          accountId: f.accounts.expense,
          debit: "80.01",
          credit: "0",
          dimensions: { project: "P1" },
        },
        {
          accountId: f.accounts.payable,
          debit: "0",
          credit: "80.01",
          dimensions: { project: "P1" },
        },
      ],
    }),
    command = key(),
    edited = await editManualDraft(f.owner, String(old.id), request, command);
  expect(
    await editManualDraft(f.owner, String(old.id), request, command),
  ).toEqual(edited);
  const history = (
    await db
      .prepare(
        "SELECT journal_version,snapshot_json FROM finance_manual_draft_editions WHERE journal_id=? ORDER BY journal_version",
      )
      .bind(old.id)
      .all<{ journal_version: number; snapshot_json: string }>()
  ).results;
  expect(history.map((h) => h.journal_version)).toEqual([1, 2]);
  expect(JSON.parse(history[0].snapshot_json).lines[0]).toMatchObject({
    debit_minor: 10001,
    dimension_json: "{}",
  });
  expect(JSON.parse(history[1].snapshot_json).lines[0]).toMatchObject({
    debit_minor: 8001,
    dimension_json: '{"project":"P1"}',
  });
  await expect(
    postJournal(f.manager, String(old.id), key(), { expectedVersion: 1 }),
  ).rejects.toMatchObject({ status: 409 });
  await postJournal(f.manager, String(old.id), key(), { expectedVersion: 2 });
  expect(
    (await trialBalance(f.owner, undefined, f.chart)).find(
      (a) => a.id === f.accounts.expense,
    )?.balance,
  ).toBe("80.01");
  await expect(
    editManualDraft(
      f.owner,
      String(old.id),
      { ...request, expectedVersion: 3 },
      key(),
    ),
  ).rejects.toMatchObject({ code: "MANUAL_DRAFT_CHANGED" });
  await expect(
    db
      .prepare(
        "UPDATE finance_manual_draft_editions SET reason='changed' WHERE journal_id=?",
      )
      .bind(old.id)
      .run(),
  ).rejects.toThrow("FINANCIAL_EVENT_IMMUTABLE");
});
it("refuses cross-Company and unbalanced edits and accepts only one concurrent draft revision", async () => {
  const old = await createJournal(
      f.owner,
      createJournalSchema.parse(input()),
      key(),
    ),
    request = manualDraftEditSchema.parse({
      ...input(),
      expectedVersion: 1,
      reason: "Review the complete manual draft before posting",
    });
  await expect(
    editManualDraft(
      { ...f.owner, companyId: "other" },
      String(old.id),
      request,
      key(),
    ),
  ).rejects.toMatchObject({ code: "JOURNAL_NOT_FOUND" });
  await expect(
    editManualDraft(
      f.owner,
      String(old.id),
      {
        ...request,
        lines: [
          { accountId: f.accounts.expense, debit: "100.01", credit: "0" },
          { accountId: f.accounts.payable, debit: "0", credit: "100" },
        ],
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "UNBALANCED_JOURNAL" });
  expect(
    await db
      .prepare(
        "SELECT count(*) n FROM finance_manual_draft_editions WHERE journal_id=?",
      )
      .bind(old.id)
      .first("n"),
  ).toBe(0);
  const results = await Promise.allSettled([
    editManualDraft(f.owner, String(old.id), request, key()),
    editManualDraft(f.owner, String(old.id), request, key()),
  ]);
  expect(results.filter((r) => r.status === "fulfilled")).toHaveLength(1);
  expect(
    await db
      .prepare("SELECT version FROM finance_journals WHERE id=?")
      .bind(old.id)
      .first("version"),
  ).toBe(2);
  expect(
    await db
      .prepare(
        "SELECT count(*) n FROM finance_journal_lines WHERE journal_id=?",
      )
      .bind(old.id)
      .first("n"),
  ).toBe(2);
});

it("rolls back original and edited immutable editions with line replacement after final outbox failure, then retries once",async()=>{
 const old=await createJournal(f.owner,createJournalSchema.parse(input()),key()),id=String(old.id),command=key(),request=manualDraftEditSchema.parse({...input(),expectedVersion:1,memo:"Reviewed corrected draft",reason:"Independent verification before posting",lines:[{accountId:f.accounts.expense,debit:"80.01",credit:"0"},{accountId:f.accounts.payable,debit:"0",credit:"80.01"}]}),before=await db.prepare("SELECT * FROM finance_journals WHERE id=?").bind(id).first(),lines=(await db.prepare("SELECT * FROM finance_journal_lines WHERE journal_id=? ORDER BY ordinal,id").bind(id).all()).results,fault=`manual_draft_fault_${crypto.randomUUID().replaceAll("-","")}`;
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'MANUAL_DRAFT_LATE_OUTBOX' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN (NEW.event_type='finance.manual-draft.edit') EXECUTE FUNCTION ${fault}();`);
 try{
  await expect(editManualDraft(f.owner,id,request,command)).rejects.toThrow();
  expect(await db.prepare("SELECT * FROM finance_journals WHERE id=?").bind(id).first()).toEqual(before);
  expect((await db.prepare("SELECT * FROM finance_journal_lines WHERE journal_id=? ORDER BY ordinal,id").bind(id).all()).results).toEqual(lines);
  expect(Number(await db.prepare("SELECT count(*) n FROM finance_manual_draft_editions WHERE journal_id=?").bind(id).first("n"))).toBe(0);
  for(const table of ["command_receipts","audit_events","outbox_events"])expect(Number(await db.prepare(`SELECT COUNT(*) n FROM ${table} WHERE command_id=?`).bind(command).first("n"))).toBe(0);
 }finally{await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}();`);}
 const result=await editManualDraft(f.owner,id,request,command);expect(await editManualDraft(f.owner,id,request,command)).toEqual(result);expect(result).toMatchObject({id,status:"draft",version:2});
 expect(Number(await db.prepare("SELECT count(*) n FROM finance_manual_draft_editions WHERE journal_id=?").bind(id).first("n"))).toBe(2);
 for(const table of ["command_receipts","audit_events","outbox_events"])expect(Number(await db.prepare(`SELECT COUNT(*) n FROM ${table} WHERE command_id=?`).bind(command).first("n"))).toBe(1);
 expect(Number(await db.prepare("SELECT debit_minor FROM finance_journal_lines WHERE journal_id=? AND account_id=?").bind(id,f.accounts.expense).first("debit_minor"))).toBe(8001);
});
