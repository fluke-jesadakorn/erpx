import 'server-only';
import { headers } from 'next/headers';
import type { NativeManufacturingBomReadInput } from '../../public/native-manufacturing-bom';
import type { NativeManufacturingBom } from '../../lib/native-manufacturing-types';

export function createNextNativeManufacturingBomReader(
  read: (
    incoming: Pick<Headers, 'get'>,
    input: NativeManufacturingBomReadInput,
    signal?: AbortSignal,
  ) => Promise<NativeManufacturingBom>,
) {
  return async (input: NativeManufacturingBomReadInput, signal?: AbortSignal) =>
    read(await headers(), input, signal);
}
