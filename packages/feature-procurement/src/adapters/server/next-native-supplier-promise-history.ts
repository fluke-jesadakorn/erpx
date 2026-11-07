import 'server-only';
import { headers } from 'next/headers';
import type { NativeSupplierPromiseHistoryReadInput } from '../../public/native-supplier-promise-history';
import type { NativeSupplierPromiseHistory } from '../../lib/native-supplier-promise-history-types';

export function createNextNativeSupplierPromiseHistoryReader(
  read: (
    incoming: Pick<Headers, 'get'>,
    orderId: string,
    input: NativeSupplierPromiseHistoryReadInput,
    signal?: AbortSignal,
  ) => Promise<NativeSupplierPromiseHistory>,
) {
  return async (orderId: string, input: NativeSupplierPromiseHistoryReadInput, signal?: AbortSignal) =>
    read(await headers(), orderId, input, signal);
}
