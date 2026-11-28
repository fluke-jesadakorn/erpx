import 'server-only';
import { headers } from 'next/headers';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import { ApiError } from '@erpx/platform-runtime-server/public/http';

/**
 * Strict private transport for the owner-local Maintenance lane-36 attachment
 * descriptor read. The axum owner route is the sole read authority; this
 * bridge emits no SQL and fails closed when the native service is absent or
 * returns anything other than the exact bounded envelope.
 *
 * Every registered Holding is supported. The target is selected from the
 * caller's WorkContext, or, for legacy fact callers without one, from the
 * exact incoming Host. There is no first-Holding default and an unknown
 * target never reaches the private service.
 */
type Row = Record<string, unknown>;
type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
type Config = Readonly<{ privateBaseUrl: string; timeoutMs: number; holdings: readonly Holding[] }>;
export type Descriptor = Readonly<{
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
  version: number;
}>;
export type DescriptorActor = Pick<WorkContext, 'holdingId' | 'holdingSlug' | 'companyId'>;

const ENV = 'ERPX_NATIVE_MAINTENANCE_LANE36_CONFIG';
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const TARGET_TYPE = 'maintenance_work';
const KEYS = ['company_id', 'branch_id', 'department_id', 'version'] as const;

const fail = (
  code = 'NATIVE_MAINTENANCE_LANE36_UNAVAILABLE',
  status = 503,
  message = 'The native Maintenance descriptor read is unavailable.',
): never => {
  throw new ApiError(status, code, message);
};
const record = (value: unknown): value is Row =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value: unknown, keys: readonly string[]): value is Row =>
  record(value) && Object.keys(value).length === keys.length && keys.every(key => Object.hasOwn(value, key));
const text = (value: unknown, max = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const scopeId = (value: unknown): value is string =>
  text(value, 200) && value.length > 0 && /^[A-Za-z0-9_.:-]+$/.test(String(value));
const hostId = (value: unknown): value is string =>
  text(value, 512) && value.length > 0 && !/[\s/?#@]/.test(String(value));
const nullableText = (value: unknown, max = 200): boolean => value === null || text(value, max);
const integer = (value: unknown, min: number): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;

const descriptor = (row: unknown): row is Descriptor =>
  exact(row, KEYS) &&
  scopeId((row as Row).company_id) &&
  nullableText((row as Row).branch_id) &&
  nullableText((row as Row).department_id) &&
  integer((row as Row).version, 1);

function settings(raw: string): Config {
  let value: unknown;
  try { value = JSON.parse(raw); } catch { return fail('NATIVE_MAINTENANCE_LANE36_CONFIG_INVALID'); }
  if (
    !record(value) || !text(value.privateBaseUrl, 512) ||
    !Number.isSafeInteger(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 || (value.timeoutMs as number) > 30_000 ||
    !Array.isArray(value.holdings) || value.holdings.length < 1 || value.holdings.length > 64
  ) return fail('NATIVE_MAINTENANCE_LANE36_CONFIG_INVALID');
  let base: URL;
  try { base = new URL(value.privateBaseUrl as string); } catch { return fail('NATIVE_MAINTENANCE_LANE36_CONFIG_INVALID'); }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) || !base.port ||
    RESERVED_PORTS.has(port) || (port >= 3200 && port <= 3299) ||
    base.username || base.password || base.origin !== value.privateBaseUrl
  ) return fail('NATIVE_MAINTENANCE_LANE36_CONFIG_INVALID');
  const holdings: Holding[] = [];
  const hosts = new Set<string>();
  const ids = new Set<string>();
  for (const item of value.holdings) {
    if (
      !record(item) || !text(item.origin, 512) || !hostId(item.host) ||
      !text(item.holdingSlug, 128) || !scopeId(item.holdingId) ||
      !/^[a-z0-9_-]+$/.test(String(item.holdingSlug)) ||
      (item.companyId !== undefined && !scopeId(item.companyId))
    ) return fail('NATIVE_MAINTENANCE_LANE36_CONFIG_INVALID');
    let origin: URL;
    try { origin = new URL(String(item.origin)); } catch { return fail('NATIVE_MAINTENANCE_LANE36_CONFIG_INVALID'); }
    if (
      origin.protocol !== 'https:' || origin.origin !== item.origin ||
      origin.username || origin.password || origin.host !== String(item.host) ||
      hosts.has(String(item.host)) || ids.has(String(item.holdingId))
    ) return fail('NATIVE_MAINTENANCE_LANE36_CONFIG_INVALID');
    hosts.add(String(item.host));
    ids.add(String(item.holdingId));
    holdings.push({
      origin: origin.origin,
      host: String(item.host),
      holdingSlug: String(item.holdingSlug),
      holdingId: String(item.holdingId),
      ...(item.companyId === undefined ? {} : { companyId: String(item.companyId) }),
    });
  }
  return { privateBaseUrl: base.origin, timeoutMs: value.timeoutMs as number, holdings };
}

export function nativeMaintenanceLane36Configured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[ENV] !== undefined;
}

