import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { all } from "@erpx/server-pos/public/pos-core";
export function pmsHospitalitySetupFinanceFacts(db: SqlDatabase) {
    return {
        periods: (facts: {
            company: string;
            date: string;
            date2: string;
        }) => all(db, "SELECT id FROM finance_periods WHERE company_id=? AND state='open' AND start_date<=? AND end_date>=?", facts.company, facts.date, facts.date2),
        charts: (facts: {
            company: string;
        }) => all(db, "SELECT id,name,base_currency FROM finance_charts WHERE company_id=? AND status='active'", facts.company),
        accounts: (facts: {
            company: string;
        }) => all(db, "SELECT id,code,name,category,currency,chart_id FROM finance_accounts WHERE company_id=? AND status='active' ORDER BY code", facts.company)
    };
}
