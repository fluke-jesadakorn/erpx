import "server-only";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { organizationAccessData } from "@erpx/server-organization/public/organization-access";
import {appointmentOverview} from '@erpx/server-organization/public/department-authority-reads';
import {organizationRoster} from '@erpx/server-organization/public/organization-roster-read';
import { draftCompanyName } from "@erpx/server-organization/public/people-app-entrypoint-facts";

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
