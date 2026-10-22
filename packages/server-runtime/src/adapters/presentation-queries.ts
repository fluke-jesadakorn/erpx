import "server-only";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { reportLibrary } from "@erpx/server-runtime/public/executive/reports";
import { reportView } from "@erpx/server-runtime/public/executive/reports";
import { metricCatalogFor } from "@erpx/server-runtime/public/executive/metrics";

type ReadArguments<F extends (...args: never[]) => unknown> = Parameters<F> extends [unknown, ...infer Rest] ? Rest : never;

/** Native composition for presentation readers; the supplied Holding connection stays private. */
export async function runtimePresentationQueries(holdingId: string) {
  const { write } = await holdingResources(holdingId);
  return {
    reportLibrary: (...args: ReadArguments<typeof reportLibrary>) => reportLibrary(write, ...args),
    reportView: (...args: ReadArguments<typeof reportView>) => reportView(write, ...args),
    metricCatalogFor: (...args: ReadArguments<typeof metricCatalogFor>) => metricCatalogFor(write, ...args),
  };
}
