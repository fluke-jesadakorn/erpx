import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { ApiError } from '@erpx/server-runtime/public/http';
import {
  assetPurchaseControls,
  withAssetPurchasedCommand,
  withAssetPurchasedRead,
} from '@erpx/server-assets/public/purchases';
import {
  nativeAssetCommand,
  nativeAssetDetail,
  nativeAssetWorkspace,
  nativeAssetsAvailable,
  nativeAssetsFailure,
} from './native-assets-page';

function ownedFailure(error: unknown): never {
  const failure = nativeAssetsFailure(error);
  if (failure) throw new ApiError(failure.status, failure.code, failure.message);
  throw error;
}

function unavailable(): never {
  throw new ApiError(
    503,
    'NATIVE_ASSETS_UNAVAILABLE',
    'The native Assets service is unavailable.',
  );
}

async function purchasedRead(actor: WorkContext, capability: 'assets.register' | 'assets.maintenance') {
  try {
    await withAssetPurchasedRead(actor, capability, async () => undefined);
  } catch (error) {
    ownedFailure(error);
  }
}

/** Native workspace plus the retained purchase controls used only to gate the
 * current UI. The native owner read still enforces its own current authority. */
export async function ownedAssetWorkspace(actor: WorkContext, holdingSlug: string) {
  if (!nativeAssetsAvailable()) return undefined;
  await purchasedRead(actor, 'assets.register');
  let data;
  try {
    data = await nativeAssetWorkspace(actor, holdingSlug);
  } catch (error) {
    ownedFailure(error);
  }
  if (!data) return undefined;
  try {
    return { ...data, purchases: await assetPurchaseControls(actor) };
  } catch (error) {
    ownedFailure(error);
  }
}

export async function ownedAssetDetail(actor: WorkContext, holdingSlug: string, id: string) {
  if (!nativeAssetsAvailable()) return undefined;
  await purchasedRead(actor, 'assets.register');
  let data;
  try {
    data = await nativeAssetDetail(actor, holdingSlug, id);
  } catch (error) {
    ownedFailure(error);
  }
  if (!data) return undefined;
  try {
    return { ...data, purchases: await assetPurchaseControls(actor) };
  } catch (error) {
    ownedFailure(error);
  }
}

/** Keep the retained Commercial admission check ahead of the native command.
 * Its transaction is a preflight only; the native owner unit remains the sole
 * writer and still re-checks current IAM. */
export async function ownedAssetCommand(
  actor: WorkContext,
  holdingSlug: string,
  input: unknown,
  key: string,
) {
  if (!nativeAssetsAvailable()) return undefined;
  const body = input !== null && typeof input === 'object' ? (input as Record<string, unknown>) : {};
  const change = typeof body.id === 'string' && body.id.length > 0;
  const action = change && typeof body.action === 'string' ? body.action : 'create';
  const capability = !change ? 'assets.register' : action === 'place' ? 'assets.custody' : 'assets.register';
  const operation = change ? `asset.${action}` : 'asset.create';
  const target = change
    ? ({ kind: 'asset', id: String(body.id) } as const)
    : ({ kind: 'asset', id: `asset-${key}` } as const);
  const permissions = [
    'records.write',
    ...(action === 'link-finance' ? ['finance.write'] : []),
    ...(action === 'retire-approve' ? ['records.review'] : []),
  ];
  try {
    await withAssetPurchasedCommand(
      actor,
      {
        key,
        operation,
        capability: capability as Parameters<typeof withAssetPurchasedCommand>[1]['capability'],
        purpose: action === 'place' || !change ? 'new-work' : 'settle-existing',
        target,
        creating: !change,
        permissions,
      },
      async () => undefined,
    );
  } catch (error) {
    ownedFailure(error);
  }
  try {
    const data = await nativeAssetCommand(actor, holdingSlug, input, key);
    if (!data) unavailable();
    return data;
  } catch (error) {
    ownedFailure(error);
  }
}
