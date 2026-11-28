import "server-only";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import { loadPublicMarketingForm } from "@erpx/marketing-server/public/marketing-public-form";
import { assertProductionModuleEnabled } from "@erpx/pms-server/public/feature-availability";

type ReadArguments<F extends (...args: never[]) => unknown> = Parameters<F> extends [unknown, ...infer Rest] ? Rest : never;

/** Native composition for presentation readers; the supplied Holding connection stays private. */
export async function marketingPresentationQueries(holdingId: string) {
  const { write } = await holdingResources(holdingId);
  return {
    loadPublicMarketingForm: (...args: ReadArguments<typeof loadPublicMarketingForm>) => loadPublicMarketingForm(write, ...args),
    assertProductionModuleEnabled: (...args: ReadArguments<typeof assertProductionModuleEnabled>) => assertProductionModuleEnabled(write, ...args),
  };
}
