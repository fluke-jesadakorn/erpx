import { OPEN_PERIOD_GUARD_SQL, OPEN_PERIOD_IDS_SQL } from './finance-read-tail-r16-sql';
import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import {all} from '@erpx/server-runtime/public/runtime/sql';
export function pmsHospitalitySetupFinanceFacts(db: SqlDatabase) {
    return {
        periods: (facts: {
            company: string;
            date: string;
            date2: string;
        }) => all(db, OPEN_PERIOD_IDS_SQL, facts.company, facts.date, facts.date2),
        charts: (facts: {
            company: string;
        }) => all(db, "SELECT id,name,base_currency FROM finance_charts WHERE company_id=? AND status='active'", facts.company),
        accounts: (facts: {
            company: string;
        }) => all(db, "SELECT id,code,name,category,currency,chart_id FROM finance_accounts WHERE company_id=? AND status='active' ORDER BY code", facts.company)
    };
}
