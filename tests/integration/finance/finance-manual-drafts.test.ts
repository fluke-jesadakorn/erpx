import { packageGrantStore } from "@erpx/server-commercial/public/package-grants";
import { beforeAll, beforeEach, afterEach, expect, it, vi } from "vitest";
import { applyMigrations, env } from "../../support/runtime-env";
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "../../support/helpers/expense-fixture";
import {
  createJournal,
  createJournalSchema,
  postJournal,
  trialBalance,
} from "@erpx/server-finance/public/finance";
import {
  editManualDraft,
  manualDraftEditSchema,
} from "@erpx/server-finance/public/finance-manual-drafts";
import { createFinanceDimension } from "@erpx/server-finance/public/finance-books";
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
afterEach(() => vi.unstubAllEnvs());
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
  await packageGrantStore(db,{authorize:async()=>({userId:"isolated-manual-finance-fixture"}),obligations:async()=>{throw Error("No manual Finance withdrawal witness");}}).apply({expectedVersion:0,idempotencyKey:key(),grant:{companyId:f.company,packageId:"finance",packageVersion:1,state:"active",startsAt:0,endsAt:null,version:1,reason:"Native manual Finance purchase",evidence:"test:finance-manual-purchase"}});
  vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");

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

// Additional native evidence for the original generic manual journal owner.
// Chart, Org and worker references are the existing declared fixture; every
// journal, post, rate revision and reversal below uses its normal owner command.
async function journalMoneyFinancePurchase() {
  await packageGrantStore(db, { authorize: async () => ({ userId: 'isolated-native-journal-money-review' }), obligations: async () => { throw Error('No generic journal withdrawal witness'); } }).apply({
    expectedVersion: 0, idempotencyKey: key(), grant: { companyId: f.company, packageId: 'finance', packageVersion: 1, state: 'active', startsAt: 0, endsAt: null, version: 1, reason: 'Actual original Finance owner purchase for native manual journal Money', evidence: 'test:generic-journal-money' },
  });
  vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT', 'enforced');
}
async function journalMoneyImage() {
  return {
    journals: (await db.prepare('SELECT to_jsonb(j)::text image FROM finance_journals j WHERE j.company_id=? ORDER BY j.id').bind(f.company).all()).results,
    lines: (await db.prepare('SELECT to_jsonb(l)::text image FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.company_id=? ORDER BY l.journal_id,l.ordinal').bind(f.company).all()).results,
    invoices: (await db.prepare('SELECT to_jsonb(i)::text image FROM finance_invoices i WHERE i.company_id=? ORDER BY i.id').bind(f.company).all()).results,
    allocations: (await db.prepare('SELECT to_jsonb(a)::text image FROM finance_allocations a JOIN finance_invoices i ON i.id=a.invoice_id WHERE i.company_id=? ORDER BY a.id').bind(f.company).all()).results,
    payments: (await db.prepare('SELECT to_jsonb(p)::text image FROM finance_payments p WHERE p.company_id=? ORDER BY p.id').bind(f.company).all()).results,
    money: (await db.prepare('SELECT to_jsonb(m)::text image FROM money_transaction_snapshots m WHERE m.company_id=? ORDER BY source_type,source_id,amount_kind').bind(f.company).all()).results,
    events: (await db.prepare('SELECT to_jsonb(e)::text image FROM finance_financial_events e WHERE e.company_id=? ORDER BY e.id').bind(f.company).all()).results,
    books: (await db.prepare('SELECT to_jsonb(b)::text image FROM finance_book_postings b JOIN finance_journals j ON j.id=b.primary_journal_id WHERE j.company_id=? ORDER BY b.primary_journal_id,b.rule_id').bind(f.company).all()).results,
    assignments: (await db.prepare('SELECT to_jsonb(a)::text image FROM assignments a WHERE a.company_id=? ORDER BY a.id').bind(f.company).all()).results,
    receipts: (await db.prepare("SELECT to_jsonb(r)::text image FROM command_receipts r WHERE r.authority_json::jsonb->>'companyId'=? ORDER BY r.command_id").bind(f.company).all()).results,
    audit: (await db.prepare("SELECT to_jsonb(a)::text image FROM audit_events a WHERE a.authority_json::jsonb->>'companyId'=? ORDER BY a.id").bind(f.company).all()).results,
    outbox: (await db.prepare("SELECT to_jsonb(o)::text image FROM outbox_events o JOIN command_receipts r ON r.command_id=o.command_id WHERE r.authority_json::jsonb->>'companyId'=? ORDER BY o.id").bind(f.company).all()).results,
  };
}
async function journalMoneyRows() {
  return (await db.prepare(`SELECT source_type,source_id,amount_kind,transaction_date::text,currency,currency_scale,amount_units::text,usd_units::text,fx_rate_id,used_latest_fallback,
    original_source_type,original_source_id,original_amount_kind FROM money_transaction_snapshots WHERE company_id=? AND source_type='finance_journal' ORDER BY source_id,amount_kind`).bind(f.company).all()).results;
}
it('retains exact manual journal Money through the original reversal and lawful rereversal after a new quote, with no duplicate capture on exact replay', async () => {
  f = await expenseFixture(true, false);
  for (const [actor, role] of [[f.owner, 'finance_preparer'], [f.manager, 'finance_approver']] as const) { actor.role = role;actor.assignmentId = await f.grant(actor, role); }
  await journalMoneyFinancePurchase();
  const { confirmManualRate } = await import('@erpx/server-finance/public/company-money');
  const { reverseJournal } = await import('@erpx/server-finance/public/finance');
  expect(await db.prepare('SELECT COUNT(*) n FROM company_fx_rates WHERE company_id=?').bind(f.company).first('n')).toBe(0);
  const journal = await createJournal(f.owner, createJournalSchema.parse(input()), key()), id = String(journal.id), postKey = key(), postInput = { expectedVersion: 1 }, noQuote = await journalMoneyImage();
  await expect(postJournal(f.manager, id, postKey, postInput)).rejects.toMatchObject({ status: 422, code: 'MANUAL_RATE_REQUIRED' });
  expect(await journalMoneyImage()).toEqual(noQuote);
  expect(await db.prepare('SELECT 1 FROM command_receipts WHERE command_id=?').bind(postKey).first()).toBeNull();
  const rate = await confirmManualRate(f.manager, { currency: 'THB', effectiveDate: '2026-01-01', currencyPerUsd: '36', note: 'Fictional manually confirmed quote for the original journal' }, key());
  const posted = await postJournal(f.manager, id, postKey, postInput), original = await journalMoneyRows();
  expect(original).toEqual([{ source_type: 'finance_journal', source_id: id, amount_kind: 'debit_total', transaction_date: '2026-09-30', currency: 'THB', currency_scale: 2, amount_units: '10001', usd_units: '278', fx_rate_id: rate.id, used_latest_fallback: false, original_source_type: null, original_source_id: null, original_amount_kind: null }]);
  const later = await confirmManualRate(f.manager, { currency: 'THB', effectiveDate: '2026-10-01', currencyPerUsd: '40', note: 'Later current quote cannot replace original journal rounding' }, key());expect(later.id).not.toBe(rate.id);
  const firstKey = key(), firstBody = { periodId: f.period, entryDate: '2026-10-01', memo: 'Original generic manual reversal' }, first = await reverseJournal(f.manager, id, firstKey, firstBody), firstId = String(first.reversalJournalId);
  expect(first).toEqual({ reversalJournalId: `reversal-${firstKey}`, reversedJournalId: id });
  const secondKey = key(), secondBody = { periodId: f.period, entryDate: '2026-10-02', memo: 'Lawful reversal of the original manual reversal' }, second = await reverseJournal(f.manager, firstId, secondKey, secondBody), secondId = String(second.reversalJournalId);
  expect(second).toEqual({ reversalJournalId: `reversal-${secondKey}`, reversedJournalId: firstId });
  const money = await journalMoneyRows(), snapshot = (sourceId: string) => money.find(row => row.source_id === sourceId);
  expect(snapshot(id)).toEqual(original[0]);
  expect(snapshot(firstId)).toEqual({ ...original[0], source_id: firstId, transaction_date: '2026-10-01', amount_units: '-10001', usd_units: '-278', original_source_type: 'finance_journal', original_source_id: id, original_amount_kind: 'debit_total' });
  expect(snapshot(secondId)).toEqual({ ...original[0], source_id: secondId, transaction_date: '2026-10-02' });
  expect(money).toHaveLength(3);
  expect((await db.prepare('SELECT id,status,source_type,source_id,currency,fx_rate_micros FROM finance_journals WHERE id IN (?,?,?) ORDER BY id').bind(id, firstId, secondId).all()).results).toEqual([
    { id, status: 'reversed', source_type: 'manual', source_id: null, currency: 'THB', fx_rate_micros: 1000000 },
    { id: firstId, status: 'reversed', source_type: 'reversal', source_id: id, currency: 'THB', fx_rate_micros: 1000000 },
    { id: secondId, status: 'posted', source_type: 'reversal', source_id: firstId, currency: 'THB', fx_rate_micros: 1000000 },
  ].sort((a, b) => a.id.localeCompare(b.id)));
  const lines = async (journalId: string) => (await db.prepare('SELECT account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor FROM finance_journal_lines WHERE journal_id=? ORDER BY ordinal').bind(journalId).all()).results;
  expect(await lines(firstId)).toEqual([{ account_id: f.accounts.expense, debit_minor: 0, credit_minor: 10001, base_debit_minor: 0, base_credit_minor: 10001 }, { account_id: f.accounts.payable, debit_minor: 10001, credit_minor: 0, base_debit_minor: 10001, base_credit_minor: 0 }]);
  expect(await lines(secondId)).toEqual(await lines(id));
  const stable = await journalMoneyImage();
  expect(await postJournal(f.manager, id, postKey, postInput)).toEqual(posted);
  expect(await reverseJournal(f.manager, id, firstKey, firstBody)).toEqual(first);
  expect(await reverseJournal(f.manager, firstId, secondKey, secondBody)).toEqual(second);
  expect(await journalMoneyImage()).toEqual(stable);
  await expect(reverseJournal(f.manager, id, firstKey, { ...firstBody, memo: 'Different body cannot adopt the original Money reversal' })).rejects.toMatchObject({ code: 'IDEMPOTENCY_CONFLICT' });
  expect(await journalMoneyImage()).toEqual(stable);
});

