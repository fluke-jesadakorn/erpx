import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';
import { nativeRequest } from '@erpx/server-runtime/public/native-http';
import {
  nativeHoldingConfiguration,
  nativeHoldingTargetById,
  nativeHoldingTargetForHost,
  type NativeHoldingTarget,
} from '@erpx/native-sdk/holding-target';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { originProfile } from '@erpx/feature-runtime/lib/holding-hosts';
import { randomUUID } from 'node:crypto';

/**
 * Fail-closed transport for the mounted native Work navigation read and
 * mark-home command. The owner keeps the snapshot SQL, counts and receipt; this
 * bridge forwards only the bounded context and never reads the Holding database.
 */
const CONFIG_ENV = 'ERPX_NATIVE_WORK_PROJECTION_CONFIG';
const unavailable = (): never => {
  throw new ApiError(503, 'NATIVE_WORK_NAVIGATION_UNAVAILABLE', 'The native Work navigation is unavailable.');
};
const contract = (): never => {
  throw new ApiError(502, 'NATIVE_WORK_NAVIGATION_RESPONSE_INVALID', 'The native Work navigation response did not match its contract.');
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

function configuration() {
  const raw = process.env[CONFIG_ENV];
  if (raw === undefined) return unavailable();
  try {
    return nativeHoldingConfiguration(raw, originProfile());
  } catch {
    return unavailable();
  }
}

function target(actor: WorkContext, incoming: Pick<Headers, 'get'>): NativeHoldingTarget {
  const config = configuration();
  const sourceHost = incoming.get('host');
  const loopback =
    originProfile() === 'loopback' && sourceHost !== null && /^127\.0\.0\.1:\d+$/.test(sourceHost);
  const selected = loopback
    ? (config.holdings.find((holding) => holding.holdingSlug === actor.holdingSlug) ??
      nativeHoldingTargetById(config, actor.holdingId))
    : (nativeHoldingTargetForHost(config, sourceHost) ??
      nativeHoldingTargetById(config, actor.holdingId));
  if (!selected || selected.holdingId !== actor.holdingId || selected.holdingSlug !== actor.holdingSlug)
    return unavailable();
  return selected;
}

function headers(incoming: Pick<Headers, 'get'>, selected: NativeHoldingTarget, actor: WorkContext) {
  const values: Record<string, string> = {
    host: new URL(selected.origin).host,
    origin: selected.origin,
    accept: 'application/json',
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.authority.revision),
    'x-erpx-context-assignment-version': String(actor.authority.assignmentVersion),
  };
  const branch = actor.scope.kind === 'holding' ? null : actor.scope.branchSlug;
  if (branch) values['x-erpx-workspace-branch'] = branch;
  if (actor.scope.kind === 'department' && actor.scope.departmentSlug)
    values['x-erpx-workspace-department'] = actor.scope.departmentSlug;
  const cookie = incoming.get('cookie');
  if (cookie) {
    if (new TextEncoder().encode(cookie).length > 8192) return unavailable();
    values.cookie = cookie;
  }
  return values;
}

export async function nativeNavigationSnapshot(
  incoming: Pick<Headers, 'get'>,
  actor: WorkContext,
  includeAttention: boolean,
): Promise<Record<string, unknown>> {
  const config = configuration();
  const selected = target(actor, incoming);
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(selected.holdingSlug)}/navigation${includeAttention ? '?include=attention' : ''}`,
    config.privateBaseUrl,
  );
  let response: Response;
  try {
    response = await nativeRequest(url, { method: 'GET', headers: headers(incoming, selected, actor) }, config.timeoutMs);
  } catch {
    return unavailable();
  }
  const payload: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const error = record(payload) && record(payload.error) ? payload.error : undefined;
    throw new ApiError(
      response.status,
      typeof error?.code === 'string' ? error.code : 'NATIVE_WORK_NAVIGATION_FAILED',
      typeof error?.message === 'string' ? error.message : 'The native Work navigation read was rejected.',
    );
  }
  if (!record(payload) || !record(payload.data)) return contract();
  return payload.data;
}

/** Mark the home stream read and return the remaining unread count. */
export async function nativeMarkHomeRead(
  incoming: Pick<Headers, 'get'>,
  actor: WorkContext,
): Promise<number> {
  const config = configuration();
  const selected = target(actor, incoming);
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(selected.holdingSlug)}/navigation`,
    config.privateBaseUrl,
  );
  const values = headers(incoming, selected, actor);
  values['content-type'] = 'application/json';
  values['idempotency-key'] = `nav-read-home:${randomUUID()}`;
  let response: Response;
  try {
    response = await nativeRequest(
      url,
      { method: 'POST', headers: values, body: JSON.stringify({ action: 'read-home' }) },
      config.timeoutMs,
    );
  } catch {
    return unavailable();
  }
  const payload: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const error = record(payload) && record(payload.error) ? payload.error : undefined;
    throw new ApiError(
      response.status,
      typeof error?.code === 'string' ? error.code : 'NATIVE_WORK_NAVIGATION_FAILED',
      typeof error?.message === 'string' ? error.message : 'The native Work navigation command was rejected.',
    );
  }
  if (!record(payload) || !record(payload.data) || typeof payload.data.home !== 'number')
    return contract();
  return payload.data.home;
}
