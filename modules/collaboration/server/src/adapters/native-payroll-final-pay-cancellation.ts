import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned people/payroll final-pay cancellation statements. */
export function payrollFinalPayCancellation(db: SqlDatabase, companyId: string) {
  const call = (kind: string, key: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, key, 0);
  return {
    event: (f: {
      cancellationId: string; prepId: string; taskId: string; runId: string; companyId: string;
      userId: string; taskVersion: number; evidenceJson: string; now: number;
    }) => call('payroll_final_pay_cancel_event', f.cancellationId,
      [f.cancellationId, f.prepId, f.taskId, f.runId, f.companyId, f.userId, f.taskVersion, f.evidenceJson, f.now]),
    cancelRun: (f: { now: number; runId: string; companyId: string; runVersion: number }) =>
      call('payroll_run_cancel', '', [f.now, f.runId, f.companyId, f.runVersion]),
    cancelPrep: (f: { now: number; prepId: string; companyId: string; version: number }) =>
      call('payroll_final_pay_prep_cancel', '', [f.now, f.prepId, f.companyId, f.version]),
  };
}
