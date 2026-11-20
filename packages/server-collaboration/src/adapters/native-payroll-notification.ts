import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { nativeNotificationStatement } from './native-notification-statement';

/** Native-owned payroll advance independent-decision notice statement. */
export function stagePayrollAdvanceNotification(
  db: SqlDatabase,
  actor: WorkContext,
  id: string,
  now: number,
): Promise<SqlStatement> {
  return nativeNotificationStatement(
    db,
    actor,
    {
      companyId: actor.companyId,
      userId: actor.userId,
      recordType: 'payroll_advance',
      recordId: id,
      recordVersion: 1,
      message: '',
      kind: 'payroll_advance',
      timestamp: now,
    },
    `payroll-advance-${id}`,
    now,
  );
}

/** Native-owned payroll correction independent-decision notice statement. */
export function stagePayrollCorrectionNotification(
  db: SqlDatabase,
  actor: WorkContext,
  id: string,
  now: number,
): Promise<SqlStatement> {
  return nativeNotificationStatement(
    db,
    actor,
    {
      companyId: actor.companyId,
      userId: actor.userId,
      recordType: 'payroll_correction',
      recordId: id,
      recordVersion: 1,
      message: '',
      kind: 'payroll_correction',
      timestamp: now,
    },
    `payroll-correction-${id}`,
    now,
  );
}
