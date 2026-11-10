import 'server-only';
import type { BillingView } from '@erpx/feature-billing/lib/billing';
import type { BillingInput } from '@erpx/server-billing/domain/billing';
import { headers } from 'next/headers';
import { ApiError } from '@erpx/server-runtime/public/http';
import {
  createNativeBillingClient,
  nativeBillingAvailable,
  nativeBillingConfiguration,
  nativeBillingFailure,
  NativeBillingError,
  type NativeBillingCommandInput,
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
 * Native cost commands when the exact private Billing configuration exists.
 * Other Billing actions stay on the retained command path.
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
  let action: NativeBillingCommandInput['action'];
  let body: Record<string, unknown>;
  if (input.action === 'cost') {
    action = 'cost';
    body = {
      version: input.version,
      reason: input.reason,
      periodId: input.periodId,
      category: input.category,
      amountSatang: input.amountSatang,
      serviceStart: input.serviceStart,
      serviceEnd: input.serviceEnd,
      note: input.note ?? '',
      simulated: input.simulated ?? false,
      evidenceId: input.evidenceId ?? null,
      ...(input.costId ? { costId: input.costId } : {}),
      ...(input.originalInvoiceId ? { originalInvoiceId: input.originalInvoiceId } : {}),
    };
  } else if (input.action === 'delete-cost') {
    action = 'delete-cost';
    body = {
      version: input.version,
      reason: input.reason,
      costId: input.costId,
    };
  } else if (input.action === 'complete-costs') {
    action = 'complete-costs';
    body = {
      version: input.version,
      reason: input.reason,
      periodId: input.periodId,
      complete: input.complete,
    };
  } else if (input.action === 'approve-target-rollout') {
    action = 'approve-target-rollout';
    body = {
      version: input.version,
      reason: input.reason,
      targetMonth: input.targetMonth,
      minimumMarginBps: input.minimumMarginBps,
    };
  } else if (input.action === 'verify-provider-cost') {
    action = 'verify-provider-cost';
    body = {
      version: input.version,
      reason: input.reason,
      targetMonth: input.targetMonth,
      sourceKey: input.sourceKey,
      provider: input.provider,
      statementId: input.statementId,
      originalCurrency: input.originalCurrency,
      originalMinor: input.originalMinor,
      fxNumerator: input.fxNumerator,
      fxDenominator: input.fxDenominator,
      providerCostSatang: input.providerCostSatang,
      evidenceId: input.evidenceId,
    };
  } else if (input.action === 'verify-statement') {
    action = 'verify-statement';
    body = {
      version: input.version,
      reason: input.reason,
      targetMonth: input.targetMonth,
      provider: input.provider,
      statementId: input.statementId,
      originalCurrency: input.originalCurrency,
      providerTotalMinor: input.providerTotalMinor,
      evidenceId: input.evidenceId,
    };
  } else if (input.action === 'retry') {
    action = 'retry';
    body = {
      jobId: input.jobId,
      reason: input.reason,
    };
  } else {
    return undefined;
  }
  try {
    return await instance.command(
      await headers(),
      { ...actor, holdingSlug: holding.holdingSlug },
      { holdingId: holding.holdingId, holdingSlug: holding.holdingSlug, action, key, body },
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
