import 'server-only';
import { ApiError } from '@erpx/platform-runtime-server/public/http';

/**
 * Multi-holding private transport for the lane-35 owner-scope read. One config
 * carries every registered Holding; a caller must supply its WorkContext
 * `(holdingId, holdingSlug)`, or an explicitly registered `origin`. There is no
 * positional/first-Holding fallback. The axum owner executes the SQL and owns
 * the scope shape. No SQL, no fallback: any absent or invalid configuration,
 * unmatched identity, rejected response or malformed payload fails closed.
 */
export type Holding = Readonly<{
  holdingId: string;
  holdingSlug: string;
  host: string;
  origin: string;
  companyId?: string;
}>;

/** WorkContext identity the caller must supply to select a Holding. */
export type HoldingIdentity = Readonly<{
  holdingId: string;
  holdingSlug: string;
}>;

export type HoldingsConfig = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: readonly Holding[];
}>;

export type OwnerScopeRow = Readonly<{
  companyId: string;
  branchId: string | null;
  departmentId: string | null;
  version: number;
}>;

export type Transport = (
  url: URL,
  headers: Record<string, string>,
  body: string,
  signal: AbortSignal,
) => Promise<Response>;

const ENV = 'ERPX_NATIVE_RECORDS_LANE35_HOLDINGS_CONFIG';
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

const unavailable = (): never => {
  throw new ApiError(
    503,
    'NATIVE_RECORDS_OWNER_SCOPE_UNAVAILABLE',
    'The native Records owner-scope read is unavailable.',
  );
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max = 8192): value is string =>
  typeof value === 'string' && Buffer.byteLength(value) <= max;

function configuration(raw: string | undefined): HoldingsConfig {
  if (raw === undefined) return unavailable();
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailable();
  }
  if (!record(value) || !text(value.privateBaseUrl, 512)) return unavailable();
  const timeoutMs = value.timeoutMs;
  if (
    typeof timeoutMs !== 'number' ||
    !Number.isSafeInteger(timeoutMs) ||
    timeoutMs < 250 ||
    timeoutMs > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return unavailable();
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return unavailable();
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    RESERVED_PORTS.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    base.origin !== value.privateBaseUrl
  )
    return unavailable();
  const holdings: Holding[] = [];
  for (const item of value.holdings) {
    if (
      !record(item) ||
      !text(item.holdingId, 200) ||
      item.holdingId.length === 0 ||
      !text(item.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(item.holdingSlug) ||
      !text(item.host, 512) ||
      item.host.length === 0 ||
      !text(item.origin, 512) ||
      (item.companyId !== undefined && !text(item.companyId, 200))
    )
      return unavailable();
    let origin: URL;
    try {
      origin = new URL(item.origin);
    } catch {
      return unavailable();
    }
    if (
      origin.protocol !== 'https:' ||
      origin.origin !== item.origin ||
      origin.username ||
      origin.password ||
      origin.host !== item.host
    )
      return unavailable();
    holdings.push({
      holdingId: item.holdingId,
      holdingSlug: item.holdingSlug,
      host: item.host,
      origin: origin.origin,
      ...(item.companyId !== undefined ? { companyId: item.companyId } : {}),
    });
  }
  return { privateBaseUrl: base.origin, timeoutMs, holdings };
}

/**
 * Select the one exact registered Holding. A WorkContext must match both
 * holdingId and holdingSlug; without one the caller must name a registered
 * origin. Never fall back to a positional or first Holding.
 */
export function selectHolding(
  config: HoldingsConfig,
  context?: HoldingIdentity,
  origin?: string,
): Holding {
  if (context) {
    if (
      !text(context.holdingId, 200) ||
      context.holdingId.length === 0 ||
      !text(context.holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(context.holdingSlug)
    )
      return unavailable();
    const match = config.holdings.find(
      (holding) =>
        holding.holdingId === context.holdingId && holding.holdingSlug === context.holdingSlug,
    );
    if (!match) return unavailable();
    return match;
  }
  if (origin !== undefined) {
    if (!text(origin, 512)) return unavailable();
    const match = config.holdings.find((holding) => holding.origin === origin);
    if (!match) return unavailable();
    return match;
  }
  return unavailable();
}

const nullableText = (value: unknown): value is string | null =>
  value === null || text(value, 200);

export function validateOwnerScope(value: unknown): OwnerScopeRow | null {
  if (value === null) return null;
  if (
    !record(value) ||
    Object.keys(value).sort().join(',') !== 'branchId,companyId,departmentId,version' ||
    !text(value.companyId, 200) ||
    value.companyId.length === 0 ||
    !nullableText(value.branchId) ||
    !nullableText(value.departmentId) ||
    typeof value.version !== 'number' ||
    !Number.isSafeInteger(value.version) ||
    value.version < 0
  )
    return unavailable();
  return value as unknown as OwnerScopeRow;
}

async function directHttp(
  url: URL,
  headers: Record<string, string>,
  body: string,
  signal: AbortSignal,
): Promise<Response> {
  const response = await globalThis.fetch(url, {
    method: 'POST',
    headers,
    body,
    cache: 'no-store',
    redirect: 'error',
    signal,
  });
  if (response.redirected) return unavailable();
  return response;
}

export function nativeLane35HoldingsConfigured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[ENV] !== undefined;
}

/** Read one legacy attachment owner scope through the selected Holding. */
export async function readNativeAttachmentOwnerScope(
  kind: string,
  id: string,
  context?: HoldingIdentity,
  origin?: string,
  transport: Transport = directHttp,
): Promise<OwnerScopeRow | null> {
  if (!text(kind, 128) || !text(id, 200) || id.length === 0) return unavailable();
  const config = configuration(process.env[ENV]);
  const holding = selectHolding(config, context, origin);
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/records/native-lane-35/attachment-owner-scope`,
    config.privateBaseUrl,
  );
  const headers: Record<string, string> = {
    host: holding.host,
    origin: holding.origin,
    accept: 'application/json',
    'content-type': 'application/json',
    'cache-control': 'no-cache',
  };
  if (holding.companyId !== undefined) headers['x-erpx-context-company'] = holding.companyId;
  const raw = JSON.stringify({ kind, id });
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), config.timeoutMs);
  try {
    const response = await transport(url, headers, raw, controller.signal);
    if (response.status !== 200 || !response.body) {
      void response.body?.cancel().catch(() => undefined);
      if (response.status >= 400 && response.status < 500) {
        const payload = await response.json().catch((): unknown => null);
        const error = record(payload) && record(payload.error) ? payload.error : undefined;
        throw new ApiError(
          response.status,
          text(error?.code, 100) ? String(error!.code) : 'NATIVE_RECORDS_OWNER_SCOPE_REJECTED',
          text(error?.message, 300) ? String(error!.message) : 'The native Records read rejected the request.',
        );
      }
      return unavailable();
    }
    const bodyText = await response.text();
    if (Buffer.byteLength(bodyText) > 1024 * 1024) return unavailable();
    const payload: unknown = JSON.parse(bodyText);
    if (!record(payload) || !record(payload.data) || !('scope' in payload.data)) return unavailable();
    return validateOwnerScope(payload.data.scope);
  } catch (error) {
    if (error instanceof ApiError) throw error;
    return unavailable();
  } finally {
    clearTimeout(timer);
  }
}
