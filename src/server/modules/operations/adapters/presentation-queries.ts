import "server-only";
import { holdingResources, runtimeEnv } from "@/server/modules/runtime/public/runtime";
import { staffSettings } from "@/server/modules/operations/public/staff-settings";
import { scopedBranchNames } from "@/server/modules/organization/public/people-app-entrypoint-facts";
import { aiProviderOverview } from "@/server/modules/agent/public/ai-providers";
import { listBrandProfiles } from "@/server/modules/brand/public/branding";
import { serviceTeamOptions } from "@/server/modules/support/public/service-team-options";

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
