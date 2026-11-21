import 'server-only';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';

/**
 * Fail-closed native transport for the wave-3 Manufacturing accounting-profile
 * writer. The axum route is the only place that executes these writes; when it
 * is not configured the caller must surface an unavailable error rather than
 * fall back to a TypeScript SQL writer.
 */
export type NativeManufacturingWave3Config = Readonly<{
  baseUrl: string;
  host: string;
  origin: string;
  timeoutMs: number;
}>;

export type NativeProfileReceipt = Readonly<{ id: string; status: string; version: number }>;

export class NativeManufacturingWave3Error extends Error {
  readonly code: string;
  readonly status: number;
  constructor(code: string, status = 503, message = 'The native Manufacturing command is unavailable.') {
    super(message);
    this.name = 'NativeManufacturingWave3Error';
    this.code = code;
    this.status = status;
  }
}

const fail = (code: string, status = 503, message?: string): never => {
  throw new NativeManufacturingWave3Error(code, status, message);
};

const KEY = /^[a-zA-Z0-9:_.-]{8,180}$/;
const RESERVED = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

export function nativeManufacturingWave3Available(): boolean {
  return process.env.ERPX_NATIVE_MANUFACTURING_WAVE3_CONFIG !== undefined;
}

export function nativeManufacturingWave3Configuration(raw: string): NativeManufacturingWave3Config {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return fail('NATIVE_MANUFACTURING_WAVE3_CONFIG_INVALID');
  }
  if (
    value === null ||
    typeof value !== 'object' ||
    Array.isArray(value) ||
    typeof (value as Record<string, unknown>).baseUrl !== 'string' ||
    typeof (value as Record<string, unknown>).host !== 'string' ||
    typeof (value as Record<string, unknown>).origin !== 'string'
  )
    return fail('NATIVE_MANUFACTURING_WAVE3_CONFIG_INVALID');
  const config = value as NativeManufacturingWave3Config;
  let base: URL;
  let origin: URL;
  try {
    base = new URL(config.baseUrl);
    origin = new URL(config.origin);
  } catch {
    return fail('NATIVE_MANUFACTURING_WAVE3_CONFIG_INVALID');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    RESERVED.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    config.baseUrl !== base.origin ||
    origin.protocol !== 'https:' ||
    config.origin !== origin.origin ||
    origin.username ||
    origin.password ||
    !config.host ||
    config.host.length > 200
  )
    return fail('NATIVE_MANUFACTURING_WAVE3_CONFIG_INVALID');
  const timeoutMs = (config as { timeoutMs?: unknown }).timeoutMs ?? 5_000;
  if (!Number.isSafeInteger(timeoutMs) || (timeoutMs as number) < 250 || (timeoutMs as number) > 30_000)
    return fail('NATIVE_MANUFACTURING_WAVE3_CONFIG_INVALID');
  return { baseUrl: config.baseUrl, host: config.host, origin: config.origin, timeoutMs: timeoutMs as number };
}

function client() {
  const raw = process.env.ERPX_NATIVE_MANUFACTURING_WAVE3_CONFIG;
  if (raw === undefined) return undefined;
  return nativeManufacturingWave3Configuration(raw);
}

function bounded(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function receipt(value: unknown): NativeProfileReceipt {
  if (
    !bounded(value) ||
    typeof value.id !== 'string' ||
    value.id.length < 1 ||
    value.id.length > 200 ||
    typeof value.status !== 'string' ||
    !['draft', 'approved', 'rejected', 'retired'].includes(value.status) ||
    !Number.isSafeInteger(value.version) ||
    (value.version as number) < 1
  )
    return fail('NATIVE_MANUFACTURING_WAVE3_RESPONSE_INVALID');
  return { id: value.id, status: value.status, version: value.version as number };
}

async function boundedJson(response: Response): Promise<unknown> {
  const contentType = response.headers.get('content-type')?.split(';')[0]?.trim();
  if (response.status !== 200 || contentType !== 'application/json' || response.redirected) {
    void response.body?.cancel().catch(() => undefined);
    return fail('NATIVE_MANUFACTURING_WAVE3_UNAVAILABLE', response.status);
  }
  const raw = await response.text();
  if (new TextEncoder().encode(raw).length > 64 * 1024)
    return fail('NATIVE_MANUFACTURING_WAVE3_RESPONSE_LIMIT');
  try {
    return JSON.parse(raw);
  } catch {
    return fail('NATIVE_MANUFACTURING_WAVE3_RESPONSE_INVALID');
  }
}

/**
 * POST one accounting-profile command to the native axum route. `incoming`
 * supplies the browser Origin so the owner can enforce the exact registered
 * target; the caller identity and authority are rendered from `actor`.
 */
export async function nativeAccountingProfileCommand(
  incoming: Pick<Headers, 'get'>,
  actor: WorkContext,
  holdingSlug: string,
  kind: { create: true } | { review: string },
  body: Record<string, unknown>,
  key: string,
): Promise<NativeProfileReceipt | undefined> {
  const config = client();
  if (!config) return undefined;
  if (!actor.authority || !actor.iam || !KEY.test(key))
    return fail('NATIVE_MANUFACTURING_WAVE3_CONTEXT_CHANGED', 409);
  if (!actor.iam.permissions.includes('finance.write') && !actor.iam.permissions.includes('finance.approve'))
    return fail('MANUFACTURING_ACCOUNTING_PERMISSION_REQUIRED', 403);
  if (!actor.modules.includes('manufacturing'))
    return fail('MANUFACTURING_ACCOUNTING_MODULE_FORBIDDEN', 403);
  const origin = incoming.get('origin');
  if (origin && origin !== config.origin) return fail('ORIGIN_FORBIDDEN', 403);

  const path =
    'create' in kind
      ? `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/manufacturing/accounting-profile`
      : `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/manufacturing/accounting-profile/${encodeURIComponent(kind.review)}`;
  const url = new URL(path, config.baseUrl);
  const outgoing = new Headers({
    host: config.host,
    accept: 'application/json',
    'cache-control': 'no-cache',
    'content-type': 'application/json',
    'idempotency-key': key,
    'x-erpx-context-user': actor.userId,
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-permissions': [...actor.iam.permissions].join(','),
    'x-erpx-context-modules': [...actor.modules].join(','),
  });
  if (origin) outgoing.set('origin', origin);
  const payload = JSON.stringify(body);
  if (new TextEncoder().encode(payload).length > 16 * 1024)
    return fail('NATIVE_MANUFACTURING_WAVE3_REQUEST_INVALID', 422);
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), config.timeoutMs);
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'POST',
      headers: outgoing,
      body: payload,
      signal: controller.signal,
      redirect: 'error',
      cache: 'no-store',
    });
  } catch {
    return fail('NATIVE_MANUFACTURING_WAVE3_UNAVAILABLE');
  } finally {
    clearTimeout(timer);
  }
  const value = await boundedJson(response);
  if (!bounded(value) || !bounded(value.data))
    return fail('NATIVE_MANUFACTURING_WAVE3_RESPONSE_INVALID');
  const parsed = receipt(value.data);
  if (parsed.id.length > 200) return fail('NATIVE_MANUFACTURING_WAVE3_RESPONSE_INVALID');
  return parsed;
}
