import { expect, it, vi } from "vitest";
import { cp, mkdir, mkdtemp, readdir, rm } from "node:fs/promises";
import { resolve, join } from "node:path";
import { createHash } from "node:crypto";
import { env } from "../../support/runtime-env";
import { migrateDatabase } from "../../../src/server/modules/runtime/adapters/runtime/migrations";
import { seedExpenseCompany } from "../../../fixtures/expenses/company";
import {
  prepareBookRule,
  decideBookRule,
  bookRuleSchema,
  createFinanceDimension,
} from "../../../src/server/modules/finance/public/finance-books";
import {
  prepareAssetPolicy,
  decideAssetPolicy,
  prepareAssetPolicyRun,
  decideAssetPolicyRun,
} from "../../../src/server/modules/finance/public/finance-asset-policies";
import {
  editManualDraft,
  manualDraftEditSchema,
} from "../../../src/server/modules/finance/public/finance-manual-drafts";
import { statementPackageExport } from "../../../src/server/modules/finance/public/finance-reporting";
import { setFinanceCompanyTool } from "../../../src/server/modules/finance/public/finance-company-tools";
vi.mock("../../../src/server/modules/runtime/adapters/runtime", () => ({
  runtimeEnv: async () => env,
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
}));
const hash = (v: string) => createHash("sha256").update(v).digest("hex"),
  key = () => crypto.randomUUID();
