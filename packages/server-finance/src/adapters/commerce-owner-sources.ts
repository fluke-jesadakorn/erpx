import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
/** Commerce's actual independent customer/supplier, chart, period, FX and book
 * references plus original posted invoice/payment/refund effects. This is a
 * source lease, with no generic Finance permission or purchase decision. */
export function commerceOwnerFinanceSourceReadLocks(database:SqlDatabase){return [
 database.prepare("LOCK TABLE finance_parties,finance_charts,finance_accounts,finance_sales_defaults,finance_periods,finance_book_rules,finance_book_account_mappings,finance_dimension_definitions,finance_dimension_values,company_money_settings,company_currency_scales,company_fx_rates IN SHARE MODE"),
 database.prepare("LOCK TABLE finance_invoices,finance_journals,finance_journal_lines,finance_book_postings,finance_payments,finance_allocations,finance_refund_obligations,money_transaction_snapshots IN SHARE ROW EXCLUSIVE MODE"),
 database.prepare("LOCK TABLE finance_supplier_instructions IN SHARE MODE")
];}
