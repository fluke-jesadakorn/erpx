import "server-only";
import { createHash } from "node:crypto";
import type { PostgresDatabase } from "../../runtime/public/runtime/postgres";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import { packageSettlementSource, type PackageSettlementSourceLock } from "../../commercial/public/package-settlement-sources";
import { assertPosPackageObligationSources, posPackageSettlementSources as pos } from "../../pos/public/package-obligations";
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
  const facts = (await database.prepare(`WITH scope AS (SELECT ?::text company_id)
    SELECT 'invoice' kind,i.id,i.version,i.status state,i.outstanding_minor::text amount FROM finance_invoices i,scope
      WHERE i.company_id=scope.company_id AND i.source_type='pos' AND (i.outstanding_minor<>0 OR i.status NOT IN ('paid','voided'))
    UNION ALL SELECT 'payment',p.id,p.version,p.status,p.outstanding_minor::text FROM finance_payments p,scope
      WHERE p.company_id=scope.company_id AND EXISTS(SELECT 1 FROM (${pos.payments}) source WHERE source.company_id=p.company_id AND source.finance_payment_id=p.id)
        AND (p.status NOT IN ('settled','voided') OR p.outstanding_minor<>0)
    UNION ALL SELECT 'posting',pp.source_key,COALESCE(j.version,0),COALESCE(j.status,'missing'),COALESCE(sum(l.base_debit_minor-l.base_credit_minor),0)::text
      FROM (${pos.postings}) pp LEFT JOIN finance_journals j ON j.id=pp.journal_id AND j.company_id=pp.company_id
      LEFT JOIN finance_journal_lines l ON l.journal_id=j.id,scope WHERE pp.company_id=scope.company_id
      GROUP BY pp.source_key,j.id,j.version,j.status HAVING j.id IS NULL OR j.status<>'posted' OR count(l.id)<2 OR sum(l.base_debit_minor-l.base_credit_minor)<>0
    UNION ALL SELECT 'clearing',concat(l.account_id,':',j.currency),0,'unsettled',sum(l.debit_minor-l.credit_minor)::text
      FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id JOIN (${pos.settings}) c ON c.company_id=j.company_id,scope
      WHERE j.company_id=scope.company_id AND j.status='posted' AND l.account_id IN (
        c.accounts_json::jsonb->>'stripe',c.accounts_json::jsonb->>'advance',c.accounts_json::jsonb->>'refund',c.accounts_json::jsonb->>'disputes',c.accounts_json::jsonb->>'receivable')
      GROUP BY l.account_id,j.currency HAVING sum(l.debit_minor-l.credit_minor)<>0 ORDER BY kind,id`).bind(input.companyId).all()).results;
  return { companyId: input.companyId, packageId: input.packageId, moduleId: "finance", observedAt: input.now,
    revision: createHash("sha256").update(JSON.stringify({ companyId: input.companyId, packageVersion: input.packageVersion, facts })).digest("hex"), outstanding: facts.length };
}