it("upgrades a populated pre-expansion Company, preserves posted journals and issued export bytes and reviews legacy asset carrying plus old draft dimensions", async () => {
  const current = resolve("migrations/postgres"),
    old = await mkdtemp(resolve(".runtime/tests/finance-pre-expansion-"));
  try {
    for (const track of ["control", "holding-write"]) {
      await mkdir(join(old, track), { recursive: true });
      for (const file of (await readdir(join(current, track))).filter(
        (n) => n.endsWith(".sql") && Number(n.slice(0, 4)) <= 986,
      ))
        await cp(join(current, track, file), join(old, track, file));
    }
    await migrateDatabase(env.CONTROL_DB, "control", old);
    await migrateDatabase(env.H001_DB, "holding-write", old);
    const db = env.H001_DB,
      f = await seedExpenseCompany(db, env.H001_FILES),
      assetId = key(),
      cost = key(),
      accumulated = key(),
      expense = key(),
      capital = key(),
      acquisition = key(),
      depreciation = key(),
      manual = key(),
      profile = key(),
      issued = key();
    for (const [id, code, category] of [
      [cost, "COST", "asset"],
      [accumulated, "ACC", "asset"],
      [expense, "DEP", "expense"],
      [capital, "CAPITAL", "equity"],
    ])
      await db
        .prepare(
          "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',1,1)",
        )
        .bind(id, f.chart, f.company, code, code, category)
        .run();
    async function legacyJournal(
      id: string,
      source: string,
      sourceId: string | null,
      date: string,
      dr: string,
      cr: string,
      amount: number,
      status = "posted",
    ) {
      await db.batch([
        db
          .prepare(
            "INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,created_by,posted_by,posted_at,created_at,updated_at) VALUES(?,?,?,?,?,?,'THB',1000000,?,?,'Original pre-expansion evidence',?,?,?,?,1,1)",
          )
          .bind(
            id,
            f.company,
            f.chart,
            f.period,
            id,
            date,
            source,
            sourceId,
            status,
            f.reviewer.userId,
            status === "draft" ? null : f.manager.userId,
            status === "draft" ? null : 1,
          ),
        db
          .prepare(
            "INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,ordinal) VALUES(?,?,?, ?,0,?,0,0)",
          )
          .bind(`${id}-debit`, id, dr, amount, amount),
        db
          .prepare(
            "INSERT INTO finance_journal_lines(id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,ordinal) VALUES(?,?,?,0,?,0,?,1)",
          )
          .bind(`${id}-credit`, id, cr, amount, amount),
      ]);
    }
    await legacyJournal(
      acquisition,
      "asset_acquisition",
      assetId,
      "2026-09-01",
      cost,
      capital,
      120001,
    );
    await legacyJournal(
      depreciation,
      "asset_depreciation",
      `${assetId}:2026-09`,
      "2026-09-30",
      expense,
      accumulated,
      40000,
    );
    await legacyJournal(
      manual,
      "manual",
      null,
      "2026-10-31",
      expense,
      capital,
      10001,
      "draft",
    );
    await db
      .prepare(
        "INSERT INTO finance_assets(id,company_id,asset_number,name,account_id,cost_minor,residual_minor,useful_life_months,in_service_date,status,accumulated_depreciation_minor,version,depreciation_account_id,currency,depreciation_expense_account_id,activation_journal_id,activated_by,created_by,created_at,updated_at) VALUES(?,?,?,'Historical machine',?,120001,0,3,'2026-09-01','in_service',40000,3,?,'THB',?,?,?, ?,1,1)",
      )
      .bind(
        assetId,
        f.company,
        assetId,
        cost,
        accumulated,
        expense,
        acquisition,
        f.manager.userId,
        f.reviewer.userId,
      )
      .run();
    await db
      .prepare(
        "INSERT INTO finance_asset_depreciations(id,asset_id,company_id,depreciation_month,ordinal,period_id,entry_date,amount_minor,journal_id,asset_version,command_id,created_at) VALUES(?,?,?,'2026-09',1,?,'2026-09-30',40000,?,3,?,1)",
      )
      .bind(key(), assetId, f.company, f.period, depreciation, key())
      .run();
    await db
      .prepare(
        "INSERT INTO finance_accounting_profiles(id,company_id,statutory_chart_id,standard,fiscal_start_month,effective_date,eligibility_evidence,policy_evidence,tax_basis,inventory_policy,depreciation_policy,status,version,prepared_by,reviewed_by,created_at) VALUES(?,?,?,'TFRS for NPAEs',1,'2026-01-01','Retained fictional eligibility','Retained fictional policy','current_tax','moving_average','straight_line','approved',2,?,?,1)",
      )
      .bind(profile, f.company, f.chart, f.reviewer.userId, f.manager.userId)
      .run();
    const html =
        "<!doctype html><html><body><h1>Original issued Company edition</h1><p>Asset cost 1200.01; retained depreciation 400.00.</p></body></html>",
      exportHash = hash(html);
    await db
      .prepare(
        "INSERT INTO finance_statement_packages(id,company_id,chart_id,profile_id,profile_version,start_date,end_date,comparative_start_date,comparative_end_date,edition,source_revision,source_fingerprint,presentation_json,notes_json,snapshot_json,status,reason,prepared_by,reviewed_by,export_html,export_sha256,version,created_at,updated_at) VALUES(?,?,?,?,2,'2026-09-01','2026-09-30','2025-09-01','2025-09-30',1,1,?,'{}','{}','{}','issued','Original synthetic issued migration fixture',?,?,?,?,2,1,1)",
      )
      .bind(
        issued,
        f.company,
        f.chart,
        profile,
        hash("original immutable source"),
        f.reviewer.userId,
        f.manager.userId,
        html,
        exportHash,
      )
      .run();
    const original = async () => ({
      journals: (
        await db
          .prepare(
            "SELECT * FROM finance_journals WHERE id IN (?,?) ORDER BY id",
          )
          .bind(acquisition, depreciation)
          .all()
      ).results,
      lines: (
        await db
          .prepare(
            "SELECT id,journal_id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,ordinal FROM finance_journal_lines WHERE journal_id IN (?,?) ORDER BY id",
          )
          .bind(acquisition, depreciation)
          .all()
      ).results,
      events: (
        await db
          .prepare(
            "SELECT * FROM finance_financial_events WHERE journal_id IN (?,?) ORDER BY id",
          )
          .bind(acquisition, depreciation)
          .all()
      ).results,
      legacy: (
        await db
          .prepare("SELECT * FROM finance_asset_depreciations WHERE asset_id=?")
          .bind(assetId)
          .all()
      ).results,
      export: await statementPackageExport(f.reviewer, issued),
    });
    const before = await original();
    expect(before.export).toEqual({
      content: html,
      sha256: exportHash,
      edition: 1,
    });
    await migrateDatabase(env.CONTROL_DB, "control", current);
    await migrateDatabase(db, "holding-write", current);
    expect(await original()).toEqual(before);
    await setFinanceCompanyTool(
      f.reviewer,
      {
        tool: "asset_policies",
        enabled: true,
        expectedVersion: 0,
        reason: "Review legacy carrying evidence after additive migration",
      },
      key(),
    );
    const secondary = key();
    await db
      .prepare(
        "INSERT INTO finance_charts(id,company_id,name,base_currency,created_by,created_at,updated_at) VALUES(?,?,'Secondary reviewed opening','THB',?,1,1)",
      )
      .bind(secondary, f.company, f.reviewer.userId)
      .run();
    const mappings = [];
    for (const a of (
      await db
        .prepare("SELECT id FROM finance_accounts WHERE chart_id=? ORDER BY id")
        .bind(f.chart)
        .all<{ id: string }>()
    ).results) {
      const to = key();
      await db
        .prepare(
          "INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at) SELECT ?,?,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at FROM finance_accounts WHERE id=?",
        )
        .bind(to, secondary, a.id)
        .run();
      mappings.push({ primaryAccountId: a.id, secondaryAccountId: to });
    }
    const book = await prepareBookRule(
      f.reviewer,
      bookRuleSchema.parse({
        primaryChartId: f.chart,
        secondaryChartId: secondary,
        effectiveDate: "2026-10-01",
        openingPeriodId: f.period,
        mappings,
        reason: "Review original posted opening after upgrade",
      }),
      key(),
    );
    await decideBookRule(
      f.manager,
      String(book.id),
      {
        expectedVersion: 1,
        reason: "Independent original book carrying review",
      },
      "approve",
      key(),
    );
    const policy = await prepareAssetPolicy(
      f.reviewer,
      {
        assetId,
        expectedVersion: 3,
        adoptLegacy: true,
        effectiveDate: "2026-10-01",
        books: [
          { chartId: f.chart, policy: { method: "straight_line" } },
          { chartId: secondary, policy: { method: "straight_line" } },
        ],
        reason:
          "Adopt the immutable original asset and independently reviewed book opening",
      },
      key(),
    );
    await decideAssetPolicy(
      f.manager,
      String(policy.id),
      {
        expectedVersion: 1,
        reason: "Independent legacy source and carrying review",
      },
      "approve",
      key(),
    );
    const run = await prepareAssetPolicyRun(
      f.reviewer,
      String(policy.id),
      {
        expectedVersion: 2,
        periodId: f.period,
        entryDate: "2026-10-31",
        reason: "Continue October from the reviewed retained basis",
      },
      key(),
    );
    expect(run.result.books.map((b) => b.amount)).toEqual(["400.00", "400.00"]);
    await decideAssetPolicyRun(
      f.manager,
      String(run.id),
      {
        expectedVersion: 1,
        reason: "Independent remaining carrying and period review",
      },
      "post",
      key(),
    );
    await createFinanceDimension(
      f.reviewer,
      {
        code: "project",
        name: "Project",
        required: true,
        values: [{ code: "UPGRADE", name: "Reviewed old draft source" }],
      },
      key(),
    );
    await editManualDraft(
      f.reviewer,
      manual,
      manualDraftEditSchema.parse({
        expectedVersion: 1,
        chartId: f.chart,
        periodId: f.period,
        entryDate: "2026-10-31",
        currency: "THB",
        memo: "Fill the new required dimension on the old draft",
        reason: "Retain the old draft and add its reviewed reporting dimension",
        lines: [
          {
            accountId: expense,
            debit: "100.01",
            credit: "0",
            dimensions: { project: "UPGRADE" },
          },
          {
            accountId: capital,
            debit: "0",
            credit: "100.01",
            dimensions: { project: "UPGRADE" },
          },
        ],
      }),
      key(),
    );
    expect(await original()).toEqual(before);
    expect(
      await db
        .prepare(
          "SELECT count(*) n FROM finance_manual_draft_editions WHERE journal_id=?",
        )
        .bind(manual)
        .first("n"),
    ).toBe(2);
    await migrateDatabase(db, "holding-write", current);
    expect(await original()).toEqual(before);
  } finally {
    await rm(old, { recursive: true, force: true });
  }
}, 60000);
