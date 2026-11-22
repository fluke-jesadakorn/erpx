import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { holdingResources } from '@erpx/server-runtime/public/runtime';
import { ApiError } from '@erpx/server-runtime/public/http';
import { requireProcurementPurchasedRead } from '@erpx/server-procurement/public/purchases';
import { NativeProcurementError } from './native-supplier-promise-history';
import {
  captureNativeSupplierBill,
  decideNativeSupplierBill,
  saveNativeSupplierBillPolicy,
  nativeProcurementAvailable,
  type NativeSupplierBillReceipt,
} from './native-supplier-bill-command';

function ownedFailure(error: unknown): never {
  if (error instanceof NativeProcurementError) {
    // The native denial carries the owner code/message; preserve them so the
    // route keeps the exact status/payload contract of the retained writer.
    if (error.code === 'NATIVE_PROCUREMENT_CONTEXT_CHANGED')
      throw new ApiError(
        409,
        'WORK_CONTEXT_CHANGED',
        'Your work role changed. Reload and review before retrying.',
      );
    if (typeof error.status === 'number' && error.status >= 400 && error.status < 500)
      throw new ApiError(error.status, error.code, error.message);
    throw new ApiError(
      503,
      'NATIVE_PROCUREMENT_UNAVAILABLE',
      'The native Procurement service is unavailable.',
    );
  }
  throw error;
}

/** Keep the retained Commercial supply purchase and current Identity
 * authority preflight ahead of the native command. The native owner command
 * still enforces its own current actor scope; no silent TypeScript writer is
 * used as a fallback once the native configuration is present. */
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

/** Native supplier-bill capture with the same module/purchase preflight as
 * the retained command. Once configured, a failure never falls back to the
 * retained writer. */
export async function ownedCaptureSupplierBill(
  actor: WorkContext,
  holdingSlug: string,
  body: unknown,
  key: string,
): Promise<NativeSupplierBillReceipt | undefined> {
  if (!nativeProcurementAvailable()) return undefined;
  await admission(actor);
  try {
    const receipt = await captureNativeSupplierBill(actor, holdingSlug, key, body);
    if (!receipt)
      throw new ApiError(
        503,
        'NATIVE_PROCUREMENT_UNAVAILABLE',
        'The native Procurement service is unavailable.',
      );
    return receipt;
  } catch (error) {
    ownedFailure(error);
  }
}

/** Native supplier-bill decision (approve/return) with the same preflight.
 * Once configured, a failure never falls back to the retained writer. */
export async function ownedDecideSupplierBill(
  actor: WorkContext,
  holdingSlug: string,
  id: string,
  body: unknown,
  key: string,
): Promise<NativeSupplierBillReceipt | undefined> {
  if (!nativeProcurementAvailable()) return undefined;
  await admission(actor);
  try {
    const receipt = await decideNativeSupplierBill(actor, holdingSlug, id, key, body);
    if (!receipt)
      throw new ApiError(
        503,
        'NATIVE_PROCUREMENT_UNAVAILABLE',
        'The native Procurement service is unavailable.',
      );
    return receipt;
  } catch (error) {
    ownedFailure(error);
  }
}

/** Native supplier-invoice policy save with the same preflight. Once
 * configured, a failure never falls back to the retained writer. */
export async function ownedSaveSupplierBillPolicy(
  actor: WorkContext,
  holdingSlug: string,
  body: unknown,
  key: string,
): Promise<NativeSupplierBillReceipt | undefined> {
  if (!nativeProcurementAvailable()) return undefined;
  await admission(actor);
  try {
    const receipt = await saveNativeSupplierBillPolicy(actor, holdingSlug, key, body);
    if (!receipt)
      throw new ApiError(
        503,
        'NATIVE_PROCUREMENT_UNAVAILABLE',
        'The native Procurement service is unavailable.',
      );
    return receipt;
  } catch (error) {
    ownedFailure(error);
  }
}
