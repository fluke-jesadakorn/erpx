import "server-only";
import { createHash } from "node:crypto";
import type { PostgresDatabase } from "@erpx/platform-runtime-server/public/runtime/postgres";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { packageSettlementSource, type PackageSettlementSourceLock } from "@erpx/commercial-server/public/package-settlement-sources";
import { assertPosPackageObligationSources, posPackageSettlementSources as pos } from "@erpx/pos-server/public/package-obligations";
import { packageObligationFactsSql } from "./finance-read-tail-r24-sql";
type Input = { companyId: string; packageId: string; packageVersion: number; moduleId: string; now: number };
const sources = new WeakMap<SqlDatabase, string>();
export async function lockPosFinancePackageObligationSources(owner: PostgresDatabase, lock: PackageSettlementSourceLock) {
  const { database, companyId } = packageSettlementSource(owner, lock);
  await database.prepare("LOCK TABLE finance_invoices,finance_payments,finance_journals,finance_journal_lines,finance_allocations IN SHARE MODE").run();
  sources.set(database, companyId);
}
export async function posFinancePackageObligations(owner: PostgresDatabase, database: SqlDatabase, input: Input) {
  owner.assertActiveTransaction(database);
  if (sources.get(database) !== input.companyId) throw new Error("POS_FINANCE_PACKAGE_OBLIGATION_SOURCE_REQUIRED");
  assertPosPackageObligationSources(owner, database, input.companyId);
  if (input.moduleId !== "finance" || input.packageId !== "pos" || input.packageVersion !== 1 || !Number.isSafeInteger(input.now) || input.now < 0)
    throw new Error("POS_FINANCE_PACKAGE_OBLIGATION_SCOPE_REQUIRED");
  const facts = (await database.prepare(packageObligationFactsSql({ sourceType: "pos", payments: pos.payments, paymentFk: "finance_payment_id", postingKey: "pp.source_key", postings: pos.postings, postingGroup: "pp.source_key", clearingFrom: `FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id JOIN (${pos.settings}) c ON c.company_id=j.company_id,scope`, clearingWhere: `l.account_id IN (
        c.accounts_json::jsonb->>'stripe',c.accounts_json::jsonb->>'advance',c.accounts_json::jsonb->>'refund',c.accounts_json::jsonb->>'disputes',c.accounts_json::jsonb->>'receivable')` })).bind(input.companyId).all()).results;
  return { companyId: input.companyId, packageId: input.packageId, moduleId: "finance", observedAt: input.now,
    revision: createHash("sha256").update(JSON.stringify({ companyId: input.companyId, packageVersion: input.packageVersion, facts })).digest("hex"), outstanding: facts.length };
}
