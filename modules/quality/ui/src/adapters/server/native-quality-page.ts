import 'server-only';
import { headers } from 'next/headers';
import {
  createNativeQualityReader,
  type NativeQualityDetailCore,
  type NativeQualityQueueAlert,
  type NativeQualityCancellationCursor,
  type NativeQualityCancellationPage,
  type NativeQualityWorkspace,
  type QualityTransport,
} from './native-quality';

function client(transport?: QualityTransport) {
  const raw = process.env.ERPX_NATIVE_QUALITY_CONFIG;
  if (raw === undefined) return undefined;
  return createNativeQualityReader(raw, transport);
}

/** The complete escalation queue as the Quality workspace presenter consumes
  * it. Purchase visibility is a separate composition admission check. */
export async function nativeQualityAlerts(
  holdingSlug: string,
  transport?: QualityTransport,
): Promise<readonly NativeQualityQueueAlert[] | undefined> {
  const native = client(transport);
  if (!native) return undefined;
  return native.alertQueue(await headers(), holdingSlug);
}

export async function nativeQualityWorkspace(
  holdingSlug: string,
  transport?: QualityTransport,
): Promise<NativeQualityWorkspace | undefined> {
  const native = client(transport);
  if (!native) return undefined;
  return native.workspace(await headers(), holdingSlug);
}

export async function nativeQualityDetailCore(
  holdingSlug: string,
  id: string,
  transport?: QualityTransport,
): Promise<NativeQualityDetailCore | undefined> {
  const native = client(transport);
  if (!native) return undefined;
  return native.detailCore(await headers(), holdingSlug, id);
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
