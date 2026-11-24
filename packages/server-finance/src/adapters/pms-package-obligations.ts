import "server-only";
import { createHash } from "node:crypto";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { packageSettlementSource, type PackageSettlementSourceLock } from "@erpx/server-commercial/public/package-settlement-sources";
import { assertPmsPackageObligationSources, pmsPackageSettlementSources as pms } from "@erpx/server-pms/public/package-obligations";
import { packageObligationFactsSql } from "./finance-read-tail-r24-sql";
type Input = { companyId: string; packageId: string; packageVersion: number; moduleId: string; now: number };
const sources = new WeakMap<SqlDatabase, string>();
export async function lockPmsFinancePackageObligationSources(owner: PostgresDatabase, lock: PackageSettlementSourceLock) {
  const { database, companyId } = packageSettlementSource(owner, lock);
  await database.prepare("LOCK TABLE finance_invoices,finance_payments,finance_journals,finance_journal_lines,finance_allocations IN SHARE MODE").run();
  sources.set(database, companyId);
}
export async function pmsFinancePackageObligations(owner: PostgresDatabase, database: SqlDatabase, input: Input) {
  owner.assertActiveTransaction(database);
  if (sources.get(database) !== input.companyId) throw new Error("PMS_FINANCE_PACKAGE_OBLIGATION_SOURCE_REQUIRED");
  assertPmsPackageObligationSources(owner, database, input.companyId);
  if (input.moduleId !== "finance" || input.packageId !== "pms" || input.packageVersion !== 1 || !Number.isSafeInteger(input.now) || input.now < 0)
    throw new Error("PMS_FINANCE_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
  const facts = (await database.prepare(packageObligationFactsSql({ sourceType: "pms", payments: pms.payments, paymentFk: "finance_id", postingKey: "concat(pp.property_id,':',pp.source_key)", postings: pms.postings, postingGroup: "pp.property_id,pp.source_key", clearingFrom: "FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id,scope", clearingWhere: `EXISTS(SELECT 1 FROM (${pms.clearing}) c WHERE c.company_id=j.company_id AND c.account_id=l.account_id)` })).bind(input.companyId).all()).results;
  return { companyId: input.companyId, packageId: input.packageId, moduleId: "finance", observedAt: input.now,
    revision: createHash("sha256").update(JSON.stringify({ companyId: input.companyId, packageVersion: input.packageVersion, facts })).digest("hex"), outstanding: facts.length };
}
