import "server-only";
import { customerPortalContext } from "@erpx/server-customer_360/public/customer-portal";
import { customerPortalProjection } from "@erpx/server-customer_360/public/customer-portal";
import {customerPortalProjectionRequest} from '@erpx/server-customer_360/adapters/portal-projection-request';

type ReadArguments<F extends (...args: never[]) => unknown> = Parameters<F> extends [unknown, ...infer Rest] ? Rest : never;

export async function customerPortalPresentationQueries(holdingSlug: string) {
  const { actor } = await customerPortalContext(holdingSlug);
  return { actor, queries: {
    customerPortalProjection: (...args: ReadArguments<typeof customerPortalProjection>) => customerPortalProjectionRequest(holdingSlug,args[0]),
  } };
}
