import "server-only";
import { customerPortalContext } from "@/server/modules/customer_360/public/customer-portal";
import { customerPortalProjection } from "@/server/modules/customer_360/public/customer-portal";

type ReadArguments<F extends (...args: never[]) => unknown> = Parameters<F> extends [unknown, ...infer Rest] ? Rest : never;

export async function customerPortalPresentationQueries(holdingSlug: string) {
  const { write, actor } = await customerPortalContext(holdingSlug);
  return { actor, queries: {
    customerPortalProjection: (...args: ReadArguments<typeof customerPortalProjection>) => customerPortalProjection(write, ...args),
  } };
}
