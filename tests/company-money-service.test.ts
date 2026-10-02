import { beforeAll, describe, expect, it, vi } from "vitest";
import { applyMigrations, env } from "./runtime-env";
import type { WorkContext } from "../src/server/authorization";
import { companyRates, configureCurrencyScale, confirmManualRate, quoteMoneyConversion, saveCurrencyPreference, snapshotMoney } from "../src/server/company-money";

vi.mock("../src/server/runtime", async () => {
  const { env } = await import("./runtime-env");
  return { holdingResources: async () => ({ write: env.H001_DB }) };
});

const db = env.H001_DB;
const approver = { holdingId: "h001", companyId: "money-company-a", userId: "money-approver", assignmentId: "money-assignment-a", role: "finance_approver", branchId: null, departmentId: null } as WorkContext;
const otherCompany = { ...approver, companyId: "money-company-b", assignmentId: "money-assignment-b" } as WorkContext;
const preparer = { ...approver, userId: "money-preparer", assignmentId: "money-assignment-preparer", role: "finance_preparer" } as WorkContext;

beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
  await db.exec(`
    INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES
      ('money-company-a','company','money-company-a','Money A','money-company-a'),
      ('money-company-b','company','money-company-b','Money B','money-company-b');
    INSERT INTO assignments(id,user_id,role,modules_json,company_id,scope_kind,valid_from,valid_until) VALUES
      ('money-assignment-a','money-approver','finance_approver','["finance"]','money-company-a','holding',0,9999999999999),
      ('money-assignment-b','money-approver','finance_approver','["finance"]','money-company-b','holding',0,9999999999999),
      ('money-assignment-preparer','money-preparer','finance_preparer','["finance"]','money-company-a','holding',0,9999999999999);
  `);
});

describe("company manual currency persistence", () => {
  it("scopes preferences and the custom ISO precision to one company", async () => {
    expect(await db.prepare("SELECT default_currency FROM company_money_settings WHERE company_id=?").bind(approver.companyId).first()).toEqual({ default_currency: "USD" });
    await saveCurrencyPreference(approver, { currency: "KWD" }, "money-pref-001");
    await configureCurrencyScale(approver, { currency: "XAU", scale: 6 }, "money-scale-001");
    expect(await db.prepare("SELECT display_currency FROM user_currency_preferences WHERE company_id=? AND user_id=?").bind(approver.companyId, approver.userId).first()).toEqual({ display_currency: "KWD" });
    expect(await db.prepare("SELECT display_currency FROM user_currency_preferences WHERE company_id=? AND user_id=?").bind(otherCompany.companyId, approver.userId).first()).toBeNull();
    expect(await db.prepare("SELECT scale FROM company_currency_scales WHERE company_id=? AND currency='XAU'").bind(otherCompany.companyId).first()).toBeNull();
    await expect(configureCurrencyScale(approver, { currency: "XAU", scale: 2 }, "money-scale-002")).rejects.toMatchObject({ code: "CURRENCY_SCALE_LOCKED" });
    await expect(configureCurrencyScale(preparer, { currency: "XAU", scale: 6 }, "money-scale-003")).rejects.toMatchObject({ status: 403 });
    await expect(configureCurrencyScale({ ...approver, branchId: "branch-1" }, { currency: "XTS", scale: 2 }, "money-scale-branch")).rejects.toMatchObject({ code: "COMPANY_FINANCE_REQUIRED" });
  });

  it("keeps rate revisions and transaction snapshots immutable across a later correction", async () => {
    await expect(confirmManualRate(preparer, { currency: "THB", effectiveDate: "2026-09-29", currencyPerUsd: "36", note: "unauthorized" }, "money-rate-denied")).rejects.toMatchObject({ status: 403 });
    await expect(confirmManualRate({ ...approver, branchId: "branch-1" }, { currency: "THB", effectiveDate: "2026-09-29", currencyPerUsd: "36", note: "branch" }, "money-rate-branch")).rejects.toMatchObject({ code: "COMPANY_FINANCE_REQUIRED" });
    const first = await confirmManualRate(approver, { currency: "THB", effectiveDate: "2026-09-29", currencyPerUsd: "36", note: "manual rate" }, "money-rate-001");
    expect((await companyRates(db, approver.companyId))[0].id).toBe(first.id);
    const quote = await quoteMoneyConversion(approver, { amount: "100.00", currency: "USD", targetCurrency: "THB", transactionDate: "2026-09-29", requireRate: true });
    expect(quote.converted).toMatchObject({ amount: "3600.00", currency: "THB", targetRateVersion: first.id });
    expect((await quoteMoneyConversion(otherCompany, { amount: "100.00", currency: "USD", targetCurrency: "THB", transactionDate: "2026-09-29", requireRate: false })).converted).toBeNull();
    expect((await quoteMoneyConversion(otherCompany, { amount: "100.00", currency: "THB", targetCurrency: "THB", transactionDate: "2026-09-29", requireRate: true })).converted).toMatchObject({ amount: "100.00", sourceRateVersion: null });
    await expect(quoteMoneyConversion(otherCompany, { amount: "100.00", currency: "USD", targetCurrency: "THB", transactionDate: "2026-09-29", requireRate: true })).rejects.toMatchObject({ code: "MANUAL_RATE_REQUIRED" });

    const snapshot = await snapshotMoney(db, approver, { sourceType: "test_sale", sourceId: "sale-1", amountKind: "total", transactionDate: "2026-09-29", currency: "THB", amountUnits: "360000" });
    await db.batch([snapshot.statement]);
    const second = await confirmManualRate(approver, { currency: "THB", effectiveDate: "2026-09-29", currencyPerUsd: "37", note: "corrected" }, "money-rate-002");
    expect(second.id).not.toBe(first.id);
    const recorded = await db.prepare("SELECT currency,amount_units::text,usd_units::text,fx_rate_id FROM money_transaction_snapshots WHERE company_id=? AND source_id='sale-1'").bind(approver.companyId).first();
    expect(recorded).toEqual({ currency: "THB", amount_units: "360000", usd_units: "10000", fx_rate_id: first.id });
    await expect(db.prepare("UPDATE company_fx_rates SET currency_per_usd=38 WHERE id=?").bind(first.id).run()).rejects.toMatchObject({ code: "23514" });
    await expect(db.prepare("UPDATE money_transaction_snapshots SET usd_units=1 WHERE source_id='sale-1'").run()).rejects.toMatchObject({ code: "23514" });
  });
});
