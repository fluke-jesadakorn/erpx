import "server-only";
import {all} from "@erpx/server-pos/public/pos-core";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function PosReadFinanceFacts(db:SqlDatabase){return {
readFinanceAccounts1:(facts:{company:string;})=>all(db,"SELECT id,code,name,chart_id,currency,control_kind FROM finance_accounts WHERE company_id=? AND category='asset' AND control_kind='bank' AND status='active' ORDER BY code",facts.company),
readFinanceBankTransactions1:(facts:{company:string;fact1:unknown;currency:string | number | boolean | null;amount_minor:string | number | boolean | null;reference:string | number | boolean | null;business_date:string | number | boolean | null;})=>all(db,`SELECT t.id,t.transaction_date,t.external_id,t.amount_minor,t.currency,t.version
  FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id
  WHERE t.company_id=? AND s.bank_account=? AND t.currency=? AND t.amount_minor=?
  AND t.external_id=? AND t.transaction_date>=? AND t.match_status IN ('unmatched','proposed')
  ORDER BY t.transaction_date DESC,t.id LIMIT 10`,facts.company,facts.fact1,facts.currency,facts.amount_minor,facts.reference,facts.business_date),
readFinanceBankTransactions2:(facts:{company:string;})=>all(db,"SELECT id,transaction_date,description,amount_minor,currency,version FROM finance_bank_transactions WHERE company_id=? AND match_status IN ('unmatched','proposed') ORDER BY transaction_date DESC LIMIT 200",facts.company),
readFinanceAccounts2:(facts:{company:string;})=>all(db,"SELECT id,code,name,chart_id,category,currency,control_kind FROM finance_accounts WHERE company_id=? AND status='active' ORDER BY code",facts.company),
readFinanceCharts1:(facts:{company:string;})=>all(db,'SELECT id,name,base_currency,status FROM finance_charts WHERE company_id=? ORDER BY name',facts.company)
};}