async function journalMoneyFault(event: 'money' | 'cleanup', company: string, command: string, sql: string, run: () => Promise<unknown>, expected: { status: number; code: string }) {
  const name = 'journal_money_fault_' + key().replaceAll('-', ''), before = await journalMoneyImage();
  // Labelled adverse diagnostic source mutation, never a positive journal,
  // commercial grant, Money fixture or supported permission transition.
  // 1039 keeps NEW assertion rows in a transaction-local GUC, not the baseline
  // table. Preserve its normal view trigger and schedule this cleanup fault on
  // a separate diagnostic relation so it runs at the owned final deferred flush.
  const table = event === 'money' ? 'money_transaction_snapshots' : `${name}_pending`;
  if (event === 'cleanup') await db.exec(`CREATE TABLE ${table}(command_id text PRIMARY KEY,step text NOT NULL,ok bigint NOT NULL)`);
  await db.exec(`CREATE SEQUENCE ${name};CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NOT (SELECT is_called FROM ${name}) THEN
    ${event === 'cleanup' ? `IF (COALESCE(NULLIF(current_setting('erpx.command_assertions',true),''),'{}')::jsonb #>> ARRAY[NEW.command_id,NEW.step]) IS NOT NULL THEN RAISE EXCEPTION 'Native purchase cleanup did not remove its transaction-local guard';END IF;` : ''}
    PERFORM nextval('${name}');${sql};END IF;RETURN NEW;END $$;
    CREATE CONSTRAINT TRIGGER ${name} AFTER INSERT ON ${table} DEFERRABLE INITIALLY DEFERRED FOR EACH ROW ${event === 'money' ? `WHEN(NEW.company_id='${company}' AND NEW.source_type='finance_journal' AND NEW.source_id='reversal-${command}')` : ''} EXECUTE FUNCTION ${name}()`);
  if (event === 'cleanup') await db.exec(`CREATE FUNCTION ${name}_schedule() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
    IF OLD.step='commercial-purchase-after' AND OLD.command_id LIKE 'finance-purchase:%' AND EXISTS(SELECT 1 FROM command_receipts WHERE command_id='${command}' AND authority_json::jsonb->>'companyId'='${company}') THEN
      IF (COALESCE(NULLIF(current_setting('erpx.command_assertions',true),''),'{}')::jsonb #>> ARRAY[OLD.command_id,OLD.step]) IS NOT NULL THEN RAISE EXCEPTION 'Diagnostic cleanup must follow the original view handler';END IF;
      -- The inner Finance boundary has flushed ALL constraints already. Re-defer
      -- ONLY this diagnostic event; the new outer owned final flush must run it.
      EXECUTE 'SET CONSTRAINTS ${name} DEFERRED';
      INSERT INTO ${table}(command_id,step,ok) VALUES(OLD.command_id,OLD.step,OLD.ok);
    END IF;RETURN OLD;END $$;
    CREATE TRIGGER zz_${name} INSTEAD OF DELETE ON command_assertions FOR EACH ROW EXECUTE FUNCTION ${name}_schedule()`);
  try { await expect(run()).rejects.toMatchObject(expected);expect(await db.prepare(`SELECT is_called FROM ${name}`).first('is_called')).toBe(true);expect(await journalMoneyImage()).toEqual(before); }
  finally {
    if (event === 'cleanup') await db.exec(`DROP TRIGGER zz_${name} ON command_assertions;DROP FUNCTION ${name}_schedule()`);
    await db.exec(`DROP TRIGGER ${name} ON ${table};DROP FUNCTION ${name}();DROP SEQUENCE ${name}`);
    if (event === 'cleanup') await db.exec(`DROP TABLE ${table}`);
  }
}
it('rolls back generic reversal Money with late original authority and purchase-cleanup source changes, then retains the first request on retry and replay', async () => {
  await journalMoneyFinancePurchase();
  const { reverseJournal } = await import('@erpx/server-finance/public/finance');
  const journal = await createJournal(f.owner, createJournalSchema.parse(input()), key()), id = String(journal.id);await postJournal(f.manager, id, key(), { expectedVersion: 1 });
  const command = key(), body = { periodId: f.period, entryDate: '2026-10-01', memo: 'Original request survives a whole owned reversal rollback' };
  await journalMoneyFault('money', f.company, command, `UPDATE assignments SET revoked_at=1,version=version+1 WHERE id='${f.manager.assignmentId}'`, () => reverseJournal(f.manager, id, command, body), { status: 403, code: 'FINANCE_ACCESS_CHANGED' });
  const extra = `INSERT INTO money_transaction_snapshots(company_id,source_type,source_id,amount_kind,transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at)
    SELECT company_id,source_type,source_id,'adverse-cleanup',transaction_date,currency,currency_scale,amount_units,usd_units,fx_rate_id,used_latest_fallback,captured_at FROM money_transaction_snapshots WHERE company_id='${f.company}' AND source_type='finance_journal' AND source_id='${id}' AND amount_kind='debit_total'`;
  await journalMoneyFault('cleanup', f.company, command, extra, () => reverseJournal(f.manager, id, command, body), { status: 409, code: 'JOURNAL_MONEY_SOURCE_CHANGED' });
  const reversed = await reverseJournal(f.manager, id, command, body), stable = await journalMoneyImage();
  expect(reversed).toEqual({ reversalJournalId: `reversal-${command}`, reversedJournalId: id });
  expect(await reverseJournal(f.manager, id, command, body)).toEqual(reversed);expect(await journalMoneyImage()).toEqual(stable);
  await journalMoneyFault('cleanup', f.company, command, extra, () => reverseJournal(f.manager, id, command, body), { status: 409, code: 'JOURNAL_MONEY_SOURCE_CHANGED' });
  expect(await reverseJournal(f.manager, id, command, body)).toEqual(reversed);expect(await journalMoneyImage()).toEqual(stable);
  await db.prepare('UPDATE assignments SET revoked_at=1,version=version+1 WHERE id=?').bind(f.manager.assignmentId).run();
  const revoked = await journalMoneyImage();await expect(reverseJournal(f.manager, id, command, body)).rejects.toMatchObject({ status: 403, code: 'FINANCE_ACCESS_CHANGED' });expect(await journalMoneyImage()).toEqual(revoked);
});

it('waits before the real selected Work row held by an old disabled Finance command without deadlocks serialization or hidden retries', async () => {
  const { reverseJournal, createAccount } = await import('@erpx/server-finance/public/finance');
  const { measureReads } = await import('@erpx/server-runtime/adapters/runtime/read-metrics');
  const journal = await createJournal(f.owner, createJournalSchema.parse(input()), key()), id = String(journal.id);await postJournal(f.manager, id, key(), { expectedVersion: 1 });
  vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT', 'disabled');
  const {selectWorkRole}=await import('@erpx/server-work_projection/public/work-role-store'),sessionId=key(),version=Number(await db.prepare('SELECT version FROM assignments WHERE id=?').bind(f.manager.assignmentId).first('version'));
  const selection=await selectWorkRole(db,{sessionId,userId:f.manager.userId,companyId:f.company,assignmentId:f.manager.assignmentId,assignmentVersion:version,revision:0,expiresAt:Date.now()+600000});
  const actor={...f.manager,authority:{sessionId,revision:selection.revision,assignmentVersion:version}};
  const oldBody = { chartId: f.chart, code: '5881', name: 'Original disabled owner account', category: 'expense' as const, currency: 'THB', controlKind: 'none' as const, isControl: false }, oldKey = key(), reversalKey = key(), body = { periodId: f.period, entryDate: '2026-10-01', memo: 'Actual new reversal waits before protected snapshots' }, name = 'journal_money_writer_' + key().replaceAll('-', '');
  // This additional view trigger follows command_assertions_change by name.
  // The normal handler still validates/stores the guard, after the original
  // active_work_contexts FOR UPDATE statement and before receipt insertion.
  await db.exec(`CREATE SEQUENCE ${name};CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN
    IF NEW.command_id='${oldKey}' AND NEW.step='work-context' THEN
      IF NEW.ok<>1 OR (COALESCE(NULLIF(current_setting('erpx.command_assertions',true),''),'{}')::jsonb #>> ARRAY[NEW.command_id,NEW.step]) IS DISTINCT FROM '1' OR NOT EXISTS(SELECT 1 FROM active_work_contexts WHERE session_id='${sessionId}' AND company_id='${f.company}' AND user_id='${actor.userId}' AND assignment_id='${actor.assignmentId}' AND assignment_version=${version} AND revision=${selection.revision}) THEN RAISE EXCEPTION 'Actual selected Work guard must precede the diagnostic barrier';END IF;
      PERFORM nextval('${name}');PERFORM pg_sleep(0.5);
    END IF;RETURN NEW;END $$;
    CREATE TRIGGER zz_${name} INSTEAD OF INSERT ON command_assertions FOR EACH ROW EXECUTE FUNCTION ${name}()`);
  try {
    const observed = await measureReads(async () => {
      let done = false;const old = Promise.allSettled([createAccount(actor, oldBody, oldKey).finally(() => { done = true; })]);
      let atReceipt = false;for (let attempt = 0; attempt < 100; attempt++) { atReceipt = !!await db.prepare(`SELECT is_called FROM ${name}`).first('is_called');if (atReceipt || done) break;await new Promise(resolve => setTimeout(resolve, 5)); }
      if (!atReceipt) { const [result] = await old;if (result.status === 'rejected') throw result.reason;throw Error('Original disabled writer never held its actual selected Work-context row'); }
      expect(done).toBe(false);const reversal = Promise.allSettled([reverseJournal(actor, id, reversalKey, body)]);
      let waiting = false;for (let attempt = 0; attempt < 100 && !waiting && !done; attempt++) { waiting = !!await db.prepare("SELECT 1 FROM pg_locks WHERE database=(SELECT oid FROM pg_database WHERE datname=current_database()) AND relation='active_work_contexts'::regclass AND mode='ExclusiveLock' AND NOT granted LIMIT 1").first();if (!waiting) await new Promise(resolve => setTimeout(resolve, 5)); }
      const [[oldResult], [reversed]] = await Promise.all([old, reversal]);return { atReceipt, waiting, oldResult, reversed };
    });
    expect(observed.result.atReceipt).toBe(true);expect(observed.result.waiting).toBe(true);expect(observed.result.oldResult.status).toBe('fulfilled');expect(observed.result.reversed.status).toBe('fulfilled');
    expect({ deadlockFailures: observed.metrics.deadlockFailures ?? 0, serializationFailures: observed.metrics.serializationFailures ?? 0, transactionRetries: observed.metrics.transactionRetries ?? 0 }).toEqual({ deadlockFailures: 0, serializationFailures: 0, transactionRetries: 0 });
    expect(await db.prepare('SELECT status FROM finance_journals WHERE id=?').bind(id).first('status')).toBe('reversed');expect(await db.prepare("SELECT COUNT(*) n FROM money_transaction_snapshots WHERE company_id=? AND source_type='finance_journal' AND source_id=?").bind(f.company, `reversal-${reversalKey}`).first('n')).toBe(1);
    const stable = await journalMoneyImage();expect(await createAccount(actor, oldBody, oldKey)).toEqual(observed.result.oldResult.status === 'fulfilled' ? observed.result.oldResult.value : undefined);expect(await reverseJournal(actor, id, reversalKey, body)).toEqual(observed.result.reversed.status === 'fulfilled' ? observed.result.reversed.value : undefined);expect(await journalMoneyImage()).toEqual(stable);
  } finally { await db.exec(`DROP TRIGGER zz_${name} ON command_assertions;DROP FUNCTION ${name}();DROP SEQUENCE ${name}`); }
});

async function journalMoneyOpeningSource() {
  const native = await import('@erpx/server-finance/public/finance');
  const receivable = await native.createAccount(f.owner, { chartId: f.chart, code: '1181', name: 'Original opening receivable', category: 'asset', currency: 'THB', controlKind: 'receivable', isControl: true }, key());
  const equity = await native.createAccount(f.owner, { chartId: f.chart, code: '3181', name: 'Original opening equity', category: 'equity', currency: 'THB', controlKind: 'none', isControl: false }, key());
  const party = await native.createParty(f.owner, native.partySchema.parse({ kind: 'customer', code: key(), name: 'Original opening customer', currency: 'THB' }), key());
  const definition = native.openingTrialBalanceSchema.parse({ chartId: f.chart, periodId: f.period, entryDate: '2026-09-30', currency: 'THB', lines: [
    { accountId: receivable.id, debit: '100.01', credit: '0', subledgerKind: 'receivable' },
    { accountId: equity.id, debit: '0', credit: '100.01', subledgerKind: 'none' },
  ], subledgerItems: [{ kind: 'receivable', partyId: party.id, reference: key(), dueDate: '2026-10-31', controlAccountId: receivable.id, offsetAccountId: equity.id, amount: '100.01' }] });
  const preview = await native.previewOpeningTrialBalance(f.owner, definition), imported = await native.importOpeningTrialBalance(f.owner, { ...definition, previewToken: preview.previewToken }, key()), id = String(imported.id);
  await postJournal(f.manager, id, key(), { expectedVersion: 1 });
  const invoice = await db.prepare("SELECT id,version,status,outstanding_minor,base_outstanding_minor FROM finance_invoices WHERE company_id=? AND source_type='opening_import' AND source_id=?").bind(f.company, id).first<{ id: string; version: number; status: string; outstanding_minor: number; base_outstanding_minor: number }>();
  if (!invoice) throw Error('Actual original Opening import must own its real subledger invoice');
  expect(invoice).toMatchObject({ status: 'posted', outstanding_minor: 10001, base_outstanding_minor: 10001 });
  return { native, id, invoice, party, receivable, equity };
}
it('retains real unused Opening subledgers and their original Money when a late cleanup changes the exact voided invoice, then retries the original reversal atomically', async () => {
  await journalMoneyFinancePurchase();
  const source = await journalMoneyOpeningSource(), original = await journalMoneyRows(), sourceMoney = original.find(row => row.source_id === source.id);
  // Current original Opening posting DOES capture Money. This case proves
  // native source preservation, not a fabricated pre-capture legacy source.
  expect(sourceMoney).toMatchObject({ amount_units: '10001', usd_units: '278' });
  const command = key(), body = { periodId: f.period, entryDate: '2026-10-01', memo: 'Independently correct the exact unused opening schedules' };
  await journalMoneyFault('cleanup', f.company, command, `UPDATE finance_invoices SET status='posted',outstanding_minor=total_minor,base_outstanding_minor=base_total_minor,version=version+1,updated_at=updated_at+1 WHERE id='${source.invoice.id}' AND company_id='${f.company}'`, () => source.native.reverseJournal(f.manager, source.id, command, body), { status: 409, code: 'JOURNAL_MONEY_SOURCE_CHANGED' });
  const reversed = await source.native.reverseJournal(f.manager, source.id, command, body), id = String(reversed.reversalJournalId);
  expect(reversed).toEqual({ reversalJournalId: `reversal-${command}`, reversedJournalId: source.id });
  expect(await db.prepare('SELECT status,outstanding_minor,base_outstanding_minor,version FROM finance_invoices WHERE id=?').bind(source.invoice.id).first()).toEqual({ status: 'voided', outstanding_minor: 0, base_outstanding_minor: 0, version: source.invoice.version + 1 });
  expect(await db.prepare('SELECT 1 FROM finance_allocations WHERE invoice_id=?').bind(source.invoice.id).first()).toBeNull();
  const money = await journalMoneyRows();expect(money.find(row => row.source_id === source.id)).toEqual(sourceMoney);expect(money.find(row => row.source_id === id)).toEqual({ ...sourceMoney, source_id: id, transaction_date: '2026-10-01', amount_units: '-10001', usd_units: '-278', original_source_type: 'finance_journal', original_source_id: source.id, original_amount_kind: 'debit_total' });
  const stable = await journalMoneyImage();expect(await source.native.reverseJournal(f.manager, source.id, command, body)).toEqual(reversed);expect(await journalMoneyImage()).toEqual(stable);
  await expect(source.native.reverseJournal(f.manager, id, key(), { ...body, entryDate: '2026-10-02' })).rejects.toMatchObject({ code: 'REIMPORT_OPENING_BALANCE' });expect(await journalMoneyImage()).toEqual(stable);
});

it('waits for the actual disabled Opening receipt confirmation before its first reversal snapshot and preserves the original settled-subledger refusal with zero retries', async () => {
  await journalMoneyFinancePurchase();
  const source = await journalMoneyOpeningSource(), { measureReads } = await import('@erpx/server-runtime/adapters/runtime/read-metrics');
  const bank = await source.native.createAccount(f.owner, { chartId: f.chart, code: '1081', name: 'Actual original opening receipt bank', category: 'asset', currency: 'THB', controlKind: 'bank', isControl: true }, key());
  f.payer.role = 'treasury';f.payer.assignmentId = await f.grant(f.payer, 'treasury');
  const payment = await source.native.createPayment(f.owner, { partyId: String(source.party.id), kind: 'receipt', paymentNumber: key(), paymentDate: '2026-10-02', currency: 'THB', amount: '100.01', bankAccount: String(bank.id), settlementInvoiceId: source.invoice.id }, key()), paymentId = String(payment.id);
  await source.native.approvePayment(f.manager, paymentId, { expectedVersion: 1, proof: 'A different Finance approver reviewed the actual opening receivable' }, key());
  await source.native.releasePayment(f.payer, paymentId, { expectedVersion: 2, proof: 'Original independent Treasury release' }, key());
  const externalId = key(), statement = await source.native.importBankStatement(f.owner, { bankAccount: String(bank.id), statementNumber: key(), statementDate: '2026-10-31', opening: '0', closing: '100.01', sourceHash: key(), rows: [{ externalId, transactionDate: '2026-10-02', description: 'Original received opening receivable', amount: '100.01', currency: 'THB' }] }, key()), bankId = String(await db.prepare('SELECT id FROM finance_bank_transactions WHERE statement_id=?').bind(statement.id).first('id'));
  const confirmBody = { expectedVersion: 3, periodId: f.period, bankTransactionId: bankId, bankVersion: 1, bankReference: externalId, proof: 'Actual matching bank amount, currency and invoice identity', allocations: [{ invoiceId: source.invoice.id, amount: '100.01' }] }, confirmKey = key(), reversalKey = key(), reverseBody = { periodId: f.period, entryDate: '2026-10-03', memo: 'Current source must refuse a genuinely settled opening' }, name = 'journal_opening_writer_' + key().replaceAll('-', '');
  // Signature-backed Finance confirmation retains its package RX even with
  // disabled purchase enforcement. This is its true original source boundary.
  vi.stubEnv('ERPX_FINANCE_PACKAGE_ENFORCEMENT', 'disabled');
  await db.exec(`CREATE SEQUENCE ${name};CREATE FUNCTION ${name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM nextval('${name}');PERFORM pg_sleep(0.5);RETURN NEW;END $$;
    CREATE TRIGGER ${name} AFTER INSERT ON command_receipts FOR EACH ROW WHEN(NEW.command_id='${confirmKey}') EXECUTE FUNCTION ${name}()`);
  try {
    const observed = await measureReads(async () => {
      let done = false;const confirmation = Promise.allSettled([source.native.confirmPayment(f.payer, paymentId, confirmBody, confirmKey).finally(() => { done = true; })]);
      let atReceipt = false;for (let attempt = 0; attempt < 100; attempt++) { atReceipt = !!await db.prepare(`SELECT is_called FROM ${name}`).first('is_called');if (atReceipt || done) break;await new Promise(resolve => setTimeout(resolve, 5)); }
      if (!atReceipt) { const [result] = await confirmation;if (result.status === 'rejected') throw result.reason;throw Error('Actual original opening payment did not reach its native receipt barrier'); }
      expect(done).toBe(false);const reversal = Promise.allSettled([source.native.reverseJournal(f.manager, source.id, reversalKey, reverseBody)]);
      let waiting = false;for (let attempt = 0; attempt < 100 && !waiting && !done; attempt++) { waiting = !!await db.prepare("SELECT 1 FROM pg_locks WHERE database=(SELECT oid FROM pg_database WHERE datname=current_database()) AND relation='company_package_grants'::regclass AND mode='ShareRowExclusiveLock' AND NOT granted LIMIT 1").first();if (!waiting) await new Promise(resolve => setTimeout(resolve, 5)); }
      const [[settled], [reversed]] = await Promise.all([confirmation, reversal]);return { waiting, settled, reversed };
    });
    expect(observed.result.waiting).toBe(true);expect(observed.result.settled.status).toBe('fulfilled');expect(observed.result.reversed.status).toBe('rejected');if (observed.result.reversed.status !== 'rejected') throw Error('Original Opening allocation must refuse reversal at its true source');expect(observed.result.reversed.reason).toMatchObject({ status: 409, code: 'OPENING_SUBLEDGER_SETTLED' });
    expect({ deadlockFailures: observed.metrics.deadlockFailures ?? 0, serializationFailures: observed.metrics.serializationFailures ?? 0, transactionRetries: observed.metrics.transactionRetries ?? 0 }).toEqual({ deadlockFailures: 0, serializationFailures: 0, transactionRetries: 0 });
    expect(await db.prepare('SELECT status,outstanding_minor FROM finance_invoices WHERE id=?').bind(source.invoice.id).first()).toEqual({ status: 'paid', outstanding_minor: 0 });
    expect(await db.prepare('SELECT status FROM finance_journals WHERE id=?').bind(source.id).first('status')).toBe('posted');expect(await db.prepare('SELECT 1 FROM command_receipts WHERE command_id=?').bind(reversalKey).first()).toBeNull();expect(await db.prepare("SELECT 1 FROM money_transaction_snapshots WHERE company_id=? AND source_type='finance_journal' AND source_id=?").bind(f.company, `reversal-${reversalKey}`).first()).toBeNull();
    const stable = await journalMoneyImage();expect(await source.native.confirmPayment(f.payer, paymentId, confirmBody, confirmKey)).toEqual(observed.result.settled.status === 'fulfilled' ? observed.result.settled.value : undefined);await expect(source.native.reverseJournal(f.manager, source.id, reversalKey, reverseBody)).rejects.toMatchObject({ code: 'OPENING_SUBLEDGER_SETTLED' });expect(await journalMoneyImage()).toEqual(stable);
  } finally { await db.exec(`DROP TRIGGER ${name} ON command_receipts;DROP FUNCTION ${name}();DROP SEQUENCE ${name}`); }
});
