import 'server-only';
import { createHash } from 'node:crypto';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { PublicMarketingForm } from '@erpx/server-marketing/application/public-form';

/**
 * Strict private transport for the mounted native public Marketing form source
 * read. The axum owner owns the statement, the projection and the exact source
 * field set; this bridge emits no SQL, accepts no SQL from a caller and fails
 * closed when configuration is absent or invalid. There is no TypeScript
 * fallback read: presentation and consent rules stay with the caller that
 * consumes this source.
 */
type Holding = Readonly<{ origin: string; holdingSlug: string }>;
type Settings = Readonly<{ base: string; timeoutMs: number; holdings: readonly Holding[] }>;

/** Exact field set pinned by the owner `SOURCE_KEYS`. */
const SOURCE_KEYS = [
  'id',
  'company_id',
  'name',
  'version',
  'preview_json',
  'configuration_json',
] as const;
const MAX_TEXT = 1_048_576;

const unavailable = (): never => {
  throw new ApiError(503, 'MARKETING_PUBLIC_FORM_UNAVAILABLE', 'The native Marketing form service is unavailable.');
};
const invalid = (): never => {
  throw new ApiError(503, 'MARKETING_PUBLIC_FORM_CONFIG_INVALID', 'The native Marketing form transport is misconfigured.');
};
const record = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v);
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const validId = (v: unknown): v is string =>
  typeof v === 'string' && v.length >= 1 && v.length <= 200 && /^[A-Za-z0-9_.:-]+$/.test(v);
const validSlug = (v: unknown): v is string =>
  typeof v === 'string' && /^[a-z0-9_-]{1,128}$/.test(v);
const boundedText = (v: unknown, max: number): v is string =>
  typeof v === 'string' && v.length >= 1 && v.length <= max;

/** Selector presence only; an absent configuration never probes the service. */
export function nativePublicFormSourceConfigured(): boolean {
  return process.env.ERPX_NATIVE_MARKETING_READ_CONFIG !== undefined
    || process.env.ERPX_NATIVE_MARKETING_CONFIG !== undefined;
}

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_MARKETING_READ_CONFIG ?? process.env.ERPX_NATIVE_MARKETING_CONFIG;
  if (raw === undefined) return unavailable();
  let value: unknown;
  try { value = JSON.parse(raw); } catch { return invalid(); }
  if (!record(value) || typeof value.privateBaseUrl !== 'string' || !Array.isArray(value.holdings)) return invalid();
  if (typeof value.timeoutMs !== 'number' || !Number.isSafeInteger(value.timeoutMs) || value.timeoutMs < 250 || value.timeoutMs > 30_000) return invalid();
  let base: URL;
  try { base = new URL(value.privateBaseUrl); } catch { return invalid(); }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) || !base.port ||
    reservedPorts.has(port) || (port >= 3200 && port <= 3299) || base.username || base.password ||
    value.privateBaseUrl !== base.origin
  ) return invalid();
  const holdings = value.holdings.map((item: unknown): Holding => {
    if (!record(item) || typeof item.origin !== 'string' || !validSlug(item.holdingSlug)) return invalid();
    let origin: URL;
    try { origin = new URL(item.origin); } catch { return invalid(); }
    if (origin.protocol !== 'https:' || item.origin !== origin.origin || origin.username || origin.password) return invalid();
    return { origin: item.origin, holdingSlug: item.holdingSlug as string };
  });
  if (holdings.length < 1 || holdings.length > 64) return invalid();
  return { base: base.origin, timeoutMs: value.timeoutMs, holdings };
}

/** Validate the exact owner payload before any field reaches a TypeScript consumer. */
function sourceRow(payload: unknown): PublicMarketingForm {
  const data = record(payload) && record(payload.data) ? payload.data : undefined;
  if (!data) return unavailable();
  const keys = Object.keys(data);
  if (keys.length !== SOURCE_KEYS.length || !SOURCE_KEYS.every((key) => Object.prototype.hasOwnProperty.call(data, key))) return unavailable();
  if (!validId(data.id) || !validId(data.company_id)) return unavailable();
  if (!boundedText(data.name, 512) || !boundedText(data.preview_json, MAX_TEXT) || !boundedText(data.configuration_json, MAX_TEXT)) return unavailable();
  if (typeof data.version !== 'number' || !Number.isSafeInteger(data.version) || data.version < 0) return unavailable();
  return {
    id: data.id,
    company_id: data.company_id,
    name: data.name,
    version: data.version,
    preview_json: data.preview_json,
    configuration_json: data.configuration_json,
  };
}

/**
 * Read one active public form source from the owner. The Holding is selected by
 * its exact registered slug (no default); the owner matches the same slug and
 * forwarded host to its own registered pool. Missing config, a malformed
 * response or any owner failure fails closed with no TypeScript read fallback.
 */
export async function readNativePublicFormSource(holdingSlug: string, formId: string): Promise<PublicMarketingForm> {
  if (!validSlug(holdingSlug) || !validId(formId)) {
    throw new ApiError(422, 'MARKETING_PUBLIC_FORM_INVALID', 'Use a bounded public form id.');
  }
  const config = configuration();
  const matches = config.holdings.filter((holding) => holding.holdingSlug === holdingSlug);
  if (matches.length !== 1) throw new ApiError(409, 'WORK_CONTEXT_CHANGED', 'The selected Holding is not registered.');
  const target = matches[0]!;
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(holdingSlug)}/marketing/public-form/${encodeURIComponent(formId)}/source`, config.base);
  const headers = new Headers({
    host: new URL(target.origin).host,
    origin: target.origin,
    accept: 'application/json',
    'cache-control': 'no-cache',
    'idempotency-key': 'mkt-pub-src-' + createHash('sha256').update(holdingSlug).update('\u0000').update(formId).digest('hex').slice(0, 32),
  });
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'GET',
      headers,
      cache: 'no-store',
      redirect: 'error',
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch { return unavailable(); }
  if (response.status !== 200) {
    const payload: unknown = await response.json().catch((): unknown => null);
    void response.body?.cancel().catch(() => undefined);
    const err = record(payload) && record(payload.error) ? payload.error : undefined;
    if (response.status >= 400 && response.status < 500) {
      throw new ApiError(
        response.status,
        typeof err?.code === 'string' ? err.code : 'MARKETING_PUBLIC_FORM_READ_FAILED',
        typeof err?.message === 'string' ? err.message : 'The native Marketing form read was rejected.',
      );
    }
    return unavailable();
  }
  const payload: unknown = await response.json().catch((): unknown => null);
  return sourceRow(payload);
}
