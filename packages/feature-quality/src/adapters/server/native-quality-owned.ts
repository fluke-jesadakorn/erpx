import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { ApiError } from '@erpx/server-runtime/public/http';
import { holdingResources } from '@erpx/server-runtime/public/runtime';
import { withQualityPurchasedCommand } from '@erpx/server-quality/public/purchases';
import {
  nativeQualityAvailable,
  nativeQualityCancel,
} from './native-quality-page';
import type { NativeQualityReceipt } from './native-quality';

/** Keep the retained Commercial purchase and current-authority admission ahead
 * of the native owner command. The preflight transaction is not the writer;
 * the proposed composition backend must still recheck both facts in the
 * serializable Quality unit. */
export async function ownedNativeQualityCancel(
  actor: WorkContext,
  holdingSlug: string,
  id: string,
  input: unknown,
  key: string,
): Promise<NativeQualityReceipt | undefined> {
  if (!nativeQualityAvailable()) return undefined;
  const action =
    input !== null && typeof input === 'object' && 'action' in input
      ? input.action
      : undefined;
  if (action !== 'request' && action !== 'approve' && action !== 'reject')
    throw new ApiError(
      422,
      'QUALITY_CANCELLATION_REQUIRED',
      'Review the current Quality hold, action, version and reason.',
    );
  const { write } = await holdingResources(actor.holdingId);
  await withQualityPurchasedCommand(
    write,
    actor,
    {
      key,
      operation: `quality.hold.cancel.${action}`,
      target: { kind: 'hold', id },
      purpose: 'settle-existing',
      permissions: [action === 'request' ? 'quality.hold' : 'quality.release'],
      existing: true,
    },
    async () => undefined,
  );
  return nativeQualityCancel(holdingSlug, id, input, key);
}
