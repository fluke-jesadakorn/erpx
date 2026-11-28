import 'server-only';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import {
  nativePeoplePayrollReadAll,
  PEOPLE_PAYROLL_RD_ACTIVE_PAYEE,
  PEOPLE_PAYROLL_RD_COMMAND_RECEIPT_PRIOR,
  PEOPLE_PAYROLL_RD_ROSTER_ASSIGNMENT_BY_ID,
  PEOPLE_PAYROLL_RD_ROSTER_ASSIGNMENT_BY_SHIFT,
} from './native-people-payroll-read';

/** Owner-native point reads for the r53 read tail. Each kind is planned by the
 * Rust registry and run on the caller's open unit; no SQL or lock lives here. */

export function rosterAssignmentById<T>(db: SqlDatabase, id: string) {
  return nativePeoplePayrollReadAll<T>(PEOPLE_PAYROLL_RD_ROSTER_ASSIGNMENT_BY_ID, [id], db)
    .then(result => result.results[0] ?? null);
}

export function rosterAssignmentsByShift<T>(db: SqlDatabase, shiftId: string) {
  return nativePeoplePayrollReadAll<T>(PEOPLE_PAYROLL_RD_ROSTER_ASSIGNMENT_BY_SHIFT, [shiftId], db)
    .then(result => result.results);
}

export function commandReceiptPrior(db: SqlDatabase, commandId: string, operation: string, actorId: string, companyId: string) {
  return nativePeoplePayrollReadAll<Record<string, unknown>>(
    PEOPLE_PAYROLL_RD_COMMAND_RECEIPT_PRIOR, [commandId, operation, actorId, companyId], db,
  ).then(result => result.results[0] ?? null);
}

export function activePayee<T>(db: SqlDatabase, id: unknown, companyId: string) {
  return nativePeoplePayrollReadAll<T>(PEOPLE_PAYROLL_RD_ACTIVE_PAYEE, [id, companyId], db)
    .then(result => result.results[0] ?? null);
}
