import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { nativePayGroupOptions } from "./native-pay-group-options";
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {
  nativePeoplePayrollReadAll,
  PEOPLE_PAYROLL_RD_ORG_ROSTER_EXIT_ENGAGEMENT_IDS,
} from './native-people-payroll-read';

/** Owner-native roster reads. SQL is planned by the People Payroll native read
 * port (`RD_ORG_ROSTER_*`) and executed on the caller's open unit; this module
 * carries no SQL for these reads. */
export async function organizationRosterExitEngagementIds(db:SqlDatabase,companyId:string,positionId:string){
  return (await nativePeoplePayrollReadAll<{id:string}>(PEOPLE_PAYROLL_RD_ORG_ROSTER_EXIT_ENGAGEMENT_IDS,[companyId,positionId],db)).results.map(x=>x.id);
}
export async function organizationMonthlyPayGroups(db: SqlDatabase, actor: WorkContext) {
  const groups = await nativePayGroupOptions(db, actor.companyId, actor);
  return groups.filter(group => group.frequency === "monthly")
    .map(({ id, name, currency }) => ({ id, name, currency }));
}
