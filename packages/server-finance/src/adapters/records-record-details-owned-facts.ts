import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { fiscalCloseSourceSql } from "@erpx/server-finance/public/finance-fiscal-close-source";
import "server-only";
import { type Row } from "@erpx/server-commerce/public/commerce";
export function RecordDetailsFinanceOwnedFacts(database: SqlDatabase) {
    return { readCommerceMasterDetailRow2(id: string, companyId: string) { return database.prepare("SELECT * FROM finance_parties WHERE id=? AND company_id=? AND kind IN ('customer','supplier') AND person_id IS NULL").bind(id, companyId).first<Row>(); },
        readRecordSupplementDataAccount(id: string, companyId: string) { return database.prepare("SELECT a.*,c.base_currency FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.id=? AND a.company_id=?").bind(id, companyId).first<Row>(); },
        readRecordSupplementDataPeriod(periodId: string, companyId: string) { return database.prepare("SELECT start_date,end_date FROM finance_periods WHERE id=? AND company_id=?").bind(periodId, companyId).first<Row>(); },
        readRecordSupplementDataRows3(id: string, companyId: string, chart_id: string | number | null, value: "0" | "1", value2: string | null, value3: string | null, value4: string | null, value5: string | null, offset: number) { return database.prepare(`SELECT l.*,j.id journal_id,j.journal_number,j.entry_date,j.currency,j.status FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE l.account_id=? AND j.company_id=? AND j.chart_id=? AND j.status IN ('posted','reversed') AND (?='0' OR NOT ${fiscalCloseSourceSql()}) AND (?::text IS NULL OR j.entry_date>=?) AND (?::text IS NULL OR j.entry_date<=?) ORDER BY j.entry_date DESC,j.id,l.ordinal LIMIT 31 OFFSET ?`).bind(id, companyId, chart_id, value, value2, value3, value4, value5, offset).all<Row>(); } };
}
