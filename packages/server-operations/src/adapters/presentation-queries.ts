import "server-only";
import { holdingResources, runtimeEnv } from "../../../../packages/server-runtime/src/public/runtime";
import { staffSettings } from "@erpx/server-operations/public/staff-settings";
import { scopedBranchNames } from "@erpx/server-organization/public/people-app-entrypoint-facts";
import { aiProviderOverview } from "@erpx/server-agent/public/ai-providers";
import { listBrandProfiles } from "@erpx/server-brand/public/branding";
import { serviceTeamOptions } from "@erpx/server-support/public/service-team-options";

type ReadArguments<F extends (...args: never[]) => unknown> = Parameters<F> extends [unknown, ...infer Rest] ? Rest : never;

/** Native composition for presentation readers; the supplied Holding connection stays private. */
export async function operationsPresentationQueries(holdingId: string) {
  const { write } = await holdingResources(holdingId);
  return {
    staffSettings: (...args: ReadArguments<typeof staffSettings>) => staffSettings(write, ...args),
    scopedBranchNames: (...args: ReadArguments<typeof scopedBranchNames>) => scopedBranchNames(write, ...args),
    aiProviderOverview: async (actor: Parameters<typeof aiProviderOverview>[1]) => aiProviderOverview(write, actor, await runtimeEnv()),
    listBrandProfiles: (...args: ReadArguments<typeof listBrandProfiles>) => listBrandProfiles(write, ...args),
    serviceTeamOptions: (...args: ReadArguments<typeof serviceTeamOptions>) => serviceTeamOptions(write, ...args),
  };
}
