/** Preview/import fictional bank evidence for the Acme UI rehearsal, never real money. */
import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { mock } from "bun:test";
import { assertHospitalityDevelopmentTarget } from "@erpx/feature-shared/lib/hospitality-seed-target";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
mock.module("next/navigation", () => ({ forbidden: () => { throw new Error("CLI_PAGE_FORBIDDEN"); }, redirect: () => { throw new Error("CLI_REDIRECT_FORBIDDEN"); } }));
const manifestPath = process.env.ERPX_DATABASE_MANIFEST;
if (!manifestPath) throw new Error("ERPX_DATABASE_MANIFEST_REQUIRED");
const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
const jobsPath = resolve(dirname(manifestPath), "databases-jobs.json");
const jobs = JSON.parse(await readFile(jobsPath, "utf8"));
for (const target of [manifest, jobs]) assertHospitalityDevelopmentTarget(target, process.env.ERPX_ENVIRONMENT);
process.env.ERPX_DATABASE_MANIFEST = jobsPath;
process.env.ERPX_PROCESS_ROLE = "jobs";
const { getRuntime, closeRuntime } = await import("@erpx/server-runtime/adapters/runtime/env");
const { importBankStatement } = await import("@erpx/server-finance/public/finance");
const db = getRuntime().H001_DB;
const guide = JSON.parse(await readFile(".scratch/dev/hospitality-acme/scenario-guide.json", "utf8"));
if (guide.companyId !== "acme-co" || guide.branchId !== "bangkok") throw new Error("ACME_BASELINE_REQUIRED");
const actor: WorkContext = {
  userId: "acme-hospitality-v1-financePreparer", assignmentId: "acme-hospitality-v1-financePreparer-access", role: "finance_preparer",
  companyId: "acme-co", branchId: null, departmentId: null, holdingId: "h001", holdingSlug: "acme", holdingName: "ACME",
  name: "Fictional Acme Finance preparer", email: "financepreparer@acme-hospitality-v1.example", assignmentScope: "holding", inherited: false,
  modules: ["finance"], assignments: [], workspaces: [],
  scope: { kind: "holding", holdingId: "h001", holdingSlug: "acme", holdingName: "ACME", companyId: "acme-co" },
};
try {
  const bank = await db.prepare("SELECT p.business_date,a.id,a.code,a.currency,a.control_kind,c.base_currency FROM pms_properties p JOIN finance_accounts a ON a.id=p.config_json::jsonb->'accounts'->>'bank' JOIN finance_charts c ON c.id=a.chart_id WHERE p.id=? AND p.company_id=?").bind(guide.propertyId, guide.companyId).first<{ business_date: string; id: string; code: string; currency: string; control_kind: string; base_currency: string }>();
  if (!bank || bank.code !== "DEMO-PMS-BANK" || bank.currency !== "THB" || bank.base_currency !== "THB" || bank.control_kind !== "bank" || bank.business_date !== "2026-10-02") throw new Error("REHEARSAL_BANK_AND_DATE_REQUIRED");
  const input = {
    bankAccount: bank.id, statementNumber: "ACME-UI-MOCK-20261002", statementDate: bank.business_date, opening: "0.00", closing: "400.00", sourceHash: "fictional-acme-ui-bank-rehearsal-20261002",
    rows: [
      { externalId: "ACME-UI-MOCK-TRANSFER-20261002", transactionDate: bank.business_date, description: "Fictional Noah security deposit; no real bank transfer", amount: "500.00", currency: "THB" },
      { externalId: "ACME-UI-MOCK-CASH-20261002", transactionDate: bank.business_date, description: "Fictional withdrawal to rehearsal till for partial deposit refund", amount: "-100.00", currency: "THB" },
    ],
  };
  console.log(JSON.stringify({ developmentOnly: true, companyId: guide.companyId, ...input, policy: "Only fictional statement evidence; no payment, refund, permissions or existing-record changes." }));
  if (process.argv.includes("--apply")) console.log(JSON.stringify(await importBankStatement(actor, input, "acme-ui-mock-bank-20261002")));
} finally { await closeRuntime(); }
