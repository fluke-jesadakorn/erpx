import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned inbox statements. */
export function nativeInboxStatements(db: SqlDatabase, companyId: string) {
  const read = (company: string, kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, company,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  const call = (kind: string, values: unknown[]): Promise<SqlStatement> => read(companyId, kind, values);
  return {
    eventInsert: (v: unknown[]) => call('inbox_event_insert', v),
    deliveryInsert: (v: unknown[]) => call('inbox_delivery_insert', v),
    deliveryState: (v: unknown[]) => call('inbox_delivery_state', v),
    staffNotificationRead: (v: unknown[]) => call('inbox_staff_notification_read', v),
    noticeInsert: (v: unknown[]) => call('inbox_notice_insert', v),
    noticeTargetInsert: (v: unknown[]) => call('inbox_notice_target_insert', v),
    noticeTargetComplete: (v: unknown[]) => call('inbox_notice_target_complete', v),
    // Company-scoped inbox reads. The owner planner owns the exact SQL and the
    // ordered binds; this bridge never sends SQL text and has no TS fallback.
    eventByKey: (companyId: string, key: string) => read(companyId, 'inbox_event_by_key', [companyId, key]),
    noticeExists: (companyId: string, id: string) => read(companyId, 'inbox_notice_exists', [id, companyId]),
    recentNoticeCount: (companyId: string, userId: string, after: number) => read(companyId, 'inbox_recent_notice_count', [companyId, userId, after]),
  };
}
