import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';

/**
 * Strict server-side client for the native Manufacturing accounting-profile
 * axum route. It carries no SQL and no TypeScript fallback: the owner command
 * decides the draft/review transition and writes the receipt, audit and outbox
 * in one caller-owned serializable unit. A missing transport or invalid reply
 * fails closed.
 */
type Settings = Readonly<{
  base: string;
  host: string;
  holdingSlug: string;
  origin: string;
  timeoutMs: number;
}>;

const CONFIG_KEY = 'ERPX_NATIVE_MANUFACTURING_ACCOUNTING_CONFIG';
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function fail(code: string, status: number, message: string): never {
  throw new ApiError(status, code, message);
}

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

function config(): Settings {
  const raw = process.env[CONFIG_KEY];
  if (raw === undefined)
    fail('NATIVE_MANUFACTURING_ACCOUNTING_UNAVAILABLE', 503, 'The native Manufacturing accounting service is not configured.');
  let value: Record<string, unknown>;
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!record(parsed)) fail('NATIVE_MANUFACTURING_ACCOUNTING_CONFIG_INVALID', 503, 'The native Manufacturing accounting transport is misconfigured.');
    value = parsed;
  } catch {
    fail('NATIVE_MANUFACTURING_ACCOUNTING_CONFIG_INVALID', 503, 'The native Manufacturing accounting transport is misconfigured.');
  }
  let base: URL, origin: URL;
  try {
    base = new URL(String(value.privateBaseUrl));
    origin = new URL(String(value.origin));
  } catch {
    fail('NATIVE_MANUFACTURING_ACCOUNTING_CONFIG_INVALID', 503, 'The native Manufacturing accounting transport is misconfigured.');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    RESERVED_PORTS.has(port) ||
    (port >= 3200 && port <= 3299) ||
    origin.protocol !== 'https:' ||
    typeof value.host !== 'string' ||
    value.host !== origin.host ||
    typeof value.holdingSlug !== 'string' ||
    !/^[a-z0-9_-]{1,128}$/.test(value.holdingSlug) ||
    !Number.isInteger(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 ||
    (value.timeoutMs as number) > 30000
  )
    fail('NATIVE_MANUFACTURING_ACCOUNTING_CONFIG_INVALID', 503, 'The native Manufacturing accounting transport is misconfigured.');
  return {
    base: base.origin,
    host: value.host,
    holdingSlug: value.holdingSlug,
    origin: origin.origin,
    timeoutMs: value.timeoutMs as number,
  };
}

function headers(actor: WorkContext, key: string, cfg: Settings): Headers {
  const permissions = actor.iam?.permissions ?? [];
  if (!permissions.length || !actor.modules.length)
    fail('SIGN_IN_REQUIRED', 401, 'Sign in to manage the production accounting profile.');
  return new Headers({
    host: cfg.host,
    origin: cfg.origin,
    accept: 'application/json',
    'content-type': 'application/json',
    'idempotency-key': key,
    'x-erpx-context-user': actor.userId,
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-permissions': permissions.join(','),
    'x-erpx-context-modules': actor.modules.join(','),
  });
}

/** Create or review one profile through the native command; returns its data. */
export async function nativeManufacturingProfile(
  kind: 'create' | 'review',
  profileId: string | null,
  body: unknown,
  key: string,
  actor: WorkContext,
): Promise<Record<string, unknown>> {
  const cfg = config();
  const suffix = kind === 'create' ? '' : `/${encodeURIComponent(profileId ?? '')}`;
  let response: Response;
  try {
    response = await fetch(
      `${cfg.base}/api/v1/holdings/${encodeURIComponent(cfg.holdingSlug)}/manufacturing/accounting-profile${suffix}`,
      {
        method: 'POST',
        headers: headers(actor, key, cfg),
        body: JSON.stringify(body),
        cache: 'no-store',
        signal: AbortSignal.timeout(cfg.timeoutMs),
      },
    );
  } catch {
    fail('NATIVE_MANUFACTURING_ACCOUNTING_UNAVAILABLE', 503, 'The native Manufacturing accounting service is unavailable.');
  }
  const value: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const error = record(value) && record(value.error) ? value.error : {};
    fail(
      typeof error.code === 'string' ? error.code : 'NATIVE_MANUFACTURING_ACCOUNTING_FAILED',
      response.status,
      typeof error.message === 'string' ? error.message : 'The Manufacturing accounting command could not be completed.',
    );
  }
  if (!record(value) || !record(value.data))
    fail('NATIVE_MANUFACTURING_ACCOUNTING_RESPONSE_INVALID', 503, 'The native Manufacturing accounting reply was invalid.');
  return value.data;
}
