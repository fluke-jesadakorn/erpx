import 'server-only';
import { headers } from 'next/headers';
import type {
  NativeBrandIdentityCommandInput,
  NativeBrandIdentityReadInput,
  NativeBrandIdentityWorkspace,
} from './native-brand-identity';

/** One actual Next request snapshot. No preliminary session query or cached
 * cross-request actor is created here. */
export function createNextNativeBrandIdentityReader(
  read: (
    incoming: Pick<Headers, 'get'>,
    input: NativeBrandIdentityReadInput,
    signal?: AbortSignal,
  ) => Promise<NativeBrandIdentityWorkspace>,
) {
  return async (input: NativeBrandIdentityReadInput, signal?: AbortSignal) =>
    read(await headers(), input, signal);
}

export function createNextNativeBrandIdentityCommand(
  command: (
    incoming: Pick<Headers, 'get'>,
    input: NativeBrandIdentityCommandInput,
  ) => Promise<Record<string, unknown>>,
) {
  return async (input: Omit<NativeBrandIdentityCommandInput, 'origin'>) => {
    const incoming = await headers();
    return command(incoming, { ...input, origin: incoming.get('origin') ?? undefined });
  };
}
