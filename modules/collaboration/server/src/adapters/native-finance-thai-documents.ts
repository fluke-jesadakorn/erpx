import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned finance Thai document statements. */
export function nativeFinanceThaiDocuments(db: SqlDatabase, companyId: string) {
  const call = (kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    sequenceSeed: (v: unknown[]) => call('thai_doc_sequence_seed', v),
    sequenceBump: (v: unknown[]) => call('thai_doc_sequence_bump', v),
    issue: (v: unknown[]) => call('thai_doc_issue', v),
    eventIssued: (v: unknown[]) => call('thai_doc_event_issued', v),
    changeInsert: (v: unknown[]) => call('thai_doc_change_insert', v),
    eventSupersede: (v: unknown[]) => call('thai_doc_event_supersede', v),
    changeDecision: (v: unknown[]) => call('thai_doc_change_decision', v),
  };
}
