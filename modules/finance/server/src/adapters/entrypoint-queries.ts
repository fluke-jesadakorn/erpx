import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import type { FinanceRow } from "@erpx/finance-server/domain/finance";
import {
  nativeFinanceReadAll,
  FINANCE_RD_PERIOD_END_DATES,
  FINANCE_RD_ALL_PERIODS,
} from "./native-finance-read";

/** Read projections use the entrypoint's existing connection and Company selection.
 *  Both statements are fixed owner read plans; no TypeScript SQL remains. */
export function financeEntrypointQueries(db: SqlDatabase) {
  return {
    periodEndDates: (companyId: string) =>
      nativeFinanceReadAll<{ id: string; end_date: string }>(FINANCE_RD_PERIOD_END_DATES, [companyId], db),
    allPeriods: async (companyId: string) =>
      (await nativeFinanceReadAll<FinanceRow>(FINANCE_RD_ALL_PERIODS, [companyId], db)).results,
  };
}
