import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/server-runtime/public/runtime/types';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { nativeNotificationStatement } from './native-notification-statement';

type DecisionMessage =
  | 'Payroll advance approved; deductions will appear in payroll'
  | 'Payroll advance decision recorded'
  | 'Payroll correction approved; recalculate the run with current inputs'
  | 'Payroll correction rejected; review the decision reason'
  | 'Payroll correction request cancelled';

/** Native-owned payroll decision notification statements. The full explicit
 * row (id, company, user, record, message, version, now) is supplied by the
 * caller; the native owner validates company authority and owns the INSERT. */
export function payrollDecisionNotificationEffects(
  database: () => SqlDatabase,
  actor: WorkContext,
) {
  const stage = (
    id: string,
    companyId: string,
    userId: string,
    recordType: string,
    recordId: string,
    version: number,
    message: DecisionMessage,
    now: number,
  ): Promise<SqlStatement> =>
    nativeNotificationStatement(
      database(),
      actor,
      {
        companyId,
        userId,
        recordType,
        recordId,
        recordVersion: version,
        message,
        kind: 'explicit',
      },
      id,
      now,
    );

  return {
    stageDecidePayrollAdvanceStatements3(
      value: string,
      companyId: string,
      requestedBy: string | number | null,
      _value2: 'payroll_advance',
      advanceId: string,
      version: number,
      value3: Extract<DecisionMessage, 'Payroll advance approved; deductions will appear in payroll' | 'Payroll advance decision recorded'>,
      now: number,
    ) {
      return stage(value, companyId, String(requestedBy), 'payroll_advance', advanceId, version, value3, now);
    },
    stageDecidePayrollCorrectionStatements3(
      value: string,
      companyId: string,
      requestedBy: string | number | null,
      _value2: 'payroll_correction',
      correctionId: string,
      version: number,
      value3: Exclude<DecisionMessage, 'Payroll advance approved; deductions will appear in payroll' | 'Payroll advance decision recorded'>,
      now: number,
    ) {
      return stage(value, companyId, String(requestedBy), 'payroll_correction', correctionId, version, value3, now);
    },
  };
}
