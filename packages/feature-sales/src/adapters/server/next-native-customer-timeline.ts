import 'server-only';
import { headers } from 'next/headers';
import type {
  NativeCustomerTimelinePage,
  NativeCustomerTimelineReadInput,
} from '../../public/native-customer-timeline';

export function createNextNativeCustomerTimelineReader(
  read: (
    incoming: Pick<Headers, 'get'>,
    input: NativeCustomerTimelineReadInput,
    signal?: AbortSignal,
  ) => Promise<NativeCustomerTimelinePage>,
) {
  return async (input: NativeCustomerTimelineReadInput, signal?: AbortSignal) =>
    read(await headers(), input, signal);
}
