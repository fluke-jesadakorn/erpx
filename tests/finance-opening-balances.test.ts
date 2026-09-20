import { beforeAll, describe, expect, it, vi } from "vitest";
import { env, applyMigrations } from "./runtime-env";
import type { WorkContext } from "../src/server/authorization";
import {
  importOpeningTrialBalance,
  previewOpeningTrialBalance,
} from "../src/server/finance";

vi.mock("../src/server/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));

const db = env.H001_DB;
const actor: WorkContext = {
  userId: "opening-preparer",
  assignmentId: "opening-assignment",
  role: "finance_preparer",
  companyId: "opening-company",
  branchId: null,
  departmentId: null,
} as WorkContext;

const definition = {
  chartId: "opening-chart",
  periodId: "opening-period",
  entryDate: "2026-01-01",
  currency: "THB",
  lines: [
    { accountId: "opening-cash", debit: "1000.00", credit: "0", description: "Imported cash" },
    { accountId: "opening-equity", debit: "0", credit: "1000.00", description: "Opening equity" },
  ],
};

beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
  await db.exec(`
    INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES('opening-company','company','opening-company','Opening company','opening-company');
    INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,scope_kind) VALUES('opening-assignment','opening-preparer','finance_preparer','["finance"]','opening-company',0,9999999999999,'holding');
    INSERT INTO finance_charts(id,company_id,name,base_currency,accounting_basis,status,created_by,created_at,updated_at) VALUES('opening-chart','opening-company','Opening chart','THB','IFRS','active','opening-preparer',1,1);
    INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES
      ('opening-cash','opening-chart','opening-company','1000','Cash','asset','THB',1,1),
      ('opening-equity','opening-chart','opening-company','3000','Opening equity','equity','THB',1,1);
    INSERT INTO finance_periods(id,company_id,name,start_date,end_date,created_at) VALUES('opening-period','opening-company','Opening 2026','2026-01-01','2026-12-31',1);
  `);
});

describe("opening trial balance", () => {
  it("previews normalized lines and imports one auditable draft", async () => {
    const preview = await previewOpeningTrialBalance(actor, definition);
    expect(preview).toMatchObject({ debit: "1000.00", credit: "1000.00", currency: "THB" });
    expect(preview.previewToken).toHaveLength(64);
    expect(preview.lines[0]).toMatchObject({ accountCode: "1000", accountName: "Cash" });

    const input = { ...definition, previewToken: preview.previewToken };
    const created = await importOpeningTrialBalance(actor, input, "opening-import-one");
    expect(created).toMatchObject({ status: "draft", debit: "1000.00", credit: "1000.00" });
    expect(await importOpeningTrialBalance(actor, input, "opening-import-one")).toEqual(created);
    expect(await db.prepare("SELECT source_type,is_opening,status FROM finance_journals WHERE id=?").bind(created.id).first()).toEqual({ source_type: "opening_import", is_opening: 1, status: "draft" });
    expect(await db.prepare("SELECT COUNT(*) n FROM audit_events WHERE action='finance.journal.create'").first()).toEqual({ n: 1 });
  });

  it("rejects invalid and stale previews", async () => {
    await expect(previewOpeningTrialBalance(actor, { ...definition, chartId: "opening-chart-invalid", lines: definition.lines })).rejects.toMatchObject({ code: "CHART_NOT_FOUND" });
    await expect(previewOpeningTrialBalance(actor, { ...definition, lines: definition.lines.map((line, index) => ({ ...line, credit: "0", debit: index ? "1.00" : line.debit })) })).rejects.toMatchObject({ code: "UNBALANCED_OPENING_BALANCE" });

    const otherChart = "opening-chart-stale";
    await db.exec(`
      INSERT INTO finance_charts(id,company_id,name,base_currency,accounting_basis,status,created_by,created_at,updated_at) VALUES('${otherChart}','opening-company','Stale chart','THB','IFRS','active','opening-preparer',1,1);
      INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES
        ('stale-cash','${otherChart}','opening-company','1000','Cash','asset','THB',1,1),
        ('stale-equity','${otherChart}','opening-company','3000','Equity','equity','THB',1,1);
    `);
    const staleDefinition = { ...definition, chartId: otherChart, lines: [{ ...definition.lines[0], accountId: "stale-cash" }, { ...definition.lines[1], accountId: "stale-equity" }] };
    const preview = await previewOpeningTrialBalance(actor, staleDefinition);
    await db.prepare("UPDATE finance_accounts SET updated_at=2 WHERE id='stale-cash'").run();
    await expect(importOpeningTrialBalance(actor, { ...staleDefinition, previewToken: preview.previewToken }, "opening-import-stale")).rejects.toMatchObject({ code: "OPENING_PREVIEW_STALE" });
  });

  it("allows only one concurrent opening import per chart", async () => {
    const chartId = "opening-chart-race";
    await db.exec(`
      INSERT INTO finance_charts(id,company_id,name,base_currency,accounting_basis,status,created_by,created_at,updated_at) VALUES('${chartId}','opening-company','Race chart','THB','IFRS','active','opening-preparer',1,1);
      INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES
        ('race-cash','${chartId}','opening-company','1000','Cash','asset','THB',1,1),
        ('race-equity','${chartId}','opening-company','3000','Equity','equity','THB',1,1);
    `);
    const input = { ...definition, chartId, lines: [{ ...definition.lines[0], accountId: "race-cash" }, { ...definition.lines[1], accountId: "race-equity" }] };
    const preview = await previewOpeningTrialBalance(actor, input);
    const results = await Promise.allSettled([
      importOpeningTrialBalance(actor, { ...input, previewToken: preview.previewToken }, "opening-import-race-a"),
      importOpeningTrialBalance(actor, { ...input, previewToken: preview.previewToken }, "opening-import-race-b"),
    ]);
    expect(results.filter((result) => result.status === "fulfilled")).toHaveLength(1);
    expect(await db.prepare("SELECT COUNT(*) n FROM finance_journals WHERE chart_id=? AND is_opening=1 AND status='draft'").bind(chartId).first()).toEqual({ n: 1 });
  });
});
