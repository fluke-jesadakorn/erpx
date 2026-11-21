import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned people statements (batch B). */
export function nativePeopleBatchB(db: SqlDatabase) {
  const call = (companyId: string, kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    leaveEvidence: (companyId: string, values: unknown[]) => call(companyId, 'people_leave_evidence_manifest', values),
    prepReschedule: (companyId: string, values: unknown[]) => call(companyId, 'people_prep_task_reschedule', values),
    prepComplete: (companyId: string, values: unknown[]) => call(companyId, 'people_prep_task_complete', values),
  };
}
