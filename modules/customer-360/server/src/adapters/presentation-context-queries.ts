import "server-only";
import { customerPortalContext } from "@erpx/customer-360-server/public/customer-portal";
import { customerPortalProjection } from "@erpx/customer-360-server/public/customer-portal";
import {customerPortalProjectionRequest} from '@erpx/customer-360-server/adapters/portal-projection-request';

type ReadArguments<F extends (...args: never[]) => unknown> = Parameters<F> extends [unknown, ...infer Rest] ? Rest : never;

export async function customerPortalPresentationQueries(holdingSlug: string) {
  const { actor } = await customerPortalContext(holdingSlug);
  return { actor, queries: {
    customerPortalProjection: (...args: ReadArguments<typeof customerPortalProjection>) => customerPortalProjectionRequest(holdingSlug,args[0]),
  } };
}
