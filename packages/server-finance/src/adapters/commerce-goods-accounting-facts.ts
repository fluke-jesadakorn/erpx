import {financeOwnedSnapshotSources as commerceSnapshotSources} from "@erpx/server-commerce/public/finance-owned-snapshot-sources";
import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import type {Row} from "@erpx/server-commerce/public/commerce-contracts";
export function CommerceGoodsFinanceFacts(db:SqlDatabase){return {
readFinanceAccounts1:(facts:{id:string;companyId:string;chartId:string;currency:string;category:"other_income" | "other_expense";})=>db.prepare("SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=? AND currency=? AND category=? AND status='active'").bind(facts.id,facts.companyId,facts.chartId,facts.currency,facts.category).first(),
readFinanceAccounts2:(facts:{debit_account_id:string | number | null;companyId:string;})=>db.prepare("SELECT chart_id FROM finance_accounts WHERE id=? AND company_id=? AND status='active'").bind(facts.debit_account_id,facts.companyId).first<{chart_id:string}>(),
readFinanceJournalLines1:(facts:{debit_account_id:string | number | null;debit_account_id2:string | number | null;id:string | number | null;companyId:string;chartId:string;branchId:string | null;branchId2:string | null;departmentId:string | null;departmentId2:string | null;})=>db.prepare(`SELECT f.id,f.version,j.id journal_id,j.version journal_version,j.currency,j.fx_rate_micros,
      c.base_currency,o.currency document_currency,fl.id line_id,fl.quantity_milli,fl.value_minor,fl.subtotal_minor,fl.tax_minor,fl.stock_currency,
      (SELECT SUM(l.base_credit_minor-l.base_debit_minor) FROM finance_journal_lines l WHERE l.journal_id=j.id AND l.account_id=?) clearing_base,
      (SELECT SUM(l.credit_minor-l.debit_minor) FROM finance_journal_lines l WHERE l.journal_id=j.id AND l.account_id=?) clearing_transaction
    ${commerceSnapshotSources().commerce_goods_accounting_facts_commerce_fulfillments_f_from} ${commerceSnapshotSources().commerce_goods_accounting_facts_commerce_orders_o_join} ON o.id=f.order_id
    JOIN finance_journals j ON j.id=f.journal_id AND j.company_id=o.company_id AND j.status='posted'
    JOIN finance_charts c ON c.id=j.chart_id ${commerceSnapshotSources().commerce_goods_accounting_facts_commerce_fulfillment_lines_fl_join} ON fl.fulfillment_id=f.id
    WHERE f.invoice_id=? AND o.company_id=? AND j.chart_id=?
      AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)
    ORDER BY f.id,fl.id`).bind(facts.debit_account_id,facts.debit_account_id2,facts.id,facts.companyId,facts.chartId,facts.branchId,facts.branchId2,facts.departmentId,facts.departmentId2).all<Row>(),
guardRule2:(facts:{key:string;guardId:string;values:string[];})=>assertion(db,facts.key,facts.guardId,"EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND chart_id=? AND currency=? AND category=? AND status='active')",facts.values)
};}
