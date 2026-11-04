import 'server-only';
import type { BillingView } from '@erpx/feature-billing/lib/billing';
import { ApiError } from '@erpx/server-runtime/public/http';
import {
  createNativeBillingClient,
  nativeBillingAvailable,
  nativeBillingConfiguration,
  nativeBillingFailure,
  NativeBillingError,
  type NativeBillingPageContext,
} from './native-billing';
import { createNextNativeBillingReader } from '../adapters/server/next-native-billing';

export { nativeBillingAvailable };
export type { NativeBillingPageContext } from './native-billing';

function client() {
  const raw = process.env.ERPX_NATIVE_BILLING_CONFIG;
  if (raw === undefined) return undefined;
  return createNativeBillingClient(nativeBillingConfiguration(raw));
}

function ownedFailure(error: unknown): never {
  const failure = nativeBillingFailure(error);
  if (failure) throw new ApiError(failure.status, failure.code, failure.message);
  throw error;
}

/** Resolve the configured registered Holding by its immutable id. */
export function nativeBillingHoldingId(holdingId: string) {
  const instance = client();
  if (!instance) return undefined;
  return instance.holding(holdingId);
}

/**
 * Native SSR read cutover. When the exact private configuration is present,
 * the retained Billing view is fetched from the owner router and a missing or
 * malformed native response fails closed. There is no silent TypeScript view
 * fallback once the native configuration exists.
 */
export async function nativeBillingWorkspace(
  actor: Omit<NativeBillingPageContext, 'holdingSlug'>,
): Promise<BillingView | undefined> {
  const instance = client();
  if (!instance) return undefined;
  const holding = instance.holding(actor.holdingId);
  if (!holding)
    throw new ApiError(
      503,
      'NATIVE_BILLING_UNAVAILABLE',
      'The native Billing Holding target is not configured.',
    );
  const pageContext: NativeBillingPageContext = {
    ...actor,
    holdingSlug: holding.holdingSlug,
  };
  try {
    const data = await createNextNativeBillingReader((incoming, context, input, signal) =>
      instance.billing(incoming, context, input, signal),
    )(pageContext, { holdingId: holding.holdingId, holdingSlug: holding.holdingSlug });
    if (!data)
      throw new ApiError(
        503,
        'NATIVE_BILLING_UNAVAILABLE',
        'The native Billing projection is temporarily unavailable.',
      );
    return data;
  } catch (error) {
    ownedFailure(error);
  }
}

/** Convert an owner/transport failure for a retained caller. */
export function nativeBillingOwnedFailure(error: unknown): never {
  if (error instanceof NativeBillingError) ownedFailure(error);
  throw error;
}
