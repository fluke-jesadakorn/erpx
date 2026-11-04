import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { holdingResources } from '@erpx/server-runtime/public/runtime';
import { ApiError } from '@erpx/server-runtime/public/http';
import { hasPermission } from '@erpx/feature-identity_access/lib/permissions';
import { requireManufacturingPurchasedRead } from '@erpx/server-manufacturing/public/purchases';
import {
  nativePlanningAvailable,
  nativePlanningPageContext,
  nativePlanningFailure,
  nativePlanningWorkspace,
  type NativePlanningWorkspace,
} from './native-planning-workspace-page';

function ownedFailure(error: unknown): never {
  const failure = nativePlanningFailure(error);
  if (failure) throw new ApiError(failure.status, failure.code, failure.message);
  throw error;
}

function unavailable(): never {
  throw new ApiError(
    503,
    'NATIVE_PLANNING_UNAVAILABLE',
    'The native Planning service is unavailable.',
  );
}

/** Keep the retained Planning module, records.read and planning.basic package
 * preflight ahead of the native workspace read. The native owner read still
 * enforces its own current actor scope; no silent TypeScript planning list is
 * used as a fallback once the native configuration is present. */
async function admission(actor: WorkContext) {
  if (!actor.modules.includes('planning') || !hasPermission(actor, 'records.read'))
    throw new ApiError(
      403,
      'PLANNING_ACCESS_CHANGED',
      'Choose a current original planning assignment.',
    );
  try {
    const database = (await holdingResources(actor.holdingId)).write;
    await requireManufacturingPurchasedRead(database, actor, {
      module: 'planning',
      capabilities: ['planning.basic'],
    });
  } catch (error) {
    ownedFailure(error);
  }
}

export async function ownedPlanningWorkspace(
  actor: WorkContext,
  holdingSlug: string,
): Promise<NativePlanningWorkspace | undefined> {
  if (!nativePlanningAvailable()) return undefined;
  await admission(actor);
  try {
    const data = await nativePlanningWorkspace(
      nativePlanningPageContext(actor),
      holdingSlug,
    );
    if (!data) unavailable();
    return data;
  } catch (error) {
    ownedFailure(error);
  }
}
