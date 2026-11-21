import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { holdingResources } from '@erpx/server-runtime/public/runtime';
import { ApiError } from '@erpx/server-runtime/public/http';
import { hasPermission } from '@erpx/feature-identity_access/lib/permissions';
import { requireManufacturingPurchasedRead } from '@erpx/server-manufacturing/public/purchases';
import {
  nativeManufacturingAvailable,
  nativeManufacturingBom,
  nativeManufacturingBomPageContext,
  nativeManufacturingFailure,
  type NativeManufacturingBom,
} from './native-manufacturing-bom-page';

function ownedFailure(error: unknown): never {
  const failure = nativeManufacturingFailure(error);
  if (failure) throw new ApiError(failure.status, failure.code, failure.message);
  throw error;
}

function unavailable(): never {
  throw new ApiError(
    503,
    'NATIVE_MANUFACTURING_UNAVAILABLE',
    'The native Manufacturing service is unavailable.',
  );
}

/** Keep the retained Manufacturing module, records.read and package purchase
 * preflight ahead of the native read. The native owner read still enforces its
 * own current actor scope; no silent TypeScript reader is used as a fallback
 * once the native configuration is present. */
async function admission(actor: WorkContext) {
  if (!actor.modules.includes('manufacturing') || !hasPermission(actor, 'records.read'))
    throw new ApiError(
      403,
      'MANUFACTURING_ACCESS_CHANGED',
      'Choose a current original production assignment.',
    );
  try {
    const database = (await holdingResources(actor.holdingId)).write;
    await requireManufacturingPurchasedRead(database, actor, {
      module: 'manufacturing',
      capabilities: ['manufacturing.bom'],
    });
  } catch (error) {
    ownedFailure(error);
  }
}

export async function ownedManufacturingBom(
  actor: WorkContext,
  holdingSlug: string,
): Promise<NativeManufacturingBom | undefined> {
  if (!nativeManufacturingAvailable()) return undefined;
  await admission(actor);
  try {
    const data = await nativeManufacturingBom(
      nativeManufacturingBomPageContext(actor),
      holdingSlug,
    );
    if (!data) unavailable();
    return data;
  } catch (error) {
    ownedFailure(error);
  }
}
