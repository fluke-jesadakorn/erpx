import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { ApiError } from '@erpx/server-runtime/public/http';
import {
  assetPurchaseControls,
  withAssetPurchasedCommand,
  withAssetPurchasedRead,
} from '@erpx/server-assets/public/purchases';
import type {
  MaintenanceDetailView,
  MaintenanceWorkspaceView,
  PreventivePlanDetailView,
  PreventivePlanListView,
} from '../../lib/maintenance-view';
import {
  nativeMaintenanceAvailable,
  nativeMaintenanceCommand,
  nativeMaintenanceDetail,
  nativeMaintenanceFailure,
  nativeMaintenancePlanDetail,
  nativeMaintenancePlans,
  nativeMaintenanceWorkspace,
} from './native-maintenance-page';

/** Native transition selection is configuration presence only. A malformed
 * setting still selects the native path so its failure closes instead of
 * silently falling back to the retained TypeScript writer. Creation remains on
 * the retained TypeScript application until its native composition is mounted. */
export function nativeMaintenanceTransitionAvailable(): boolean {
  return nativeMaintenanceAvailable();
}

function ownedFailure(error: unknown): never {
  const failure = nativeMaintenanceFailure(error);
  if (failure) throw new ApiError(failure.status, failure.code, failure.message);
  throw error;
}

function unavailable(): never {
  throw new ApiError(
    503,
    'NATIVE_MAINTENANCE_UNAVAILABLE',
    'The native Maintenance service is unavailable.',
  );
}

async function purchasedRead(actor: WorkContext, capability: 'assets.maintenance' | 'assets.preventive') {
  try {
    await withAssetPurchasedRead(actor, capability, async () => undefined);
  } catch (error) {
    ownedFailure(error);
  }
}

/** Native work workspace plus retained purchase controls for UI gating only. */
export async function ownedMaintenanceWorkspace(actor: WorkContext, holdingSlug: string) {
  if (!nativeMaintenanceAvailable()) return undefined;
  await purchasedRead(actor, 'assets.maintenance');
  let data;
  try {
    data = await nativeMaintenanceWorkspace(actor, holdingSlug);
  } catch (error) {
    ownedFailure(error);
  }
  if (!data) return undefined;
  try {
    return { ...data, purchases: await assetPurchaseControls(actor) } as MaintenanceWorkspaceView;
  } catch (error) {
    ownedFailure(error);
  }
}

export async function ownedMaintenanceDetail(actor: WorkContext, holdingSlug: string, id: string) {
  if (!nativeMaintenanceAvailable()) return undefined;
  await purchasedRead(actor, 'assets.maintenance');
  let data;
  try {
    data = await nativeMaintenanceDetail(actor, holdingSlug, id);
  } catch (error) {
    ownedFailure(error);
  }
  if (!data) return undefined;
  try {
    return { ...data, purchases: await assetPurchaseControls(actor) } as MaintenanceDetailView;
  } catch (error) {
    ownedFailure(error);
  }
}

export async function ownedMaintenancePlans(actor: WorkContext, holdingSlug: string) {
  if (!nativeMaintenanceAvailable()) return undefined;
  await purchasedRead(actor, 'assets.preventive');
  try {
    return await nativeMaintenancePlans(actor, holdingSlug) as PreventivePlanListView;
  } catch (error) {
    ownedFailure(error);
  }
}

export async function ownedMaintenancePlanDetail(actor: WorkContext, holdingSlug: string, id: string) {
  if (!nativeMaintenanceAvailable()) return undefined;
  await purchasedRead(actor, 'assets.preventive');
  let data;
  try {
    data = await nativeMaintenancePlanDetail(actor, holdingSlug, id);
  } catch (error) {
    ownedFailure(error);
  }
  if (!data) return undefined;
  try {
    return { ...data, purchases: await assetPurchaseControls(actor) } as PreventivePlanDetailView;
  } catch (error) {
    ownedFailure(error);
  }
}

/** Keep the retained Commercial admission check ahead of the native command.
 * The native owner unit remains the sole writer and re-checks current IAM. */
export async function ownedMaintenanceCommand(
  actor: WorkContext,
  holdingSlug: string,
  input: unknown,
  key: string,
) {
  if (!nativeMaintenanceAvailable()) return undefined;
  const body = input !== null && typeof input === 'object' ? (input as Record<string, unknown>) : {};
  const change = typeof body.id === 'string' && body.id.length > 0;
  const action = change && typeof body.action === 'string' ? body.action : 'create';
  const capability = action === 'issue-part' ? 'assets.parts-costs' : 'assets.maintenance';
  const operation = change ? `maintenance.${action}` : 'maintenance.create';
  const target = change
    ? ({ kind: 'work', id: String(body.id) } as const)
    : ({ kind: 'work', id: `maintenance-${key}` } as const);
  try {
    await withAssetPurchasedCommand(
      actor,
      {
        key,
        operation,
        capability: capability as Parameters<typeof withAssetPurchasedCommand>[1]['capability'],
        purpose: change ? 'settle-existing' : 'new-work',
        target,
        creating: !change,
        permissions: ['records.write'],
      },
      async () => undefined,
    );
  } catch (error) {
    ownedFailure(error);
  }
  try {
    const data = await nativeMaintenanceCommand(actor, holdingSlug, input, key);
    if (!data) unavailable();
    return data;
  } catch (error) {
    ownedFailure(error);
  }
}
