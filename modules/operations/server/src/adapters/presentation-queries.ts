import "server-only";
import { holdingResources, runtimeEnv } from "../../../../../platform/runtime/server/src/public/runtime";
import { staffSettings } from "@erpx/operations-server/public/staff-settings";
import { scopedBranchNames } from "@erpx/organization-server/public/people-app-entrypoint-facts";
import { aiProviderOverview } from "@erpx/agent-server/public/ai-providers";
import { nativeListBrandProfiles } from "@erpx/brand-server/public/branding";
import { serviceTeamOptions } from "@erpx/support-server/public/service-team-options";

type ReadArguments<F extends (...args: never[]) => unknown> = Parameters<F> extends [unknown, ...infer Rest] ? Rest : never;

/** Native composition for presentation readers; the supplied Holding connection stays private. */
export async function operationsPresentationQueries(holdingId: string) {
  const { write } = await holdingResources(holdingId);
  return {
    staffSettings: (...args: ReadArguments<typeof staffSettings>) => staffSettings(write, ...args),
    scopedBranchNames: (...args: ReadArguments<typeof scopedBranchNames>) => scopedBranchNames(write, ...args),
    aiProviderOverview: async (actor: Parameters<typeof aiProviderOverview>[1]) => aiProviderOverview(write, actor, await runtimeEnv()),
    listBrandProfiles: (...args: Parameters<typeof nativeListBrandProfiles>) => nativeListBrandProfiles(...args),
    serviceTeamOptions: (...args: ReadArguments<typeof serviceTeamOptions>) => serviceTeamOptions(write, ...args),
  };
}
