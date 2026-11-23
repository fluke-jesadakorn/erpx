import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import "server-only";
import {type Row} from '@erpx/server-commerce/public/commerce';
import {
  nativePeoplePayrollReadAll,
  PEOPLE_PAYROLL_RD_RECORD_SUPPLEMENT_ROWS,
} from './native-people-payroll-read';
export function RecordDetailsPeoplePayrollOwnedFacts(database: SqlDatabase) { return {
  /** Owner-native read: SQL is planned by the People Payroll native read port
   * (`RD_RECORD_SUPPLEMENT_ROWS`) and executed on the caller's open unit; this
   * module carries no SQL for this read. */
  readRecordSupplementDataRows4(companyId: string, id: string, currency: string, offset: number) { return nativePeoplePayrollReadAll<Row>(PEOPLE_PAYROLL_RD_RECORD_SUPPLEMENT_ROWS, [companyId, id, currency, offset], database); }
}; }
