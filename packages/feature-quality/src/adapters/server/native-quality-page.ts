import 'server-only';
import { headers } from 'next/headers';
import {
  createNativeQualityReader,
  type NativeQualityDetail,
  type NativeQualityCancellationCursor,
  type NativeQualityCancellationPage,
  type NativeQualityReceipt,
  type NativeQualityWorkspace,
  type QualityTransport,
} from './native-quality';

/** Selector presence only; unset configuration keeps the explicit retained
 * Quality path instead of probing the private service. */
export function nativeQualityAvailable(): boolean {
  return process.env.ERPX_NATIVE_QUALITY_CONFIG !== undefined;
}

function client(transport?: QualityTransport) {
  const raw = process.env.ERPX_NATIVE_QUALITY_CONFIG;
  if (raw === undefined) return undefined;
  return createNativeQualityReader(raw, transport);
}

export async function nativeQualityWorkspace(
  holdingSlug: string,
  transport?: QualityTransport,
): Promise<NativeQualityWorkspace | undefined> {
  const native = client(transport);
  if (!native) return undefined;
  return native.workspace(await headers(), holdingSlug);
}

export async function nativeQualityDetail(
  holdingSlug: string,
  id: string,
  transport?: QualityTransport,
): Promise<NativeQualityDetail | undefined> {
  const native = client(transport);
  if (!native) return undefined;
  return native.detail(await headers(), holdingSlug, id);
}

export async function nativeQualityCancellationHistory(
  holdingSlug: string,
  id: string,
  cursor?: NativeQualityCancellationCursor,
  transport?: QualityTransport,
): Promise<NativeQualityCancellationPage | undefined> {
  const native = client(transport);
  if (!native) return undefined;
  return native.cancellationHistory(await headers(), holdingSlug, id, cursor);
}

export async function nativeQualityCancel(
  holdingSlug: string,
  id: string,
  input: unknown,
  key: string,
  transport?: QualityTransport,
): Promise<NativeQualityReceipt | undefined> {
  const native = client(transport);
  if (!native) return undefined;
  return native.cancel(await headers(), holdingSlug, id, input, key);
}

export type NativeQualityFailure = Readonly<{
  status: number;
  code: string;
  message: string;
}>;

/** Bounded failure policy for the retained page/route error handling. */
export function nativeQualityFailure(error: unknown): NativeQualityFailure | undefined {
  if (!(error instanceof Error) || !('status' in error) || typeof error.status !== 'number')
    return undefined;
  const code = 'code' in error && typeof error.code === 'string' ? error.code : 'NATIVE_QUALITY_UNAVAILABLE';
  const status = error.status;
  if (status === 401)
    return { status: 401, code: 'UNAUTHENTICATED', message: 'Sign in to continue.' };
  if (status === 403)
    return {
      status: 403,
      code: code === 'NATIVE_QUALITY_UNAVAILABLE' ? 'FORBIDDEN' : code,
      message: error.message,
    };
  if (status === 404)
    return { status: 404, code: 'QUALITY_HOLD_NOT_FOUND', message: 'Quality hold not found in this scope.' };
  if (status === 409 || status === 422)
    return { status, code, message: error.message };
  if (status === 503)
    return {
      status: 503,
      code: 'NATIVE_QUALITY_UNAVAILABLE',
      message: 'The native Quality service is unavailable.',
    };
  return undefined;
}
