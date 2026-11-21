import 'server-only';
import { headers } from 'next/headers';
import type { BillingView } from '@erpx/feature-billing/lib/billing';
import type { NativeBillingPageContext, NativeBillingReadInput } from '../../public/native-billing';

export function createNextNativeBillingReader(
  read: (
    incoming: Pick<Headers, 'get'>,
    actor: NativeBillingPageContext,
    input: NativeBillingReadInput,
    signal?: AbortSignal,
  ) => Promise<BillingView>,
) {
  return async (
    actor: NativeBillingPageContext,
    input: NativeBillingReadInput,
    signal?: AbortSignal,
  ) => read(await headers(), actor, input, signal);
}
