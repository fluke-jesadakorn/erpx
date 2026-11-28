import 'server-only';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import {
  nativePeoplePayrollReadAll,
  PEOPLE_PAYROLL_RD_PAYROLL_ITEM_SOURCE_ROWS,
} from './native-people-payroll-read';

/** Owner-native payroll-item source-rows read. The People Payroll native read
 * port plans the exact statement; this module carries no SQL for this read. */
export function payrollItemSourceRows(db: SqlDatabase, runId: string) {
  return nativePeoplePayrollReadAll<{ engagement_id: string; input_snapshot_json: string }>(
    PEOPLE_PAYROLL_RD_PAYROLL_ITEM_SOURCE_ROWS, [runId], db,
  );
}
