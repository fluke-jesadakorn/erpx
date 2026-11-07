import 'server-only';
import { headers } from 'next/headers';
import type { AccountSessionResponse } from '@erpx/contracts/account-auth';
import type { NativeSessionRead } from './native-session-http';

/** Read the actual asynchronous Next16.4 request snapshot for every invocation.
 * It never caches across accounts/renders or manufactures a Work/permission DTO.
 * The configured native HTTP read remains the only session source. */
export function createNextNativeSessionReader(read: NativeSessionRead) {
  return async (signal?: AbortSignal): Promise<AccountSessionResponse | null> => {
    const incoming = await headers();
    return read(incoming, signal);
  };
}
