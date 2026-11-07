import 'server-only';
import { headers } from 'next/headers';
import type {
  NativeSalesDocumentsPage,
  NativeSalesDocumentsReadInput,
} from '../../public/native-sales-documents';

export function createNextNativeSalesDocumentsReader(
  read: (
    incoming: Pick<Headers, 'get'>,
    input: NativeSalesDocumentsReadInput,
    signal?: AbortSignal,
  ) => Promise<NativeSalesDocumentsPage>,
) {
  return async (input: NativeSalesDocumentsReadInput, signal?: AbortSignal) =>
    read(await headers(), input, signal);
}
