import 'server-only';
import { headers } from 'next/headers';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';

/** WorkContext-backed boundary for the mounted native Support read port. */
export function createNextNativeSupportReader(actor: WorkContext) {
  return async <T>(
    load: (incoming: Headers, holdingSlug: string) => Promise<T>,
  ): Promise<T> => load(await headers(), actor.holdingSlug);
}
