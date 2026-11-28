import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance journal statements. */
export function financeJournalPlanning(db: SqlDatabase, companyId: string, userId: string) {
  const call = (kind: string, key: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId, recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, key, 0);
  return {
    journal: (f: {
      journalId: string; companyId: string; chartId: string; periodId: string; number: string;
      entryDate: string; currency: string; fx: number; sourceType: string; sourceId: string | null;
      memo: string; isOpening: boolean; status: string; now: number; userId: string;
    }) => call('finance_journal_create', f.journalId,
      [f.journalId, f.companyId, f.chartId, f.periodId, f.number, f.entryDate, f.currency, f.fx,
       f.sourceType, f.sourceId, f.memo, f.isOpening ? 1 : 0, f.status, f.userId,
       f.status === 'posted' ? f.userId : null, f.status === 'posted' ? f.now : null, f.now, f.now]),
    line: (f: {
      journalId: string; id: string; accountId: string; description: string;
      debit: number; credit: number; baseDebit: number; baseCredit: number;
      dimensions: unknown; ordinal: number;
    }) => call('finance_journal_line_create', f.id,
      [f.id, f.journalId, f.accountId, f.description, f.debit, f.credit, f.baseDebit, f.baseCredit,
       JSON.stringify(f.dimensions ?? {}), f.ordinal]),
  };
}
