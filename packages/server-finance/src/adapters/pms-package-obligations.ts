import "server-only";
import { createHash } from "node:crypto";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { packageSettlementSource, type PackageSettlementSourceLock } from "@erpx/server-commercial/public/package-settlement-sources";
import { assertPmsPackageObligationSources, pmsPackageSettlementSources as pms } from "@erpx/server-pms/public/package-obligations";
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
  const facts = (await database.prepare(`WITH scope AS (SELECT ?::text company_id)
    SELECT 'invoice' kind,i.id,i.version,i.status state,i.outstanding_minor::text amount FROM finance_invoices i,scope
      WHERE i.company_id=scope.company_id AND i.source_type='pms' AND (i.outstanding_minor<>0 OR i.status NOT IN ('paid','voided'))
    UNION ALL SELECT 'payment',p.id,p.version,p.status,p.outstanding_minor::text FROM finance_payments p,scope
      WHERE p.company_id=scope.company_id AND EXISTS(SELECT 1 FROM (${pms.payments}) source WHERE source.company_id=p.company_id AND source.finance_id=p.id)
        AND (p.status NOT IN ('settled','voided') OR p.outstanding_minor<>0)
    UNION ALL SELECT 'posting',concat(pp.property_id,':',pp.source_key),COALESCE(j.version,0),COALESCE(j.status,'missing'),COALESCE(sum(l.base_debit_minor-l.base_credit_minor),0)::text
      FROM (${pms.postings}) pp LEFT JOIN finance_journals j ON j.id=pp.journal_id AND j.company_id=pp.company_id
      LEFT JOIN finance_journal_lines l ON l.journal_id=j.id,scope WHERE pp.company_id=scope.company_id
      GROUP BY pp.property_id,pp.source_key,j.id,j.version,j.status HAVING j.id IS NULL OR j.status<>'posted' OR count(l.id)<2 OR sum(l.base_debit_minor-l.base_credit_minor)<>0
    UNION ALL SELECT 'clearing',concat(l.account_id,':',j.currency),0,'unsettled',sum(l.debit_minor-l.credit_minor)::text
      FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id,scope
      WHERE j.company_id=scope.company_id AND j.status='posted' AND EXISTS(SELECT 1 FROM (${pms.clearing}) c WHERE c.company_id=j.company_id AND c.account_id=l.account_id)
      GROUP BY l.account_id,j.currency HAVING sum(l.debit_minor-l.credit_minor)<>0 ORDER BY kind,id`).bind(input.companyId).all()).results;
  return { companyId: input.companyId, packageId: input.packageId, moduleId: "finance", observedAt: input.now,
    revision: createHash("sha256").update(JSON.stringify({ companyId: input.companyId, packageVersion: input.packageVersion, facts })).digest("hex"), outstanding: facts.length };
}
