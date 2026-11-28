import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { readFinanceWorkspacePeoplePayments, type Lane7Actor } from './native-lane-7/client';

export const financeWorkspacePendingPeopleSql = "(SELECT count(*) FROM hr_payment_batches b WHERE b.company_id=? AND b.status IN ('draft','approved','exported','partially_settled'))";
export const financeWorkspaceReadyExpenseAiSql = "(SELECT count(*) FROM expense_ai_proposals WHERE company_id=? AND state='ready')";

/**
 * Native lane-7 replacement of the retired people-payments SELECT. The owner
 * axum service executes the aggregation and projects the rows; this adapter is
 * transport only and fails closed when the native service is unavailable.
 * No TypeScript SQL fallback remains for this read.
 */
export async function financeWorkspacePeoplePayments(_write: SqlDatabase, companyId: string, actor?: Lane7Actor) {
  return readFinanceWorkspacePeoplePayments(companyId, actor);
}
