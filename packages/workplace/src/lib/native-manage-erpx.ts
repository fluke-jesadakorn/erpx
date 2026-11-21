import 'server-only';
import { headers } from 'next/headers';
import { ApiError } from '@erpx/server-runtime/public/http';
import {
  createNativeManageErpxClient,
  nativeManageErpxConfiguration,
  NativeManageErpxError,
  nativeManageErpxFailure,
  type NativeManageErpxOnboardingReceipt,
  type NativeManageErpxOnboardingView,
} from '@erpx/feature-platform_administration/public/native-manage-erpx';

function client() {
  const raw = process.env.ERPX_NATIVE_MANAGE_ERPX_CONFIG;
  if (raw === undefined) return undefined;
  return createNativeManageErpxClient(nativeManageErpxConfiguration(raw));
}

/** Selector presence only; unset configuration keeps the explicit fail-closed
 * route/page policy without probing the private service. */
export function nativeManageErpxAvailable(): boolean {
  return process.env.ERPX_NATIVE_MANAGE_ERPX_CONFIG !== undefined;
}

function unavailable(error: unknown): never {
  if (error instanceof NativeManageErpxError) {
    const mapped = nativeManageErpxFailure(error);
    if (mapped) throw new ApiError(mapped.status, mapped.code, mapped.message);
    throw new ApiError(error.status ?? 503, error.code, error.message);
  }
  throw new ApiError(
    503,
    'NATIVE_MANAGE_ERPX_UNAVAILABLE',
    'The native Manage ERPX service is unavailable.',
  );
}

export async function nativeManageErpxOnboardingView(
  incoming: Pick<Headers, 'get'>,
  userId: string,
  signal?: AbortSignal,
): Promise<NativeManageErpxOnboardingView | undefined> {
  let native;
  try {
    native = client();
  } catch (error) {
    return unavailable(error);
  }
  if (!native) return undefined;
  try {
    return await native.onboardingView(incoming, userId, signal);
  } catch (error) {
    return unavailable(error);
  }
}

export async function nativeManageErpxOnboardingViewForPage(
  userId: string,
  signal?: AbortSignal,
): Promise<NativeManageErpxOnboardingView | undefined> {
  return nativeManageErpxOnboardingView(await headers(), userId, signal);
}

/** Dispatch the verified onboarding POST to the native owner. A missing
 * configuration is reported as unavailable so the route fails closed instead of
 * falling back to the retired TypeScript onboarding path. */
export async function nativeManageErpxOnboardingCommand(
  incoming: Pick<Headers, 'get'>,
  payload: unknown,
  signal?: AbortSignal,
): Promise<NativeManageErpxOnboardingReceipt | undefined> {
  let native;
  try {
    native = client();
  } catch (error) {
    return unavailable(error);
  }
  if (!native) return undefined;
  try {
    return await native.onboardingCommand(incoming, payload, signal);
  } catch (error) {
    return unavailable(error);
  }
}
