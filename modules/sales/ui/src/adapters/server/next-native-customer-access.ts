import 'server-only';
import { headers } from 'next/headers';

/** Resolves the retained incoming headers for a native owner call. */
export function createNextNativeCustomerAccessReader() {
  return async <T>(
    load: (incoming: Pick<Headers, 'get'>, signal?: AbortSignal) => Promise<T>,
    signal?: AbortSignal,
  ): Promise<T> => load(await headers(), signal);
}
