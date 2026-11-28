import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeHoldingNotificationStatement } from './native-system-notification';

/** Native-owned legal runtime job-health statements (holding-keyed worker). */
export function nativeLegalJobHealth(db: SqlDatabase, holdingId: string) {
  const call = (kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeHoldingNotificationStatement(db, holdingId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    renew: (at: number, task: string) => call('legal_job_health_renew', [at, task]),
    started: (task: string, at: number) => call('legal_job_health_start', [task, at]),
    succeeded: (h: number, s: number, task: string) => call('legal_job_health_success', [h, s, task]),
    failed: (at: number, code: string, task: string) => call('legal_job_health_failure', [at, code, task]),
  };
}
