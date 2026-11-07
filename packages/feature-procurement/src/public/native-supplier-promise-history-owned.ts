import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { holdingResources } from '@erpx/server-runtime/public/runtime';
import { ApiError } from '@erpx/server-runtime/public/http';
import { requireProcurementPurchasedRead } from '@erpx/server-procurement/public/purchases';
import {
  nativeProcurementAvailable,
  nativeProcurementFailure,
  nativeSupplierPromiseHistory,
  nativeSupplierPromiseHistoryPageContext,
  type NativeSupplierPromiseHistory,
} from './native-supplier-promise-history-page';

function ownedFailure(error: unknown): never {
  const failure = nativeProcurementFailure(error);
  if (failure) throw new ApiError(failure.status, failure.code, failure.message);
  throw error;
}

function unavailable(): never {
  throw new ApiError(
    503,
    'NATIVE_PROCUREMENT_UNAVAILABLE',
    'The native Procurement service is unavailable.',
  );
}

/** Keep the retained Commercial supply purchase and current Identity
 * authority preflight ahead of the native read. The native owner read still
 * enforces its own current actor scope; no silent TypeScript reader is used as
 * a fallback once the native configuration is present. */
async function admission(actor: WorkContext) {
  if (!actor.modules.includes('procurement'))
    throw new ApiError(403, 'MODULE_FORBIDDEN', 'Your current role cannot access Procurement.');
  try {
    const database = (await holdingResources(actor.holdingId)).write;
    await requireProcurementPurchasedRead(database, actor);
  } catch (error) {
    ownedFailure(error);
  }
}

export async function ownedSupplierPromiseHistory(
  actor: WorkContext,
  holdingSlug: string,
  orderId: string,
): Promise<NativeSupplierPromiseHistory | undefined> {
  if (!nativeProcurementAvailable()) return undefined;
  await admission(actor);
  try {
    const data = await nativeSupplierPromiseHistory(
      nativeSupplierPromiseHistoryPageContext(actor),
      holdingSlug,
      orderId,
    );
    if (!data) unavailable();
    return data;
  } catch (error) {
    ownedFailure(error);
  }
}
