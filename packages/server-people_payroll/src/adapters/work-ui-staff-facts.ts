import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {
  nativePeoplePayrollReadAll,
  PEOPLE_PAYROLL_RD_WORK_UI_HEAD_PLACEMENT,
} from './native-people-payroll-read';
export function workUiStaffFacts(db:SqlDatabase){return {
/** Owner-native read: SQL is planned by the People Payroll native read port
 * (`RD_WORK_UI_HEAD_PLACEMENT`) and executed on the caller's open unit; this
 * module carries no SQL for this read. */
async headPlacement(companyId:string,value:string,personId:string){return (await nativePeoplePayrollReadAll<{position_id:string|null}>(PEOPLE_PAYROLL_RD_WORK_UI_HEAD_PLACEMENT,[companyId,value,personId],db)).results[0]??null;}
};}
