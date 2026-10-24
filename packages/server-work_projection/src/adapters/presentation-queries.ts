import "server-only";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { calendarData } from "@erpx/server-work_projection/public/calendar/index";
import { calendarHierarchy } from "@erpx/server-work_projection/public/calendar/index";
import { activeCompanyIdentity, activeCompanySlug } from "@erpx/server-organization/public/people-app-entrypoint-facts";
import { resolveWorkspaceBrand } from "@erpx/server-brand/public/branding";
import { getWorkCapabilities } from "@erpx/server-work_projection/public/work-capabilities";
import { executiveVisible } from "@erpx/server-runtime/public/executive/access";
import { activeCompanyName } from "@erpx/server-organization/public/people-app-entrypoint-facts";
import {getEmployeeRequest} from '@erpx/server-records/public/employee-requests-read';
import { requestPageOptions } from "@erpx/server-records/public/request-page";
import {messagingPeople} from '@erpx/server-collaboration/public/messaging';

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
