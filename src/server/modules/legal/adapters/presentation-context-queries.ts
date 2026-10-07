import "server-only";
import { portalContext } from "@/server/modules/legal/public/legal/portal";
import { portalList } from "@/server/modules/legal/public/legal/portal";

type ReadArguments<F extends (...args: never[]) => unknown> = Parameters<F> extends [unknown, ...infer Rest] ? Rest : never;

export async function legalPortalPresentationQueries(holdingSlug: string) {
  const { write, actor } = await portalContext(holdingSlug);
  return { actor, queries: {
    portalList: (...args: ReadArguments<typeof portalList>) => portalList(write, ...args),
  } };
}
