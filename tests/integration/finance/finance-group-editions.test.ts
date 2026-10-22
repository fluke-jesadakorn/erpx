import {financeExpansionWork} from "@erpx/server-finance/public/finance-expansion-work";
import {attachmentPolicy,authorizeAttachmentTarget} from "@erpx/server-files/public/attachments/policy";
import { reviewedFinanceGrant } from "../../support/helpers/finance-purchase-fixture";
import { afterEach, beforeAll, beforeEach, expect, it, vi } from "vitest";
import {
  prepareAccountingProfile,
  decideAccountingProfile,
} from "@erpx/server-finance/public/accounting-profiles";
import {
  prepareYearClosing,
  decideYearClosing,
} from "@erpx/server-finance/public/finance-reporting";
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
  reverseJournal,
  createPeriod,
  beginClose,
  closePeriod,
} from "@erpx/server-finance/public/finance";
import {
  prepareGroupReportEdition,
  groupReportEditionWorkspace,
  decideGroupReportEdition,
  groupReportEditionExport,
  groupExpansionRecord,
  groupEditionAccountSources,
  groupEditionSourceJournal,
} from "@erpx/server-finance/public/finance-group-editions";
import {
  prepareOwnershipEdition,
  decideOwnershipEdition,
  prepareAcquisitionEdition,
  decideAcquisitionEdition,
} from "@erpx/server-finance/public/finance-group-ownership";
import {
  prepareIntercompanyPair,
  decideIntercompanyPair,
} from "@erpx/server-finance/public/finance-intercompany-pairs";
import {
  createCalculationRun,
  decideCalculationRun,
} from "@erpx/server-finance/public/finance-engines";
vi.mock("server-only", () => ({}));
vi.mock("@erpx/server-runtime/adapters/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
type Fixture = Awaited<ReturnType<typeof expenseFixture>>;
it('rolls back both original posted document claims after final pair outbox failure, then retries and replays once',async()=>{
 const source=await journal(f,'2026-02-15',[{accountId:f.accounts.tax,debit:'50',credit:'0'},{accountId:f.accounts.revenue,debit:'0',credit:'50'}]),target=await journal(g,'2026-02-15',[{accountId:g.accounts.expense,debit:'50',credit:'0'},{accountId:g.accounts.payable,debit:'0',credit:'50'}]),input={reference:'Original pair retained on retry',source:{companyId:f.company,journalId:source,accountId:f.accounts.tax},target:{companyId:g.company,journalId:target,accountId:g.accounts.payable},currency:'THB',amount:'50',reason:'Reviewed original posted document accounts'},command=key(),id=`intercompany-pair-${command}`,fault=`pair_fault_${crypto.randomUUID().replaceAll('-','')}`;
 await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'INTERCOMPANY_PAIR_LATE_OUTBOX' USING ERRCODE='23514';END;$$;CREATE TRIGGER ${fault} AFTER INSERT ON outbox_events FOR EACH ROW WHEN (NEW.event_type='finance.intercompany-pair.prepare') EXECUTE FUNCTION ${fault}();`);
 try{
  await expect(prepareIntercompanyPair(f.owner,input,command)).rejects.toThrow();
  expect(await db.prepare('SELECT id FROM finance_intercompany_document_pairs WHERE id=?').bind(id).first()).toBeNull();
  expect(Number(await db.prepare('SELECT count(*) n FROM finance_intercompany_pair_claims WHERE pair_id=?').bind(id).first('n'))).toBe(0);
  for(const table of ['command_receipts','audit_events','outbox_events'])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first('n'))).toBe(0);
 }finally{await db.exec(`DROP TRIGGER ${fault} ON outbox_events;DROP FUNCTION ${fault}();`);}
 const pair=await prepareIntercompanyPair(f.owner,input,command);expect(await prepareIntercompanyPair(f.owner,input,command)).toEqual(pair);
 expect(Number(await db.prepare('SELECT count(*) n FROM finance_intercompany_pair_claims WHERE pair_id=?').bind(id).first('n'))).toBe(2);
 for(const table of ['command_receipts','audit_events','outbox_events'])expect(Number(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first('n'))).toBe(1);
});
it("rolls acquisition calculation, evidence and command records back after a final outbox failure, then retries and replays once", async () => {
  const interest = await prepareOwnershipEdition(f.owner, {
    investorCompanyId:f.company, investeeCompanyId:g.company, method:'subsidiary',
    ownershipBps:8000, votingBps:8000, controlAssessment:'Reviewed subsidiary control',
    effectiveFrom:'2026-01-01', reason:'Reviewed acquisition ownership',
  }, key());
  await decideOwnershipEdition(f.manager, String(interest.id), {
    expectedVersion:1, reason:'Independent ownership approval',
  }, 'approve', key());
  const command = key(), acquisitionId = `acquisition-${command}`, calculationId = `acquisition-calc-${command}`;
  const input = {
    ownershipId:String(interest.id), ownershipVersion:2, acquisitionDate:'2026-01-20',
    currency:'THB', consideration:'100', nciMeasurement:'fair_value' as const,
    nciFairValue:'20', identifiableNetAssets:'80', cguReference:'Reviewed source CGU',
    reason:'Reviewed purchase evidence',
  };
  const fault = `group_acquisition_fault_${command.replaceAll('-','_')}`;
  await db.exec(`CREATE FUNCTION ${fault}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'GROUP_ACQUISITION_FINAL_OUTBOX' USING ERRCODE='23514'; END; $$; CREATE TRIGGER ${fault} BEFORE INSERT ON outbox_events FOR EACH ROW WHEN(NEW.command_id='${command.replaceAll("'","''")}') EXECUTE FUNCTION ${fault}();`);
  try {
    await expect(prepareAcquisitionEdition(f.owner,input,command)).rejects.toMatchObject({code:'23514'});
    expect(await db.prepare('SELECT 1 FROM finance_acquisitions WHERE id=?').bind(acquisitionId).first()).toBeNull();
    expect(await db.prepare('SELECT 1 FROM finance_calculation_runs WHERE id=?').bind(calculationId).first()).toBeNull();
    expect(await db.prepare('SELECT 1 FROM finance_calculation_events WHERE run_id=?').bind(calculationId).first()).toBeNull();
    for (const table of ['command_receipts','audit_events','outbox_events'])
      expect(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first('n')).toBe(0);
  } finally {
    await db.exec(`DROP TRIGGER ${fault} ON outbox_events; DROP FUNCTION ${fault}();`);
  }
  const applied = await prepareAcquisitionEdition(f.owner,input,command);
  expect(await prepareAcquisitionEdition(f.owner,input,command)).toEqual(applied);
  expect(applied).toMatchObject({id:acquisitionId,calculationRunId:calculationId,version:1});
  expect(await db.prepare('SELECT count(*) n FROM finance_acquisitions WHERE id=?').bind(acquisitionId).first('n')).toBe(1);
  expect(await db.prepare('SELECT count(*) n FROM finance_calculation_runs WHERE id=?').bind(calculationId).first('n')).toBe(1);
  expect(await db.prepare('SELECT count(*) n FROM finance_calculation_events WHERE run_id=?').bind(calculationId).first('n')).toBe(1);
  for (const table of ['command_receipts','audit_events','outbox_events'])
    expect(await db.prepare(`SELECT count(*) n FROM ${table} WHERE command_id=?`).bind(command).first('n')).toBe(1);
});
it("recognizes the associate's actual period profit through an owned equity-method bridge while excluding its individual GL", async () => {
  await activity(f, "30", "10");
  await activity(g, "20", "5");
  const investment = key();
  await db
    .prepare(
      "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,'associate','Associate investment','asset','THB',1,1)",
    )
    .bind(investment, f.chart, f.company)
    .run();
  await journal(f, "2026-01-20", [
    { accountId: investment, debit: "40", credit: "0" },
    { accountId: f.accounts.cash, debit: "0", credit: "40" },
  ]);
  const o = await prepareOwnershipEdition(
    f.owner,
    {
      investorCompanyId: f.company,
      investeeCompanyId: g.company,
      method: "equity",
      ownershipBps: 2000,
      votingBps: 2000,
      controlAssessment: "Significant influence with twenty percent interest",
      effectiveFrom: "2026-01-01",
      reason: "Reviewed associate ownership",
    },
    key(),
  );
  await decideOwnershipEdition(
    f.manager,
    String(o.id),
    {
      expectedVersion: 1,
      reason: "Significant influence and percentage verified",
    },
    "approve",
    key(),
  );
  const c = await createCalculationRun(
    f.owner,
    {
      kind: "consolidation",
      currency: "THB",
      currencyScale: 2,
      policyVersion: 1,
      input: {
        consideration: "0",
        fairValueNetAssets: "0",
        parentBps: 10000,
        associateOpening: "40",
        associateBps: 2000,
        associateProfit: "15",
      },
    },
    key(),
  );
  await decideCalculationRun(
    f.manager,
    String(c.id),
    {
      expectedVersion: 1,
      reason: "Source book profit fifteen and carrying forty checked",
    },
    "review",
    key(),
  );
  await decideCalculationRun(
    f.manager,
    String(c.id),
    {
      expectedVersion: 2,
      reason: "Associate closing forty-three independently approved",
    },
    "approve",
    key(),
  );
  const input = await definition(),
    e = await prepareGroupReportEdition(
      f.owner,
      {
        ...input,
        entities: input.entities.map((x) => ({
          ...x,
          method: x.companyId === g.company ? "equity" : "full",
        })),
        adjustments: [
          {
            calculationRunId: String(c.id),
            ownershipId: String(o.id),
            output: "equityMethodClosing",
            debit: {
              code: "associate",
              name: "Associate investment",
              category: "asset",
            },
            credit: {
              code: "associate_income",
              name: "Share of associate profit",
              category: "other_income",
            },
            existingDebitBalance: "40",
          },
        ],
      },
      key(),
    ),
    r = await result(String(e.id));
  expect(r.profit).toBe("23.00");
  expect(r.balanceSheet.find((a) => a.code === "cash")?.amount).toBe("80.00");
  expect(r.balanceSheet.find((a) => a.code === "associate")?.amount).toBe(
    "43.00",
  );
  expect(r.balanceSheet.find((a) => a.code === "capital")?.amount).toBe(
    "-100.00",
  );
  await expect(
    prepareGroupReportEdition(
      f.owner,
      {
        ...input,
        entities: input.entities.map((x) => ({
          ...x,
          method: x.companyId === g.company ? "equity" : "full",
        })),
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "GROUP_EQUITY_ADJUSTMENT_REQUIRED" });
});
it("retains dated ownership revisions and rejects stale ownership/acquisition evidence", async () => {
  const definition = {
      investorCompanyId: f.company,
      investeeCompanyId: g.company,
      method: "subsidiary" as const,
      ownershipBps: 8000,
      votingBps: 8000,
      controlAssessment: "Original reviewed eighty percent interest",
      effectiveFrom: "2026-01-01",
      reason: "Initial independently reviewed ownership",
    },
    o = await prepareOwnershipEdition(f.owner, definition, key());
  await decideOwnershipEdition(
    f.manager,
    String(o.id),
    { expectedVersion: 1, reason: "Effective range and control approved" },
    "approve",
    key(),
  );
  const a = await prepareAcquisitionEdition(
    f.owner,
    {
      ownershipId: String(o.id),
      ownershipVersion: 2,
      acquisitionDate: "2026-01-31",
      currency: "THB",
      consideration: "95",
      identifiableNetAssets: "100",
      nciMeasurement: "proportionate",
      cguReference: "Isolated CGU",
      reason: "Await independent acquisition review",
    },
    key(),
  );
  const revised = await prepareOwnershipEdition(
    f.owner,
    {
      ...definition,
      parentId: String(o.id),
      parentVersion: 2,
      ownershipBps: 9000,
      votingBps: 9000,
      effectiveFrom: "2026-03-01",
      reason: "Reviewed later purchase of ten percent interest",
    },
    key(),
  );
  await decideOwnershipEdition(
    f.manager,
    String(revised.id),
    { expectedVersion: 1, reason: "Later ownership revision approved" },
    "approve",
    key(),
  );
  expect(
    await db
      .prepare(
        "SELECT status,effective_to,ownership_bps FROM finance_ownership_interests WHERE id=?",
      )
      .bind(o.id)
      .first(),
  ).toMatchObject({
    status: "retired",
    effective_to: "2026-02-28",
    ownership_bps: 8000,
  });
  await expect(
    decideAcquisitionEdition(
      f.manager,
      String(a.id),
      {
        expectedVersion: 1,
        reason: "Ownership evidence changed after preparation",
      },
      "approve",
      key(),
    ),
  ).rejects.toMatchObject({ code: "ACQUISITION_OWNERSHIP_REQUIRED" });
  await expect(
    db
      .prepare(
        "UPDATE finance_ownership_interests SET ownership_bps=1 WHERE id=?",
      )
      .bind(o.id)
      .run(),
  ).rejects.toThrow("OWNERSHIP_EVIDENCE_IMMUTABLE");
});
it("commits one independent approval under concurrency and rejects cross-Company and cross-Holding edition access", async () => {
  await activity(f, "30", "10");
  await activity(g, "20", "5");
  const e = await prepareGroupReportEdition(f.owner, await definition(), key()),
    decision = {
      expectedVersion: 1,
      reason: "Independent concurrent group source review",
    },
    k = key();
  const approvals = await Promise.allSettled([
    decideGroupReportEdition(f.manager, String(e.id), decision, "approve", k),
    decideGroupReportEdition(
      f.manager,
      String(e.id),
      decision,
      "approve",
      key(),
    ),
  ]);
  expect(approvals.filter((a) => a.status === "fulfilled")).toHaveLength(1);
  expect(
    await db
      .prepare(
        "SELECT version,status FROM finance_group_report_editions WHERE id=?",
      )
      .bind(e.id)
      .first(),
  ).toMatchObject({ version: 2, status: "approved" });
  await expect(
    groupReportEditionExport(g.owner, String(e.id)),
  ).rejects.toMatchObject({ code: "GROUP_EDITION_NOT_FOUND" });
  await expect(
    groupReportEditionExport({ ...f.owner, holdingId: "h002" }, String(e.id)),
  ).rejects.toMatchObject({ code: "GROUP_EDITION_NOT_FOUND" });
});
it("eliminates acquisition investment and subsidiary equity, carries goodwill and NCI, and attributes profit without reducing total group profit", async () => {
  await activity(f, "30", "10");
  await activity(g, "20", "5");
  const investment = key();
  await db
    .prepare(
      "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,'investment','Investment','asset','THB',1,1)",
    )
    .bind(investment, f.chart, f.company)
    .run();
  await journal(f, "2026-01-20", [
    { accountId: investment, debit: "95", credit: "0" },
    { accountId: f.accounts.cash, debit: "0", credit: "95" },
  ]);
  const ownership = await prepareOwnershipEdition(
    f.owner,
    {
      investorCompanyId: f.company,
      investeeCompanyId: g.company,
      method: "subsidiary",
      ownershipBps: 8000,
      votingBps: 8000,
      controlAssessment: "Reviewed control of eighty percent voting rights",
      effectiveFrom: "2026-01-01",
      reason: "Independent ownership evidence",
    },
    key(),
  );
  await expect(
    decideOwnershipEdition(
      f.owner,
      String(ownership.id),
      { expectedVersion: 1, reason: "Self review is prohibited" },
      "approve",
      key(),
    ),
  ).rejects.toMatchObject({ code: "SELF_REVIEW_FORBIDDEN" });
  await decideOwnershipEdition(
    f.manager,
    String(ownership.id),
    { expectedVersion: 1, reason: "Voting rights and control reviewed" },
    "approve",
    key(),
  );
  const acquisition = await prepareAcquisitionEdition(
    f.owner,
    {
      ownershipId: String(ownership.id),
      ownershipVersion: 2,
      acquisitionDate: "2026-01-31",
      currency: "THB",
      consideration: "95",
      identifiableNetAssets: "100",
      nciMeasurement: "proportionate",
      cguReference: "Fixture subsidiary CGU",
      reason: "Reviewed net assets equal acquisition-date subsidiary equity",
    },
    key(),
  );
  expect(acquisition.result).toMatchObject({
    goodwill: "15.00",
    bargainPurchase: "0.00",
  });
  await decideAcquisitionEdition(
    f.manager,
    String(acquisition.id),
    {
      expectedVersion: 1,
      reason: "Independent acquisition measurement checked",
    },
    "approve",
    key(),
  );
  await expect(
    db
      .prepare("UPDATE finance_acquisitions SET goodwill_minor=1 WHERE id=?")
      .bind(acquisition.id)
      .run(),
  ).rejects.toThrow("ACQUISITION_EVIDENCE_IMMUTABLE");
  await expect(
    db
      .prepare("UPDATE finance_calculation_runs SET input_json='{}' WHERE id=?")
      .bind(acquisition.calculationRunId)
      .run(),
  ).rejects.toThrow("ACQUISITION_CALCULATION_IMMUTABLE");
  const calc = await createCalculationRun(
    f.owner,
    {
      kind: "consolidation",
      currency: "THB",
      currencyScale: 2,
      policyVersion: 1,
      input: {
        consideration: "0",
        fairValueNetAssets: "0",
        parentBps: 8000,
        openingNci: "20",
        subsidiaryProfit: "15",
      },
    },
    key(),
  );
  await decideCalculationRun(
    f.manager,
    String(calc.id),
    {
      expectedVersion: 1,
      reason: "Actual February profit and ownership matched",
    },
    "review",
    key(),
  );
  await decideCalculationRun(
    f.manager,
    String(calc.id),
    { expectedVersion: 2, reason: "Independent NCI bridge approved" },
    "approve",
    key(),
  );
  const input = await definition(),
    nci = {
      code: "nci",
      name: "Noncontrolling interests",
      category: "equity" as const,
    },
    retained = {
      code: "parent_retained",
      name: "Parent retained equity allocation",
      category: "equity" as const,
    },
    goodwill = {
      code: "goodwill",
      name: "Goodwill",
      category: "asset" as const,
    },
    bargain = {
      code: "bargain",
      name: "Bargain purchase",
      category: "other_income" as const,
    };
  const e = await prepareGroupReportEdition(
      f.owner,
      {
        ...input,
        acquisitionEliminations: [
          {
            acquisitionId: String(acquisition.id),
            investmentAccountId: investment,
            goodwill,
            nci,
            bargainPurchase: bargain,
          },
        ],
        adjustments: [
          {
            calculationRunId: String(calc.id),
            ownershipId: String(ownership.id),
            output: "nciClosing",
            debit: retained,
            credit: nci,
            existingDebitBalance: "20",
          },
          {
            calculationRunId: String(calc.id),
            ownershipId: String(ownership.id),
            output: "nciProfitShare",
            debit: retained,
            credit: nci,
            existingDebitBalance: "0",
          },
        ],
      },
      key(),
    ),
    r = await result(String(e.id));
  expect(r.balanceSheet.find((x) => x.code === "capital")?.amount).toBe(
    "-100.00",
  );
  expect(r.balanceSheet.find((x) => x.code === "investment")?.amount).toBe(
    "0.00",
  );
  expect(r.balanceSheet.find((x) => x.code === "cash")?.amount).toBe("140.00");
  expect(r.balanceSheet.find((x) => x.code === "goodwill")?.amount).toBe(
    "15.00",
  );
  expect(r.balanceSheet.find((x) => x.code === "nci")?.amount).toBe("-23.00");
  expect(r.profit).toBe("35.00");
  expect(r).toMatchObject({
    profitAttribution: { parent: "32.00", nci: "3.00" },
  });
  await decideGroupReportEdition(
    f.manager,
    String(e.id),
    {
      expectedVersion: 1,
      reason: "Complete acquisition elimination and profit attribution checked",
    },
    "approve",
    key(),
  );
  const oldExport = await groupReportEditionExport(f.manager, String(e.id)),
    retainedAcquisition = await db
      .prepare(
        "SELECT source_json,source_hash,consideration_minor,nci_minor,goodwill_minor,calculation_run_id FROM finance_acquisitions WHERE id=?",
      )
      .bind(acquisition.id)
      .first(),
    correction = {
      parentId: String(acquisition.id),
      parentVersion: 2,
      ownershipId: String(ownership.id),
      ownershipVersion: 2,
      acquisitionDate: "2026-01-31",
      currency: "THB",
      consideration: "95",
      identifiableNetAssets: "100",
      nciMeasurement: "fair_value" as const,
      nciFairValue: "25",
      cguReference: "Corrected independently reviewed CGU measurement",
      reason:
        "Correct NCI from proportionate twenty to independently measured fair value twenty-five",
    },
    corrected = await prepareAcquisitionEdition(f.owner, correction, key());
  expect(corrected.result).toMatchObject({
    goodwill: "20.00",
    bargainPurchase: "0.00",
  });
  await expect(
    prepareAcquisitionEdition(f.owner, correction, key()),
  ).rejects.toMatchObject({ code: "ACQUISITION_REVISION_REQUIRED" });
  await expect(
    decideAcquisitionEdition(
      f.owner,
      String(corrected.id),
      { expectedVersion: 1, reason: "Preparer cannot review correction" },
      "approve",
      key(),
    ),
  ).rejects.toMatchObject({ code: "SELF_REVIEW_FORBIDDEN" });
  await expect(
    decideAcquisitionEdition(
      { ...f.manager, companyId: g.company },
      String(corrected.id),
      { expectedVersion: 1, reason: "Other Company cannot review correction" },
      "approve",
      key(),
    ),
  ).rejects.toMatchObject({ code: "GROUP_EVIDENCE_NOT_FOUND" });
  const reviewed = await Promise.allSettled([
    decideAcquisitionEdition(
      f.manager,
      String(corrected.id),
      {
        expectedVersion: 1,
        reason: "Independent fair value NCI and goodwill reviewed",
      },
      "approve",
      key(),
    ),
    decideAcquisitionEdition(
      f.manager,
      String(corrected.id),
      {
        expectedVersion: 1,
        reason: "Independent fair value NCI and goodwill reviewed",
      },
      "approve",
      key(),
    ),
  ]);
  expect(reviewed.filter((x) => x.status === "fulfilled")).toHaveLength(1);
  expect(
    await db
      .prepare("SELECT status,version FROM finance_acquisitions WHERE id=?")
      .bind(acquisition.id)
      .first(),
  ).toMatchObject({ status: "superseded", version: 3 });
  expect(
    await db
      .prepare(
        "SELECT source_json,source_hash,consideration_minor,nci_minor,goodwill_minor,calculation_run_id FROM finance_acquisitions WHERE id=?",
      )
      .bind(acquisition.id)
      .first(),
  ).toEqual(retainedAcquisition);
  expect(await groupReportEditionExport(f.manager, String(e.id))).toEqual(
    oldExport,
  );
  await expect(
    prepareAcquisitionEdition(f.owner, correction, key()),
  ).rejects.toMatchObject({ code: "ACQUISITION_PARENT_CHANGED" });
  const bridge = await createCalculationRun(
    f.owner,
    {
      kind: "consolidation",
      currency: "THB",
      currencyScale: 2,
      policyVersion: 1,
      input: {
        consideration: "0",
        fairValueNetAssets: "0",
        parentBps: 8000,
        openingNci: "25",
        subsidiaryProfit: "15",
      },
    },
    key(),
  );
  await decideCalculationRun(
    f.manager,
    String(bridge.id),
    {
      expectedVersion: 1,
      reason:
        "Actual retained profit fifteen and corrected opening NCI twenty-five checked",
    },
    "review",
    key(),
  );
  await decideCalculationRun(
    f.manager,
    String(bridge.id),
    {
      expectedVersion: 2,
      reason: "Corrected closing NCI twenty-eight reviewed independently",
    },
    "approve",
    key(),
  );
  const amended = await prepareGroupReportEdition(
    f.owner,
    {
      ...input,
      parentId: String(e.id),
      parentVersion: 2,
      reason: "Amend the retained group report for reviewed NCI measurement",
      acquisitionEliminations: [
        {
          acquisitionId: String(corrected.id),
          investmentAccountId: investment,
          goodwill,
          nci,
          bargainPurchase: bargain,
        },
      ],
      adjustments: [
        {
          calculationRunId: String(bridge.id),
          ownershipId: String(ownership.id),
          output: "nciClosing",
          debit: retained,
          credit: nci,
          existingDebitBalance: "25",
        },
        {
          calculationRunId: String(bridge.id),
          ownershipId: String(ownership.id),
          output: "nciProfitShare",
          debit: retained,
          credit: nci,
          existingDebitBalance: "0",
        },
      ],
    },
    key(),
  );
  const amendedResult = await result(String(amended.id));
  expect(
    amendedResult.balanceSheet.find((x) => x.code === "goodwill")?.amount,
  ).toBe("20.00");
  expect(amendedResult.balanceSheet.find((x) => x.code === "nci")?.amount).toBe(
    "-28.00",
  );
  expect(amendedResult.profit).toBe("35.00");
  expect(amendedResult.profitAttribution).toEqual({
    parent: "32.00",
    nci: "3.00",
  });
  await decideGroupReportEdition(
    f.manager,
    String(amended.id),
    {
      expectedVersion: 1,
      reason:
        "Corrected NCI twenty-eight and goodwill twenty independently reconciled",
    },
    "approve",
    key(),
  );
  expect(await groupReportEditionExport(f.manager, String(e.id))).toEqual(
    oldExport,
  );
});
it("pairs actual posted documents, prevents duplicate claims and self review, and retains report-only elimination history", async () => {
  await activity(f, "30", "10");
  await activity(g, "20", "5");
  const source = await journal(f, "2026-02-15", [
      { accountId: f.accounts.tax, debit: "50", credit: "0" },
      { accountId: f.accounts.revenue, debit: "0", credit: "50" },
    ]),
    target = await journal(g, "2026-02-15", [
      { accountId: g.accounts.expense, debit: "50", credit: "0" },
      { accountId: g.accounts.payable, debit: "0", credit: "50" },
    ]);
  const input = {
      reference: "Actual intercompany documents",
      source: {
        companyId: f.company,
        journalId: source,
        accountId: f.accounts.tax,
      },
      target: {
        companyId: g.company,
        journalId: target,
        accountId: g.accounts.payable,
      },
      currency: "THB",
      amount: "50",
      reason: "Matched original Company source documents",
    },
    pair = await prepareIntercompanyPair(f.owner, input, key());
  await expect(
    prepareIntercompanyPair(f.owner, input, key()),
  ).rejects.toThrow();
  await expect(
    decideIntercompanyPair(
      f.owner,
      String(pair.id),
      { expectedVersion: 1, reason: "Independent review is mandatory" },
      "confirm",
      key(),
    ),
  ).rejects.toMatchObject({ code: "SELF_REVIEW_FORBIDDEN" });
  const report = await definition();
  await expect(
    prepareGroupReportEdition(
      f.owner,
      { ...report, intercompanyPairIds: [String(pair.id)] },
      key(),
    ),
  ).rejects.toMatchObject({ code: "GROUP_INTERCOMPANY_NOT_CONFIRMED" });
  await decideIntercompanyPair(
    f.manager,
    String(pair.id),
    { expectedVersion: 1, reason: "Both posted document totals agree" },
    "confirm",
    key(),
  );
  const e = await prepareGroupReportEdition(
      f.owner,
      { ...report, intercompanyPairIds: [String(pair.id)] },
      key(),
    ),
    r = await result(String(e.id));
  expect(r.balanceSheet.find((x) => x.code === "tax")?.amount).toBe("0.00");
  expect(r.balanceSheet.find((x) => x.code === "payable")?.amount).toBe("0.00");
  expect(r.profit).toBe("35.00");
  await decideGroupReportEdition(
    f.manager,
    String(e.id),
    {
      expectedVersion: 1,
      reason: "Original GL retained and group elimination checked",
    },
    "approve",
    key(),
  );
  const issued = await groupReportEditionExport(f.owner, String(e.id));
  const pending = await prepareGroupReportEdition(
    f.owner,
    {
      ...report,
      parentId: String(e.id),
      parentVersion: 2,
      intercompanyPairIds: [String(pair.id)],
      reason: "Retained follow-up reporting edition",
    },
    key(),
  );
  await decideIntercompanyPair(
    f.manager,
    String(pair.id),
    {
      expectedVersion: 2,
      reason: "Reverse incorrect pairing with retained history",
    },
    "reverse",
    key(),
  );
  await expect(
    decideGroupReportEdition(
      f.manager,
      String(pending.id),
      { expectedVersion: 1, reason: "Pairing changed after preparation" },
      "approve",
      key(),
    ),
  ).rejects.toMatchObject({ code: "GROUP_INTERCOMPANY_NOT_CONFIRMED" });
  expect(await groupReportEditionExport(f.owner, String(e.id))).toEqual(issued);
  expect(
    await db
      .prepare("SELECT status FROM finance_journals WHERE id=?")
      .bind(source)
      .first("status"),
  ).toBe("posted");
});
afterEach(()=>vi.unstubAllEnvs());
let f: Fixture, g: Fixture;
beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
});
beforeEach(async () => {
  f = await expenseFixture();
  g = await expenseFixture();
  for (const x of [f, g]) {
    await db
      .prepare(
        "UPDATE finance_periods SET start_date='2026-01-01',end_date='2026-01-31' WHERE id=?",
      )
      .bind(x.period)
      .run();
    await db
      .prepare(
        "INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES(?,?,'February','2026-02-01','2026-02-28',1)",
      )
      .bind(`${x.period}-feb`, x.company)
      .run();
    await db
      .prepare(
        "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,'capital','Capital','equity','THB',1,1)",
      )
      .bind(`${x.company}-capital`, x.chart, x.company)
      .run();
    await db
      .prepare(
        "INSERT INTO finance_group_entities(id,holding_id,company_id,reporting_name,functional_currency,created_at) VALUES(?,'h001',?,?,'THB',1)",
      )
      .bind(`${x.company}-entity`, x.company, x.company)
      .run();
  }
  await db
    .prepare(
      "INSERT INTO finance_company_tools(company_id,tool,enabled,version,changed_by,reason,updated_at) VALUES(?,'group_reporting',1,1,?,'Isolated group accounting fixture',1)",
    )
    .bind(f.company, f.owner.userId)
    .run();
});
async function journal(
  x: Fixture,
  date: string,
  rows: { accountId: string; debit: string; credit: string }[],
  currency = "THB",
) {
  const j = await createJournal(
    x.owner,
    createJournalSchema.parse({
      chartId: x.chart,
      periodId: date.startsWith("2026-01") ? x.period : `${x.period}-feb`,
      entryDate: date,
      currency,
      lines: rows,
    }),
    key(),
  );
  await postJournal(x.manager, String(j.id), key(), { expectedVersion: 1 });
  return String(j.id);
}
async function activity(
  x: Fixture,
  sales: string,
  cost: string,
  currency = "THB",
) {
  await journal(
    x,
    "2026-01-10",
    [
      { accountId: x.accounts.cash, debit: "100.00", credit: "0" },
      { accountId: `${x.company}-capital`, debit: "0", credit: "100.00" },
    ],
    currency,
  );
  await journal(
    x,
    "2026-02-10",
    [
      { accountId: x.accounts.cash, debit: sales, credit: "0" },
      { accountId: x.accounts.revenue, debit: "0", credit: sales },
    ],
    currency,
  );
  if (cost !== "0")
    await journal(
      x,
      "2026-02-12",
      [
        { accountId: x.accounts.expense, debit: cost, credit: "0" },
        { accountId: x.accounts.cash, debit: "0", credit: cost },
      ],
      currency,
    );
}
it("retains annual profit after independently closing two consecutive months and transferring each Company's result at year end", async () => {
  await activity(f, "30", "10");
  await activity(g, "20", "5");
  const periodIds = new Map<string, string[]>();
  for (const x of [f, g]) {
    const profile = await prepareAccountingProfile(
      x.owner,
      {
        statutoryChartId: x.chart,
        standard: "TFRS for NPAEs",
        fiscalStartMonth: 1,
        effectiveDate: "2026-01-01",
        eligibilityEvidence:
          "Synthetic Company eligibility independently assessed for reporting fixture.",
        policyEvidence:
          "Reviewed accrual basis, asset and income accounting policies for this Company.",
        taxBasis: "current_tax",
        inventoryPolicy: "moving_average",
        depreciationPolicy: "straight_line",
      },
      key(),
    );
    await decideAccountingProfile(
      x.manager,
      String(profile.id),
      "approve",
      {
        expectedVersion: 1,
        reason:
          "Independent Company accounting policy review for actual fiscal transfer",
      },
      key(),
    );
    for (const id of [x.period, `${x.period}-feb`]) {
      const started = await beginClose(x.owner, id, key());
      await closePeriod(
        x.manager,
        id,
        {
          expectedVersion: Number(started.version),
          attachmentSetIds: [],
          proof:
            "Independent reconciliation of Company GL, subledgers and bank readiness before close.",
        },
        key(),
      );
    }
    const final = await createPeriod(
        x.owner,
        {
          name: "Remainder of fiscal year",
          startDate: "2026-03-01",
          endDate: "2026-12-31",
        },
        key(),
      ),
      retained = key();
    await db
      .prepare(
        "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,'retained','Retained earnings','equity','THB',1,1)",
      )
      .bind(retained, x.chart, x.company)
      .run();
    const closing = await prepareYearClosing(
      x.owner,
      {
        chartId: x.chart,
        startDate: "2026-01-01",
        endDate: "2026-12-31",
        periodId: String(final.id),
        retainedEarningsAccountId: retained,
        reason:
          "Actual two-month result reconciled before final fiscal transfer into retained earnings.",
      },
      key(),
    );
    expect(closing.profitMinor).toBe(x === f ? 2000 : 1500);
    await decideYearClosing(
      x.manager,
      String(closing.id),
      "post",
      {
        expectedVersion: 1,
        reason:
          "Independent review of the original period result and year-end transfer.",
      },
      key(),
    );
    const started = await beginClose(x.owner, String(final.id), key());
    await closePeriod(
      x.manager,
      String(final.id),
      {
        expectedVersion: Number(started.version),
        attachmentSetIds: [],
        proof:
          "Independent reconciliation after the fiscal transfer and before retained annual group reporting.",
      },
      key(),
    );
    periodIds.set(x.company, [x.period, `${x.period}-feb`, String(final.id)]);
  }
  const base = await definition(false),
    annual = {
      ...base,
      startDate: "2026-01-01",
      endDate: "2026-12-31",
      entities: base.entities.map((e) => ({
        ...e,
        periodIds: periodIds.get(e.companyId)!,
      })),
      reason:
        "Retain annual source profit after actual Company year-end transfers",
    },
    r = await prepareGroupReportEdition(f.owner, annual, key()),
    golden = await result(String(r.id));
  expect(golden.profit).toBe("35.00");
  expect(golden.profitAttribution).toEqual({ parent: "35.00", nci: "0.00" });
  expect(
    golden.balanceSheet.find((a: { code: string }) => a.code === "retained")
      ?.amount,
  ).toBe("-35.00");
  expect(
    golden.profitAndLoss.find(
      (a: { code: string }) => a.code === f.accounts.revenue,
    )?.amount ??
      golden.profitAndLoss.reduce(
        (s: number, a: { category: string; amount: string }) =>
          s + (a.category === "revenue" ? Number(a.amount) : 0),
        0,
      ),
  ).toBe(-50);
});
async function definition(closeFixture = true) {
  if (closeFixture)
    await db
      .prepare(
        "UPDATE finance_periods SET state='closed',close_version=2,closed_by=?,closed_at=1 WHERE company_id IN (?,?)",
      )
      .bind(f.manager.userId, f.company, g.company)
      .run();
  const accounts = (
    await db
      .prepare(
        "SELECT id,code,name,category FROM finance_accounts WHERE company_id IN (?,?) ORDER BY id",
      )
      .bind(f.company, g.company)
      .all<{
        id: string;
        code: string;
        name: string;
        category: "asset" | "equity" | "revenue" | "expense" | "liability";
      }>()
  ).results;
  return {
    startDate: "2026-02-01",
    endDate: "2026-02-28",
    currency: "THB",
    entities: [f, g].map((x) => ({
      companyId: x.company,
      chartId: x.chart,
      periodIds: [`${x.period}-feb`],
      method: "full" as const,
    })),
    mappings: accounts.map((a) => ({
      accountId: a.id,
      group: { code: a.code, name: a.name, category: a.category },
      fxType: (a.category === "equity"
        ? "historical"
        : ["revenue", "expense"].includes(a.category)
          ? "average"
          : "current") as "historical" | "average" | "current",
      historicalRate: a.category === "equity" ? "33" : undefined,
    })),
    reason: "Exact retained February group reporting",
  };
}
async function result(id: string) {
  return JSON.parse(
    String(
      await db
        .prepare(
          "SELECT result_json FROM finance_group_report_editions WHERE id=?",
        )
        .bind(id)
        .first("result_json"),
    ),
  ) as {
    balanceSheet: { code: string; amount: string; category: string }[];
    profitAndLoss: { code: string; amount: string; category: string }[];
    profit: string;
    profitAttribution: {parent: string; nci: string};
  };
}
it("reads retained group drawers by ID with exact interval sources and Company/Holding denial", async () => {
  await activity(f, "30", "10");
  await activity(g, "20", "5");
  const definitionInput = await definition(),
    id = String(
      (await prepareGroupReportEdition(f.owner, definitionInput, key())).id,
    );
  const detail = await groupExpansionRecord(
    f.owner,
    "finance_group_edition",
    id,
  );
  expect(detail.groupData.editions).toHaveLength(1);
  expect(detail.groupData.editions[0].id).toBe(id);
  const code = definitionInput.mappings.find(
    (mapping) => mapping.accountId === f.accounts.revenue,
  )!.group.code;
  const sources = await groupEditionAccountSources(f.owner, id, code, 1);
  expect(sources.rows).toHaveLength(2);
  expect(sources.rows.every((row) => row.entry_date === "2026-02-10")).toBe(
    true,
  );
  const journal = await groupEditionSourceJournal(
    f.owner,
    id,
    String(sources.rows[0].journal_id),
  );
  expect(journal.lines.length).toBeGreaterThan(0);
  await expect(
    groupExpansionRecord(g.owner, "finance_group_edition", id),
  ).rejects.toMatchObject({ status: 404 });
  await expect(
    groupExpansionRecord(
      { ...f.owner, holdingId: "h002" },
      "finance_group_edition",
      id,
    ),
  ).rejects.toMatchObject({ status: 404 });
  await expect(
    groupExpansionRecord(
      { ...f.owner, scope: { ...f.owner.scope, kind: "branch", branchId: f.branch, branchSlug: "branch", branchName: "Branch" } },
      "finance_group_edition",
      id,
    ),
  ).rejects.toMatchObject({ status: 403 });
  await expect(
    groupEditionSourceJournal(f.owner, id, key()),
  ).rejects.toMatchObject({ status: 404 });
});
it("uses cumulative balance-sheet balances and only reporting-interval profit; retains revision export bytes", async () => {
  await activity(f, "30", "10");
  await activity(g, "20", "5");
  const input = await definition(),
    command = key();
  const edition = await prepareGroupReportEdition(f.owner, input, command);
  expect(await prepareGroupReportEdition(f.owner, input, command)).toEqual(
    edition,
  );
  const r = await result(String(edition.id));
  expect(r.balanceSheet).toEqual(
    expect.arrayContaining([
      { code: "cash", name: "cash", category: "asset", amount: "235.00" },
      {
        code: "capital",
        name: "Capital",
        category: "equity",
        amount: "-200.00",
      },
      {
        code: "_unclosed_profit",
        name: "Unclosed Company profit",
        category: "equity",
        amount: "-35.00",
      },
    ]),
  );
  expect(r.profit).toBe("35.00");
  expect(r.profitAndLoss.find((x) => x.code === "revenue")?.amount).toBe(
    "-50.00",
  );
  await decideGroupReportEdition(
    f.manager,
    String(edition.id),
    { expectedVersion: 1, reason: "Independent golden balances checked" },
    "approve",
    key(),
  );
  const original = await groupReportEditionExport(f.owner, String(edition.id));
  const revised = await prepareGroupReportEdition(
    f.owner,
    {
      ...input,
      parentId: String(edition.id),
      parentVersion: 2,
      reason: "Retained revised explanatory edition",
    },
    key(),
  );
  await decideGroupReportEdition(
    f.manager,
    String(revised.id),
    { expectedVersion: 1, reason: "Independent revised edition checked" },
    "approve",
    key(),
  );
  expect(await groupReportEditionExport(f.owner, String(edition.id))).toEqual(
    original,
  );
  await expect(
    db
      .prepare(
        "UPDATE finance_group_report_editions SET export_content='changed' WHERE id=?",
      )
      .bind(edition.id)
      .run(),
  ).rejects.toThrow("GROUP_REPORT_EDITION_IMMUTABLE");
});
it("uses distinct Current, Average and Historical FX with a balancing CTA", async () => {
  await db.batch([
    db
      .prepare("UPDATE finance_charts SET base_currency='USD' WHERE id=?")
      .bind(g.chart),
    db
      .prepare("UPDATE finance_accounts SET currency='USD' WHERE chart_id=?")
      .bind(g.chart),
    db
      .prepare(
        "UPDATE finance_group_entities SET functional_currency='USD',version=version+1 WHERE company_id=?",
      )
      .bind(g.company),
  ]);
  await activity(f, "30", "10");
  await activity(g, "10", "0", "USD");
  const input = await definition();
  await expect(
    prepareGroupReportEdition(f.owner, input, key()),
  ).rejects.toMatchObject({ code: "GROUP_RATE_REQUIRED" });
  const e = await prepareGroupReportEdition(
      f.owner,
      { ...input, rates: [{ currency: "USD", current: "35", average: "34" }] },
      key(),
    ),
    r = await result(String(e.id));
  expect(r.balanceSheet.find((x) => x.code === "cash")?.amount).toBe("3970.00");
  expect(r.balanceSheet.find((x) => x.code === "capital")?.amount).toBe(
    "-3400.00",
  );
  expect(
    r.balanceSheet.find((x) => x.code === "_unclosed_profit")?.amount,
  ).toBe("-360.00");
  expect(r.balanceSheet.find((x) => x.code === "_cta")?.amount).toBe("-210.00");
  expect(r.profit).toBe("360.00");
});
it("includes reversed originals and exact inverses, guards period alignment, stale evidence, self-review and Company scope", async () => {
  const original = await journal(f, "2026-01-10", [
    { accountId: f.accounts.cash, debit: "100", credit: "0" },
    { accountId: f.accounts.revenue, debit: "0", credit: "100" },
  ]);
  await reverseJournal(f.manager, original, key(), {
    periodId: `${f.period}-feb`,
    entryDate: "2026-02-05",
    memo: "Correct the original source",
  });
  const input = await definition(),
    e = await prepareGroupReportEdition(f.owner, input, key()),
    r = await result(String(e.id));
  expect(r.profit).toBe("-100.00");
  expect(r.balanceSheet.find((x) => x.code === "cash")?.amount).toBe("0.00");
  await expect(
    decideGroupReportEdition(
      f.owner,
      String(e.id),
      { expectedVersion: 1, reason: "Self approval is prohibited" },
      "approve",
      key(),
    ),
  ).rejects.toMatchObject({ code: "SELF_REVIEW_FORBIDDEN" });
  await expect(
    prepareGroupReportEdition(
      {
        ...f.owner,
        scope: {
          kind: "branch",
          branchId: f.branch,
          branchSlug: "branch",
          branchName: "Branch",
          holdingId: "h001",
          holdingSlug: "test",
          holdingName: "Test",
          companyId: f.company,
        },
      },
      input,
      key(),
    ),
  ).rejects.toMatchObject({ code: "HOLDING_FINANCE_REQUIRED" });
  await db
    .prepare(
      "UPDATE finance_group_entities SET reporting_name='Changed company',version=version+1 WHERE company_id=?",
    )
    .bind(g.company)
    .run();
  await expect(
    decideGroupReportEdition(
      f.manager,
      String(e.id),
      {
        expectedVersion: 1,
        reason: "Recheck source revisions before approving",
      },
      "approve",
      key(),
    ),
  ).rejects.toMatchObject({ code: "GROUP_EDITION_SOURCE_CHANGED" });
  await expect(
    prepareGroupReportEdition(
      f.owner,
      {
        ...input,
        entities: input.entities.map((x) => ({
          ...x,
          periodIds: [x.companyId === f.company ? f.period : g.period],
        })),
      },
      key(),
    ),
  ).rejects.toMatchObject({ code: "GROUP_PERIODS_NOT_ALIGNED" });
});

it("requires purchased source books for a complete group edition without silently replacing a Company with zero",async()=>{
 await activity(f,"30","10");await activity(g,"20","5");const input=await definition();
 await reviewedFinanceGrant(db,f.company,"finance");await reviewedFinanceGrant(db,f.company,"finance-advanced");vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");
 const command=key();await expect(prepareGroupReportEdition(f.owner,input,command)).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
 expect(await db.prepare("SELECT id FROM finance_group_report_editions WHERE company_id=?").bind(f.company).first()).toBeNull();
 await reviewedFinanceGrant(db,g.company,"finance");
 const edition=await prepareGroupReportEdition(f.owner,input,command),id=String(edition.id);expect((await result(id)).profit).toBe("35.00");
 expect((await groupExpansionRecord(f.owner,"finance_group_edition",id)).groupData.editions).toHaveLength(1);
 const code=input.mappings.find(mapping=>mapping.accountId===f.accounts.revenue)!.group.code,sources=await groupEditionAccountSources(f.owner,id,code,1);expect(sources.rows).toHaveLength(2);
 expect((await groupEditionSourceJournal(f.owner,id,String(sources.rows[0].journal_id))).lines.length).toBeGreaterThan(0);
 await reviewedFinanceGrant(db,f.company,"finance-advanced","draining");
 expect(await prepareGroupReportEdition(f.owner,input,command)).toEqual(edition);
 expect(await decideGroupReportEdition(f.manager,id,{expectedVersion:1,reason:"Independent complete retained Company source review"},"approve",key())).toMatchObject({status:"approved",version:2});
 expect(await groupReportEditionExport(f.owner,id)).toBeDefined();expect(await groupReportEditionWorkspace(f.owner)).toBeDefined();
 const ownershipInput={investorCompanyId:f.company,investeeCompanyId:g.company,method:"subsidiary" as const,ownershipBps:8000,votingBps:8000,controlAssessment:"Reviewed retained subsidiary control",effectiveFrom:"2026-01-01",reason:"Reviewed source ownership evidence"};
 await expect(prepareOwnershipEdition(f.owner,ownershipInput,key())).rejects.toMatchObject({code:"PACKAGE_DRAINING"});
 await reviewedFinanceGrant(db,f.company,"finance-advanced");const interest=await prepareOwnershipEdition(f.owner,ownershipInput,key());
 await decideOwnershipEdition(f.manager,String(interest.id),{expectedVersion:1,reason:"Independent original ownership evidence"},"approve",key());
 expect((await groupExpansionRecord(f.owner,"finance_group_ownership",String(interest.id))).row.id).toBe(interest.id);
 const acquisition=await prepareAcquisitionEdition(f.owner,{ownershipId:String(interest.id),ownershipVersion:2,acquisitionDate:"2026-01-20",currency:"THB",consideration:"100",nciMeasurement:"fair_value",nciFairValue:"20",identifiableNetAssets:"80",cguReference:"Reviewed retained source CGU",reason:"Reviewed original acquisition evidence"},key());
 expect((await groupExpansionRecord(f.owner,"finance_group_acquisition",String(acquisition.id))).row.id).toBe(acquisition.id);

});

it("Finance source purchase checks every retained Company before exposing group Work or private evidence",async()=>{
 await activity(f,"30","10");await activity(g,"20","5");const input=await definition(),edition=await prepareGroupReportEdition(f.owner,input,key()),id=String(edition.id);
 await reviewedFinanceGrant(db,f.company,"finance");await reviewedFinanceGrant(db,f.company,"finance-advanced");vi.stubEnv("ERPX_FINANCE_PACKAGE_ENFORCEMENT","enforced");
 expect((await financeExpansionWork(f.manager)).some(item=>item.id===id)).toBe(false);
 await expect(authorizeAttachmentTarget(db,f.owner,attachmentPolicy("finance_group_report","finance_support","record"),id,"read")).rejects.toMatchObject({code:"PACKAGE_REQUIRED"});
 await reviewedFinanceGrant(db,g.company,"finance");
 expect((await financeExpansionWork(f.manager)).find(item=>item.id===id)).toMatchObject({canAct:true,href:"/finance/group-editions"});
 expect(await authorizeAttachmentTarget(db,f.owner,attachmentPolicy("finance_group_report","finance_support","record"),id,"read")).toMatchObject({companyId:f.company,version:1});
});
