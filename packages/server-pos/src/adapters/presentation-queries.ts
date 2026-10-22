import "server-only";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { stripeEntrypointFacts } from "@erpx/server-pos/public/stripe-entrypoint-facts";

type ReadArguments<F extends (...args: never[]) => unknown> = Parameters<F> extends [unknown, ...infer Rest] ? Rest : never;

/** Native composition for presentation readers; the supplied Holding connection stays private. */
export async function posPresentationQueries(holdingId: string) {
  const { write } = await holdingResources(holdingId);
  return {
    stripeEntrypointFacts: (...args: ReadArguments<typeof stripeEntrypointFacts>) => stripeEntrypointFacts(write, ...args),
  };
}
