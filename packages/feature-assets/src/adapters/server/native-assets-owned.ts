import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { ApiError } from '@erpx/server-runtime/public/http';
import { withAssetPurchasedCommand } from '@erpx/server-assets/public/purchases';
import {
  nativeAssetCommand,
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
