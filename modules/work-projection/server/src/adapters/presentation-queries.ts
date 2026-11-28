import "server-only";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import { calendarData } from "@erpx/work-projection-server/public/calendar/index";
import { calendarHierarchy } from "@erpx/work-projection-server/public/calendar/index";
import { activeCompanyIdentity, activeCompanySlug } from "@erpx/organization-server/public/people-app-entrypoint-facts";
import { resolveWorkspaceBrand } from "@erpx/brand-server/public/branding";
import { getWorkCapabilities } from "@erpx/work-projection-server/public/work-capabilities";
import { executiveVisible } from "@erpx/platform-runtime-server/public/executive/access";
import { activeCompanyName } from "@erpx/organization-server/public/people-app-entrypoint-facts";
import {getEmployeeRequest} from '@erpx/records-server/public/employee-requests-read';
import { requestPageOptions } from "@erpx/records-server/public/request-page";
import {messagingPeople} from '@erpx/collaboration-server/public/messaging';

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
    resolveWorkspaceBrand: (...args: Parameters<typeof resolveWorkspaceBrand>) => resolveWorkspaceBrand(...args),
    getWorkCapabilities: (...args: ReadArguments<typeof getWorkCapabilities>) => getWorkCapabilities(write, ...args),
    executiveVisible: (...args: ReadArguments<typeof executiveVisible>) => executiveVisible(write, ...args),
    activeCompanyName: (...args: ReadArguments<typeof activeCompanyName>) => activeCompanyName(write, ...args),
    getEmployeeRequest: (...args: ReadArguments<typeof getEmployeeRequest>) => getEmployeeRequest(write, ...args),
    requestPageOptions: (...args: ReadArguments<typeof requestPageOptions>) => requestPageOptions(write, ...args),
  };
}
