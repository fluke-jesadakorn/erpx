import 'server-only';
import type { BillingView } from '@erpx/feature-billing/lib/billing';
import type { BillingInput } from '@erpx/server-billing/domain/billing';
import { headers } from 'next/headers';
import { ApiError } from '@erpx/server-runtime/public/http';
import {
  createNativeBillingClient,
  nativeBillingAvailable,
  nativeBillingCommandRequest,
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

/**
 * Native cost-review commands (`cost`, `delete-cost`, `complete-costs`). These
 * are native-authoritative: when the exact private configuration is absent the
 * caller receives `undefined` and the route fails closed with a typed 503
 * instead of running the retained TypeScript writer.
 *
 * Every other Billing action, including the target-evidence/provider/rollout/
 * rate actions and `retry`, stays on the TypeScript command: their native
 * commands authorize the holding owner, which would invert the retained
 * platform-only contract.
 */
export async function nativeBillingCostCommand(
  actor: Omit<NativeBillingPageContext, 'holdingSlug'>,
  holdingId: string,
  input: BillingInput,
  key: string,
): Promise<{ ok: boolean; url?: string } | undefined> {
  const instance = client();
  if (!instance) return undefined;
  const holding = instance.holding(holdingId);
  if (!holding)
    throw new ApiError(
      503,
      'NATIVE_BILLING_UNAVAILABLE',
      'The native Billing Holding target is not configured.',
    );
  const request = nativeBillingCommandRequest(input);
  if (!request) return undefined;
  try {
    return await instance.command(
      await headers(),
      { ...actor, holdingSlug: holding.holdingSlug },
      {
        holdingId: holding.holdingId,
        holdingSlug: holding.holdingSlug,
        action: request.action,
        key,
        body: request.body,
      },
    );
  } catch (error) {
    ownedFailure(error);
  }
}


export async function nativeBillingInvoiceCommand(
  actor: Omit<NativeBillingPageContext, 'holdingSlug'>,
  holdingId: string,
  route: string,
  body: unknown,
  key: string,
): Promise<{ ok: boolean } | undefined> {
  const instance = client();
  if (!instance) return undefined;
  const holding = instance.holding(holdingId);
  if (!holding)
    throw new ApiError(
      503,
      'NATIVE_BILLING_UNAVAILABLE',
      'The native Billing Holding target is not configured.',
    );
  if (!/^(\/[a-zA-Z0-9_-]{1,80}\/void)?$/.test(route))
    throw new ApiError(503, 'NATIVE_BILLING_REQUEST_INVALID', 'Invalid invoice route.');
  try {
    return await instance.invoice(
      await headers(),
      { ...actor, holdingSlug: holding.holdingSlug },
      { holdingId: holding.holdingId, holdingSlug: holding.holdingSlug, route, body, key },
    );
  } catch (error) {
    ownedFailure(error);
  }
}

/** Convert an owner/transport failure for a retained caller. */
export function nativeBillingOwnedFailure(error: unknown): never {
  if (error instanceof NativeBillingError) ownedFailure(error);
  throw error;
}
