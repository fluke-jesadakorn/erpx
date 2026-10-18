import { randomUUID } from "node:crypto";
import type { SqlDatabase } from "../../../src/server/modules/runtime/public/runtime/types";
import type { WorkContext } from "../../../src/server/modules/identity_access/public/authorization";
import { createChart } from "../../../src/server/modules/finance/public/finance";
import { bookRuleSchema, prepareBookRule, decideBookRule } from "../../../src/server/modules/finance/public/finance-books";

/** Pre-migration Company book fixture. Original owner commands prepare and
 * independently approve the mapping before purchase enforcement is enabled. */
export async function reviewedSecondaryBook(database: SqlDatabase, preparer: WorkContext, reviewer: WorkContext, primaryChartId: string, periodId: string, effectiveDate: string) {
  const primary = await database.prepare("SELECT base_currency FROM finance_charts WHERE id=? AND company_id=?").bind(primaryChartId, preparer.companyId).first<{base_currency:string}>();
  if (!primary) throw Error("BOOK_FIXTURE_PRIMARY_REQUIRED");
  const chart = await createChart(preparer, {name:`Retained Company book ${randomUUID()}`,baseCurrency:primary.base_currency,accountingBasis:"IFRS"},randomUUID());
  const prefix = `secondary-${randomUUID()}-`;
  await database.prepare(`INSERT INTO finance_accounts(id,chart_id,company_id,code,name,category,currency,is_control,control_kind,created_at,updated_at)
    SELECT ?||id,?,company_id,code,name,category,currency,is_control,control_kind,0,0 FROM finance_accounts WHERE chart_id=? AND company_id=?`)
    .bind(prefix,chart.id,primaryChartId,preparer.companyId).run();
  const accounts = (await database.prepare("SELECT id FROM finance_accounts WHERE chart_id=? AND company_id=? ORDER BY id").bind(primaryChartId,preparer.companyId).all<{id:string}>()).results;
  const rule = await prepareBookRule(preparer,bookRuleSchema.parse({primaryChartId,secondaryChartId:chart.id,effectiveDate,openingPeriodId:periodId,reason:"Retained independently reviewed owner posting book",mappings:accounts.map(account=>({primaryAccountId:account.id,secondaryAccountId:prefix+account.id}))}),randomUUID());
  await decideBookRule(reviewer,String(rule.id),{expectedVersion:1,reason:"Independent review of retained source accounts and opening balances"},"approve",randomUUID());
  return {chartId:chart.id,ruleId:String(rule.id)};
}
