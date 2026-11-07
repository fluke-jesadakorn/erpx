import 'server-only';
import { headers } from 'next/headers';
import type {
  NativeAssetsCommandInput,
  NativeAssetsReadInput,
} from './native-assets';
import type { NativeAssetDetail, NativeAssetWorkspace } from '../../lib/asset-view';

/** One actual Next request snapshot. No preliminary session query or cached
 * cross-request actor is created here. */
export function createNextNativeAssetsReader(
  read: (
    incoming: Pick<Headers, 'get'>,
    input: NativeAssetsReadInput,
    signal?: AbortSignal,
  ) => Promise<NativeAssetWorkspace>,
) {
  return async (input: NativeAssetsReadInput, signal?: AbortSignal) =>
    read(await headers(), input, signal);
}

export function createNextNativeAssetsDetailReader(
  detail: (
    incoming: Pick<Headers, 'get'>,
    input: NativeAssetsReadInput & { id: string },
    signal?: AbortSignal,
  ) => Promise<NativeAssetDetail>,
) {
  return async (input: NativeAssetsReadInput & { id: string }, signal?: AbortSignal) =>
    detail(await headers(), input, signal);
}

export function createNextNativeAssetsCommand(
  command: (
    incoming: Pick<Headers, 'get'>,
    input: NativeAssetsCommandInput,
  ) => Promise<{ id: string; version: number; status: string }>,
) {
  return async (input: Omit<NativeAssetsCommandInput, 'origin'>) => {
    const incoming = await headers();
    return command(incoming, { ...input, origin: incoming.get('origin') ?? undefined });
  };
}