/** Resolve exactly one registered Holding. No default: an actor must name a
 * registered (holdingId, holdingSlug) and a legacy caller must present an
 * exact registered Host. A configured companyId additionally pins the actor. */
async function targetFor(config: Config, actor?: DescriptorActor): Promise<Holding> {
  const incoming = await headers().catch(() => null);
  const host = incoming?.get('host') ?? null;
  if (actor) {
    const target = config.holdings.find(
      holding =>
        holding.holdingId === actor.holdingId &&
        holding.holdingSlug === actor.holdingSlug &&
        (holding.companyId === undefined || holding.companyId === actor.companyId),
    );
    if (!target || (host !== null && host !== target.host))
      return fail('WORK_CONTEXT_CHANGED', 409);
    return target;
  }
  const target = host === null ? undefined : config.holdings.find(holding => holding.host === host);
  if (!target) return fail('WORK_CONTEXT_CHANGED', 409);
  return target;
}

/** The sole descriptor read. Absent configuration never probes the service. */
export async function nativeAttachmentDescriptor(
  targetType: string,
  targetId: string,
  companyId: string,
  actor?: DescriptorActor,
): Promise<Descriptor | null | undefined> {
  const raw = process.env[ENV];
  if (raw === undefined) return undefined;
  if (targetType !== TARGET_TYPE || !scopeId(targetId) || !scopeId(companyId))
    return fail('MAINTENANCE_LANE36_QUERY_INVALID', 422);
  const config = settings(raw);
  const target = await targetFor(config, actor);
  if (target.companyId !== undefined && target.companyId !== companyId)
    return fail('WORK_CONTEXT_CHANGED', 409);
  const h = new Headers({
    host: target.host,
    accept: 'application/json',
    'cache-control': 'no-cache',
    'x-erpx-context-company': companyId,
  });
  const incoming = await headers().catch(() => null);
  const cookie = incoming?.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192)
      return fail('NATIVE_MAINTENANCE_LANE36_COOKIE_LIMIT');
    h.set('cookie', cookie);
  }
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/maintenance/lane-36/attachment-target`,
    config.privateBaseUrl,
  );
  url.searchParams.set('id', targetId);
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), config.timeoutMs);
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'GET', headers: h, signal: controller.signal, redirect: 'error', cache: 'no-store',
    });
  } catch {
    return fail();
  } finally {
    clearTimeout(timer);
  }
  if (response.status === 403 || response.status === 404) {
    void response.body?.cancel().catch(() => undefined);
    return fail('MAINTENANCE_LANE36_NOT_FOUND', response.status, 'Attachment target is not available in this scope.');
  }
  if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return fail();
  }
  let parsed: unknown;
  try { parsed = await response.json(); } catch { return fail('NATIVE_MAINTENANCE_LANE36_RESPONSE_INVALID', 502); }
  if (!record(parsed)) return fail('NATIVE_MAINTENANCE_LANE36_RESPONSE_INVALID', 502);
  if (record(parsed.error)) {
    const code = parsed.error.code;
    const message = parsed.error.message;
    return fail(
      text(code, 100) ? String(code) : undefined,
      response.status >= 400 && response.status < 500 ? response.status : 503,
      text(message, 300) ? String(message) : undefined,
    );
  }
  if (!response.ok) return fail();
  if (
    parsed.targetId !== targetId || parsed.companyId !== companyId ||
    (parsed.data !== null && !descriptor(parsed.data)) ||
    (record(parsed.data) && (parsed.data as Row).company_id !== companyId)
  ) return fail('NATIVE_MAINTENANCE_LANE36_RESPONSE_INVALID', 502);
  return parsed.data as Descriptor | null;
}

/** Fail-closed wrapper for consumers that must have a configured native read. */
export async function requireAttachmentDescriptor(
  targetType: string,
  targetId: string,
  companyId: string,
  actor?: DescriptorActor,
): Promise<Descriptor | null> {
  const row = await nativeAttachmentDescriptor(targetType, targetId, companyId, actor);
  if (row === undefined) return fail('NATIVE_MAINTENANCE_LANE36_CONFIG_REQUIRED');
  return row;
}
