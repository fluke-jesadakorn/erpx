import "server-only";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import { organizationAccessData } from "@erpx/organization-server/public/organization-access";
import {appointmentOverview} from '@erpx/organization-server/public/department-authority-reads';
import {organizationRoster} from '@erpx/organization-server/public/organization-roster-read';
import { draftCompanyName } from "@erpx/organization-server/public/people-app-entrypoint-facts";

type ReadArguments<F extends (...args: never[]) => unknown> = Parameters<F> extends [unknown, ...infer Rest] ? Rest : never;

/** Native composition for presentation readers; the supplied Holding connection stays private. */
export async function organizationPresentationQueries(holdingId: string) {
  const { write } = await holdingResources(holdingId);
  return {
    organizationAccessData: (...args: ReadArguments<typeof organizationAccessData>) => organizationAccessData(write, ...args),
    appointmentOverview: (...args: ReadArguments<typeof appointmentOverview>) => appointmentOverview(write, ...args),
    organizationRoster: (...args: ReadArguments<typeof organizationRoster>) => organizationRoster(write, ...args),
    draftCompanyName: (...args: ReadArguments<typeof draftCompanyName>) => draftCompanyName(write, ...args),
  };
}
