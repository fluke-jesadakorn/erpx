import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Routing key for Company-independent owner sweeps on the system route. */
export const SYSTEM_COMPANY = 'system';

/** Fetch only the SQL bytes of a fixed owner-owned projection fragment. The
 * caller composes its registered dynamic predicates around it and binds the
 * placeholders in the same order; no SQL text is sent to the native owner. */
export async function nativeInboxSqlFragment(companyId: string, kind: string, values: unknown[] = []): Promise<string> {
  let sql = '';
  const capture = {
    prepare(text: string) {
      sql = text;
      const bound = { bind() { return bound; } };
      return bound;
    },
  } as unknown as SqlDatabase;
  await nativeSystemNotificationStatement(capture, companyId,
    { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return sql;
}

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
    // Holding-wide fanout/metric sweeps have no Company actor. Their SQL is
    // Company-independent; the planner Company is routing-only on the private
    // system statement route (single-target deployments serve a default target).
    pendingTargets: () => read(SYSTEM_COMPANY, 'inbox_pending_targets', []),
    pendingMetrics: () => read(SYSTEM_COMPANY, 'inbox_pending_metrics', []),
    unresolvedMetrics: (types: string[]) => read(SYSTEM_COMPANY, 'inbox_unresolved_metrics', types),
    candidate: (companyId: string, id: string, userId: string) => read(companyId, 'inbox_candidate', [id, companyId, userId]),
    deliveryNativeOwned: (companyId: string, id: string, userId: string) => read(companyId, 'inbox_delivery_native_owned', [id, companyId, userId]),
  };
}
