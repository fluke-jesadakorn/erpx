import 'server-only';
import { headers } from 'next/headers';
import type { NativeSupplierPromiseHistoryReadInput } from '../../public/native-supplier-promise-history';

export function createNextNativeSupplierPromiseHistoryReader<T>(
  read: (
    incoming: Pick<Headers, 'get'>,
    orderId: string,
    input: NativeSupplierPromiseHistoryReadInput,
    signal?: AbortSignal,
  ) => Promise<T>,
) {
  return async (orderId: string, input: NativeSupplierPromiseHistoryReadInput, signal?: AbortSignal) =>
    read(await headers(), orderId, input, signal);
}

export function createNextNativeSupplierPromiseCommandWriter<T>(
  write: (
    incoming: Pick<Headers, 'get'>,
    orderId: string,
    input: NativeSupplierPromiseHistoryReadInput,
    key: string,
    body: unknown,
    signal?: AbortSignal,
  ) => Promise<T>,
) {
  return async (
    orderId: string,
    input: NativeSupplierPromiseHistoryReadInput,
    key: string,
    body: unknown,
    signal?: AbortSignal,
  ) => write(await headers(), orderId, input, key, body, signal);
}
