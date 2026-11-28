import 'server-only';
import { headers } from 'next/headers';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import { ApiError } from '@erpx/platform-runtime-server/public/http';

/**
 * Fail-closed transport for the mounted native Maintenance owner-options read.
 * The Rust axum handler at
 * `/api/v1/holdings/<holding>/maintenance/owner-options` owns the statement,
 * projection and Company scope and executes the SQL. This module prepares no
 * SQL, carries no fallback and never reads the retired TypeScript `people`
 * repository branch: a missing configuration, a failed fetch or an unexpected
 * payload surfaces a closed error.
 */
export type MaintenanceOwnerOption = { id: string; name: string };

type Config = {
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: { origin: string; holdingSlug: string; holdingId: string }[];
};

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string => text(value, 200) && value.length > 0;
const slug = (value: unknown): value is string =>
  text(value, 128) && /^[a-z0-9_-]+$/.test(value);

const fail = (status: number, code: string, message: string): never => {
  throw new ApiError(status, code, message);
};
const unavailable = (): never =>
  fail(503, 'NATIVE_MAINTENANCE_OWNER_OPTIONS_UNAVAILABLE', 'The native Maintenance owner-option read is unavailable.');
const invalidConfig = (): never =>
  fail(503, 'NATIVE_MAINTENANCE_CONFIG_INVALID', 'The native Maintenance owner-option transport is misconfigured.');
const invalidResponse = (): never =>
  fail(503, 'NATIVE_MAINTENANCE_OWNER_OPTIONS_RESPONSE_INVALID', 'The native Maintenance owner-option read response is invalid.');

function settings(raw: string): Config {
  let value: unknown;
  try { value = JSON.parse(raw); } catch { return invalidConfig(); }
  if (!record(value) || !text(value.privateBaseUrl) ||
    !Number.isSafeInteger(value.timeoutMs) || (value.timeoutMs as number) < 250 || (value.timeoutMs as number) > 30_000 ||
    !Array.isArray(value.holdings) || value.holdings.length < 1 || value.holdings.length > 64)
    return invalidConfig();
  let base: URL;
  try { base = new URL(value.privateBaseUrl); } catch { return invalidConfig(); }
  if (base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) || !base.port ||
    base.username || base.password || value.privateBaseUrl !== base.origin)
    return invalidConfig();
  for (const holding of value.holdings) {
    if (!record(holding) || typeof holding.origin !== 'string' ||
      !slug(holding.holdingSlug) || !identifier(holding.holdingId))
      return invalidConfig();
    try {
      const origin = new URL(holding.origin);
      if (origin.protocol !== 'https:' || holding.origin !== origin.origin || origin.port)
        return invalidConfig();
    } catch { return invalidConfig(); }
  }
  return value as unknown as Config;
}

function option(value: unknown): value is MaintenanceOwnerOption {
  if (!record(value) || Object.keys(value).length !== 2 ||
    !Object.hasOwn(value, 'id') || !Object.hasOwn(value, 'name'))
    return false;
  return identifier(value.id) && text(value.name, 400);
}

/** One bounded owner-option read. No TypeScript SQL and no fallback path. */
export async function maintenanceOwnerOptions(actor: WorkContext): Promise<MaintenanceOwnerOption[]> {
  const raw = process.env.ERPX_NATIVE_MAINTENANCE_CONFIG;
  if (raw === undefined) return unavailable();
  const config = settings(raw);
  const target = config.holdings.find(h => h.holdingSlug === actor.holdingSlug && h.holdingId === actor.holdingId);
  if (!target || !actor.authority || !actor.iam) return fail(409, 'WORK_CONTEXT_CHANGED', 'Your work role changed. Reload and review before submitting.');
  const incoming = await headers();
  const host = incoming.get('host');
  if (!host || new URL(target.origin).host !== host) return fail(409, 'WORK_CONTEXT_CHANGED', 'Your work role changed. Reload and review before submitting.');
  if (!identifier(actor.companyId)) return invalidResponse();
  const h = new Headers({ host, accept: 'application/json', 'cache-control': 'no-cache' });
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return fail(503, 'NATIVE_MAINTENANCE_READ_COOKIE_LIMIT', 'The native Maintenance owner-option read is unavailable.');
    h.set('cookie', cookie);
  }
  h.set('x-erpx-context-company', actor.companyId);
  h.set('x-erpx-context-assignment', actor.assignmentId);
  h.set('x-erpx-context-revision', String(actor.authority.revision));
  h.set('x-erpx-context-assignment-version', String(actor.authority.assignmentVersion));
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/maintenance/owner-options`, config.privateBaseUrl);
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), config.timeoutMs);
  let response: Response;
  try {
    response = await fetch(url, { method: 'GET', headers: h, signal: controller.signal, redirect: 'error', cache: 'no-store' });
  } catch { return unavailable(); }
  finally { clearTimeout(timer); }
  if (!response.ok || response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return unavailable();
  }
  let parsed: unknown;
  try { parsed = await response.json(); } catch { return invalidResponse(); }
  if (!record(parsed) || Object.keys(parsed).length !== 1 || !Object.hasOwn(parsed, 'data') ||
    !Array.isArray(parsed.data) || parsed.data.length > 1000 || !parsed.data.every(option))
    return invalidResponse();
  return parsed.data as MaintenanceOwnerOption[];
}
