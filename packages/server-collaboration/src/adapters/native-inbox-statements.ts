import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned inbox statements. */
export function nativeInboxStatements(db: SqlDatabase, companyId: string) {
  const call = (kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    eventInsert: (v: unknown[]) => call('inbox_event_insert', v),
    deliveryInsert: (v: unknown[]) => call('inbox_delivery_insert', v),
    deliveryState: (v: unknown[]) => call('inbox_delivery_state', v),
    staffNotificationRead: (v: unknown[]) => call('inbox_staff_notification_read', v),
    noticeInsert: (v: unknown[]) => call('inbox_notice_insert', v),
    noticeTargetInsert: (v: unknown[]) => call('inbox_notice_target_insert', v),
    noticeTargetComplete: (v: unknown[]) => call('inbox_notice_target_complete', v),
  };
}
