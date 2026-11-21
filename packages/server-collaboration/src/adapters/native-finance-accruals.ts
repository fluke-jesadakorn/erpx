import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance accrual statements. */
export function nativeFinanceAccruals(db: SqlDatabase, companyId: string) {
  const call = (kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    scheduleInsert: (v: unknown[]) => call('finance_accrual_schedule_insert', v),
    eventCreated: (v: unknown[]) => call('finance_accrual_event_created', v),
    scheduleDecision: (v: unknown[]) => call('finance_accrual_schedule_decision', v),
    eventDecision: (v: unknown[]) => call('finance_accrual_event_decision', v),
    journalInsert: (v: unknown[]) => call('finance_accrual_journal_insert', v),
    journalLinesInsert: (v: unknown[]) => call('finance_accrual_journal_lines_insert', v),
    runInsert: (v: unknown[]) => call('finance_accrual_run_insert', v),
    eventRun: (v: unknown[]) => call('finance_accrual_event_run', v),
    journalPost: (v: unknown[]) => call('finance_accrual_journal_post', v),
    runPost: (v: unknown[]) => call('finance_accrual_run_post', v),
    schedulePost: (v: unknown[]) => call('finance_accrual_schedule_post', v),
  };
}
