import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import type { FinanceRow } from "../domain/finance";

/** Read projections use the entrypoint's existing connection and Company selection. */
export function financeEntrypointQueries(db: SqlDatabase) {
  return {
    periodEndDates: (companyId: string) => db.prepare("SELECT id,end_date FROM finance_periods WHERE company_id=? ORDER BY start_date DESC").bind(companyId).all<{ id: string; end_date: string }>(),
    allPeriods: async (companyId: string) => (await db.prepare("SELECT * FROM finance_periods WHERE company_id=? ORDER BY start_date DESC").bind(companyId).all<FinanceRow>()).results,
  };
}
