import "server-only";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { calendarData } from "@/server/modules/work_projection/public/calendar/index";
import { calendarHierarchy } from "@/server/modules/work_projection/public/calendar/index";
import { activeCompanyIdentity, activeCompanySlug } from "@/server/modules/organization/public/people-app-entrypoint-facts";
import { resolveWorkspaceBrand } from "@/server/modules/brand/public/branding";
import { getWorkCapabilities } from "@/server/modules/work_projection/public/work-capabilities";
import { executiveVisible } from "@/server/modules/runtime/public/executive/access";
import { activeCompanyName } from "@/server/modules/organization/public/people-app-entrypoint-facts";
import { getEmployeeRequest } from "@/server/modules/records/public/employee-requests";
import { requestPageOptions } from "@/server/modules/records/public/request-page";
import {messagingPeople} from '../../collaboration/public/messaging';

type ReadArguments<F extends (...args: never[]) => unknown> = Parameters<F> extends [unknown, ...infer Rest] ? Rest : never;

/** Native composition for presentation readers; the supplied Holding connection stays private. */
export async function workProjectionPresentationQueries(holdingId: string) {
  const { write } = await holdingResources(holdingId);
  return {
    calendarData: (...args: ReadArguments<typeof calendarData>) => calendarData(write, ...args),
    calendarHierarchy: (...args: ReadArguments<typeof calendarHierarchy>) => calendarHierarchy(write, ...args),
    calendarAccounts: (...args: ReadArguments<typeof messagingPeople>) => messagingPeople(write, ...args),
    activeCompanySlug: (...args: ReadArguments<typeof activeCompanySlug>) => activeCompanySlug(write, ...args),
    activeCompanyIdentity: (...args: ReadArguments<typeof activeCompanyIdentity>) => activeCompanyIdentity(write, ...args),
    resolveWorkspaceBrand: (...args: ReadArguments<typeof resolveWorkspaceBrand>) => resolveWorkspaceBrand(write, ...args),
    getWorkCapabilities: (...args: ReadArguments<typeof getWorkCapabilities>) => getWorkCapabilities(write, ...args),
    executiveVisible: (...args: ReadArguments<typeof executiveVisible>) => executiveVisible(write, ...args),
    activeCompanyName: (...args: ReadArguments<typeof activeCompanyName>) => activeCompanyName(write, ...args),
    getEmployeeRequest: (...args: ReadArguments<typeof getEmployeeRequest>) => getEmployeeRequest(write, ...args),
    requestPageOptions: (...args: ReadArguments<typeof requestPageOptions>) => requestPageOptions(write, ...args),
  };
}
