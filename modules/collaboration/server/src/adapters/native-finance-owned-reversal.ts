import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance journal reversal statements. */
export function financeOwnedReversal(db: SqlDatabase, companyId: string) {
  const call = (kind: string, key: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, key, 0);
  return {
    reversalJournal: (f: {
      id: string; companyId: string; periodId: string; journalNumber: string; entryDate: string;
      sourceType: string; reason: string; userId: string; now: number; sourceJournalId: string;
    }) => call('finance_reversal_journal', f.id,
      [f.id, f.periodId, f.journalNumber, f.entryDate, f.sourceType, f.reason, f.userId, f.userId, f.now, f.now, f.now, f.sourceJournalId]),
    reversalLines: (f: { id: string; journalId: string; sourceJournalId: string }) =>
      call('finance_reversal_lines', '', [f.id, f.journalId, f.sourceJournalId]),
    reverseSource: (f: { userId: string; now: number; sourceJournalId: string }) =>
      call('finance_journal_reverse', '', [f.userId, f.now, f.now, f.sourceJournalId]),
  };
}
