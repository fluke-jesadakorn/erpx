import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance book adjustment statements. */
export function financeBookAdjustments(db: SqlDatabase, companyId: string) {
  const call = (kind: string, key: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, key, 0);
  return {
    create: (f: {
      id: string; companyId: string; chartId: string; primaryJournalId: string; sourceVersion: string | number | null;
      contentJson: string; sourceHash: string; preparedBy: string; reason: string; createdAt: number; updatedAt: number;
    }) => call('finance_book_adjustment_create', f.id,
      [f.id, f.companyId, f.chartId, f.primaryJournalId, f.sourceVersion, f.contentJson, f.sourceHash, f.preparedBy, f.reason, f.createdAt, f.updatedAt]),
    decide: (f: { status: string; version: number; reviewedBy: string; journalId: string | null; updatedAt: number; id: string }) =>
      call('finance_book_adjustment_decide', '', [f.status, f.version, f.reviewedBy, f.journalId, f.updatedAt, f.id]),
    reverse: (f: { reversalId: string; reason: string; updatedAt: number; id: string }) =>
      call('finance_book_adjustment_reverse', '', [f.reversalId, f.reason, f.updatedAt, f.id]),
  };
}
