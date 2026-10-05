import { beforeAll, expect, it, vi } from "vitest";
import { applyMigrations, env } from "./runtime-env";
import { seedFinanceExpansionRehearsal } from "../fixtures/finance/expansion";
import { statements } from "../src/server/finance";
import { key } from "./helpers/expense-fixture";
import { prepareFinancialReport, financialReportSchema, decideFinancePlan, snapshotFinancialReport, exportFinancialReport, prepareBudgetEdition, budgetEditionSchema, budgetActuals, budgetSourceRecords, financialReportSourceRecords } from "../src/server/finance-planning";
import { prepareGroupReportEdition, groupReportEditionSchema, decideGroupReportEdition, groupReportEditionExport, groupEditionAccountSources } from "../src/server/finance-group-editions";
vi.mock("../src/server/runtime", () => ({
  runtimeEnv: async () => env,
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
}));
beforeAll(async () => {
  await applyMigrations(env.H001_DB, env.STAFF_TEST_MIGRATIONS);
});
it("reconciles a populated multi-Company, multi-book nine-group rehearsal through two consecutive closes, cross-period billing correction and year close", async () => {
  const r = await seedFinanceExpansionRehearsal(env.H001_DB, env.H001_FILES);
  // Independent money golden: 100 bank revenue + 80.01 corrected service
  // - 1200.01 asset - 100 lease depreciation - 3 loan interest = -1123.
  expect(r.yearProfit).toBe(-112300);
  expect(r.reconciliation.ready).toBe(true);
  expect(r.reconciliation.rows.every((x) => x.differenceMinor === 0)).toBe(
    true,
  );
  for (const balances of [r.balances, r.secondaryBalances]) {
    const code = (key: string) => balances.find((x) => x.code === key)?.balance;
    expect(code("cash")).toBe("-3.00");
    expect(code("AR")).toBe("80.01");
    expect(code("DEBT")).toBe("0.00");
    expect(code("DEFERRED")).toBe("0.00");
    expect(code("RETAINED")).toBe("1123.00");
    expect(
      balances.reduce((s, x) => s + Math.round(Number(x.balance) * 100), 0),
    ).toBe(0);
  }
  expect(r.reportExport.content).toContain("-1123.00");
  expect(r.groupExport.content).toContain("-1123.00");
  // A fiscal transfer changes cumulative equity, never the year's earned result.
  // Both books must report the independently computed loss after that transfer.
  for (const chartId of [r.f.chart, r.secondary]) {
    const report = await statements(r.preparer, undefined, chartId, { startDate: "2026-01-01", endDate: "2026-12-31" });
    expect(report.profitAndLoss.netProfit).toBe("-1123.00");
    expect(report.balanceSheet.unclosedProfit).toBe("0.00");
    expect(report.balanceSheet.assets).toBe("77.01");
    expect(report.balanceSheet.liabilities).toBe("0.00");
    expect(report.balanceSheet.equity).toBe("77.01");
  }
  const mapped = (await env.H001_DB.prepare("SELECT p.id primary_id,s.id secondary_id FROM finance_accounts p JOIN finance_accounts s ON s.code=p.code AND s.company_id=p.company_id WHERE p.chart_id=? AND s.chart_id=?").bind(r.f.chart,r.secondary).all<{primary_id:string;secondary_id:string}>()).results;
  const toSecondary = new Map(mapped.map(row=>[row.primary_id,row.secondary_id]));
  const definition = JSON.parse(String(await env.H001_DB.prepare("SELECT content_json FROM finance_plan_editions WHERE id=?").bind(r.reportId).first("content_json")));
  const secondaryReport = await prepareFinancialReport(r.preparer,financialReportSchema.parse({...definition,chartId:r.secondary,name:"Reviewed secondary result after fiscal transfer",rows:definition.rows.map((row:{accountIds?:string[]})=>({...row,...(row.accountIds?{accountIds:row.accountIds.map(id=>toSecondary.get(id))}:{})}))}),key());
  await decideFinancePlan(r.approver,String(secondaryReport.id),{expectedVersion:1,reason:"Independent year-close result and source classification review"},"approve",key());
  const afterClose = await snapshotFinancialReport(r.preparer,String(secondaryReport.id),{startDate:"2026-01-01",endDate:"2026-12-31"},key());
  expect(afterClose.rows.find(row=>row.key==='profit')?.value).toBe('-1123.00');
  const sources = await financialReportSourceRecords(r.preparer,afterClose.id,{key:'expense'});
  const closingMirrors = (await env.H001_DB.prepare("SELECT id FROM finance_journals WHERE chart_id=? AND source_id IN (SELECT id FROM finance_journals WHERE company_id=? AND source_type='fiscal_close')").bind(r.secondary,r.f.company).all<{id:string}>()).results;
  expect(closingMirrors.length).toBe(1);
  expect(sources.rows.some(row=>row.journal_id===closingMirrors[0].id)).toBe(false);
  const decemberBudget = await prepareBudgetEdition(r.preparer,budgetEditionSchema.parse({chartId:r.secondary,name:'December expenses excluding fiscal transfer',year:2026,lines:[{accountId:toSecondary.get(r.account.DEP),month:'2026-12',amount:'0.00'}]}),key());
  await decideFinancePlan(r.approver,String(decemberBudget.id),{expectedVersion:1,reason:'Review the December service window independently'},'approve',key());
  expect((await budgetActuals(r.preparer,String(decemberBudget.id))).lines[0].actual).toBe('0.00');
  expect((await budgetSourceRecords(r.preparer,String(decemberBudget.id),0)).rows).toHaveLength(0);
  const groupDefinition = JSON.parse(String(await env.H001_DB.prepare('SELECT content_json FROM finance_group_report_editions WHERE id=?').bind(r.groupId).first('content_json')));
  const secondaryGroup = await prepareGroupReportEdition(r.f.owner,groupReportEditionSchema.parse({...groupDefinition,parentId:r.groupId,parentVersion:2,entities:groupDefinition.entities.map((entity:{chartId:string})=>({...entity,chartId:entity.chartId===r.f.chart?r.secondary:entity.chartId})),mappings:groupDefinition.mappings.map((mapping:{accountId:string})=>({...mapping,accountId:toSecondary.get(mapping.accountId)??mapping.accountId})),reason:'Review the secondary book after the retained fiscal transfer'}),key());
  await decideGroupReportEdition(r.approver,String(secondaryGroup.id),{expectedVersion:1,reason:'Independent secondary year profit and cumulative equity review'},'approve',key());
  expect((await groupReportEditionExport(r.f.owner,String(secondaryGroup.id))).content).toContain('-1123.00');
  const groupSources = await groupEditionAccountSources(r.f.owner,String(secondaryGroup.id),'DEP');
  expect(groupSources.rows.some(row=>row.journal_id===closingMirrors[0].id)).toBe(false);
  // Issued primary exports stay byte-for-byte identical after a book revision.
  expect((await exportFinancialReport(r.preparer,r.snapshotId)).content).toBe(r.reportExport.content);
  expect((await groupReportEditionExport(r.f.owner,r.groupId)).content).toBe(r.groupExport.content);
  for (const id of [r.loanId, r.leaseId])
    expect(
      await env.H001_DB.prepare(
        "SELECT status FROM finance_instrument_editions WHERE id=?",
      )
        .bind(id)
        .first("status"),
    ).toBe("closed");
  expect(
    await env.H001_DB.prepare(
      "SELECT status FROM finance_due_drafts WHERE id=?",
    )
      .bind(r.scheduledId)
      .first("status"),
  ).toBe("prepared");
  expect(
    await env.H001_DB.prepare(
      "SELECT status FROM finance_collection_cases WHERE id=?",
    )
      .bind(r.collectionId)
      .first("status"),
  ).toBe("test_delivered");
  expect(
    await env.H001_DB.prepare(
      "SELECT status,reviewed_by FROM finance_collection_cases WHERE id=?",
    )
      .bind(r.originalCollectionId)
      .first(),
  ).toEqual({ status: "superseded", reviewed_by: r.f.manager.userId });
  expect(
    await env.H001_DB.prepare(
      "SELECT root_id,revision_number FROM finance_collection_cases WHERE id=?",
    )
      .bind(r.collectionId)
      .first(),
  ).toEqual({ root_id: r.originalCollectionId, revision_number: 2 });
  expect(
    await env.H001_DB.prepare(
      "SELECT count(*) n FROM finance_periods WHERE company_id=? AND state!='closed'",
    )
      .bind(r.f.company)
      .first("n"),
  ).toBe(0);
  expect(
    await env.H001_DB.prepare(
      "SELECT status FROM finance_billing_corrections WHERE id=?",
    )
      .bind(r.correctionId)
      .first("status"),
  ).toBe("posted");
}, 60000);
