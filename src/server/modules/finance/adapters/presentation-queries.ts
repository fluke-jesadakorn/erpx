import "server-only";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { financeEntrypointQueries } from "@/server/modules/finance/public/entrypoint-queries";

type ReadArguments<F extends (...args: never[]) => unknown> = Parameters<F> extends [unknown, ...infer Rest] ? Rest : never;

/** Native composition for presentation readers; the supplied Holding connection stays private. */
export async function financePresentationQueries(holdingId: string) {
  const { write } = await holdingResources(holdingId);
  return {
    financeEntrypointQueries: (...args: ReadArguments<typeof financeEntrypointQueries>) => financeEntrypointQueries(write, ...args),
  };
}
