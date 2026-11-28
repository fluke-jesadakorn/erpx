import 'server-only';
import { headers } from 'next/headers';
import type {
  NativeEcommerceCatalog,
  NativeEcommerceCatalogReadInput,
} from '../../public/native-ecommerce-catalog';

export function createNextNativeEcommerceCatalogReader(
  read: (
    incoming: Pick<Headers, 'get'>,
    input: NativeEcommerceCatalogReadInput,
    signal?: AbortSignal,
  ) => Promise<NativeEcommerceCatalog>,
) {
  return async (input: NativeEcommerceCatalogReadInput, signal?: AbortSignal) =>
    read(await headers(), input, signal);
}
