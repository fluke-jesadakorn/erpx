import "server-only";
import type {SqlDatabase} from "@erpx/platform-runtime-server/public/runtime/types";
import {
  nativePeoplePayrollReadAll,
  PEOPLE_PAYROLL_RD_WORK_REPORT_PEOPLE,
} from './native-people-payroll-read';
/** Owner-native work-report people read. SQL is planned by the People Payroll
 * native read port (`RD_WORK_REPORT_PEOPLE`) and executed on the caller's open
 * unit; this module carries no SQL for this read. */
export function workReportPeoplePayrollFacts(db:SqlDatabase){return {people:(companyId:string)=>nativePeoplePayrollReadAll<{user_id:string;legal_name:string}>(PEOPLE_PAYROLL_RD_WORK_REPORT_PEOPLE,[companyId],db)};}
