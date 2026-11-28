import 'server-only';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import {
  nativePeoplePayrollReadAll,
  PEOPLE_PAYROLL_RD_EVALUATION_ENGAGEMENT_TIMEZONE,
  PEOPLE_PAYROLL_RD_EVALUATION_PAY_BASIS_ROW,
} from './native-people-payroll-read';

/** Owner-native evaluation reads. The People Payroll native read port plans the
 * exact statements; this module carries no SQL for these reads. Both the
 * timezone read (today-group and role-period-zone) and the pay-basis row read
 * (workspace basis row and compensation-draft basis) dedupe two TS literals
 * onto one owner read kind each. */
export function evaluationEngagementTimezone(db: SqlDatabase, engagementId: string) {
  return nativePeoplePayrollReadAll<{ timezone: string }>(
    PEOPLE_PAYROLL_RD_EVALUATION_ENGAGEMENT_TIMEZONE, [engagementId], db,
  ).then(result => result.results[0] ?? null);
}

export function evaluationPayBasisRow(db: SqlDatabase, id: string, companyId: string, effectiveFrom: string) {
  return nativePeoplePayrollReadAll<{ version: number; payload_json: string }>(
    PEOPLE_PAYROLL_RD_EVALUATION_PAY_BASIS_ROW, [id, companyId, effectiveFrom], db,
  ).then(result => result.results[0] ?? null);
}
