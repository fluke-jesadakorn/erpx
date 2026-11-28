import 'server-only';
import type { SqlDatabase, SqlStatement } from '@erpx/platform-runtime-server/public/runtime/types';
import { nativeSystemNotificationStatement } from './native-system-notification';

/** Native-owned people/payroll statements (batch A). */
export function nativePeopleBatchA(db: SqlDatabase) {
  const call = (companyId: string, kind: string, values: unknown[]): Promise<SqlStatement> =>
    nativeSystemNotificationStatement(db, companyId,
      { userId: '', recordType: '', recordId: '', recordVersion: 0, message: '', kind, values_json: JSON.stringify(values) }, '', 0);
  return {
    employee: (f: { employeeId: string; personId: string; companyId: string; branch: string; department: string | null; legalName: string; now: number }) =>
      call(f.companyId, 'people_self_service_employee', [f.employeeId, f.personId, f.companyId, f.branch, f.department, f.legalName, f.now]),
    payslip: (f: { companyId: string; values: unknown[] }) => call(f.companyId, 'people_issued_payslip', f.values),
    cancelPayment: (f: { companyId: string; now: number; id: string; expectedVersion: number }) =>
      call(f.companyId, 'people_expense_payment_cancel', [f.now, f.id, f.companyId, f.expectedVersion]),
  };
}
