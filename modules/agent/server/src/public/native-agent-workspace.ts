import 'server-only';
import { headers as requestHeaders } from 'next/headers';
import { nativeRequest } from '@erpx/platform-runtime-server/public/native-http';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';

/**
 * Fail-closed server-to-server client for the mounted native Agent workspace
 * read. The four retained consumers have no browser Request, so the Holding
 * authority comes from an exact registered loopback configuration. Missing or
 * invalid configuration, a malformed reply or any non-200 response fails
 * closed; there is no TypeScript SQL fallback.
 */
export type NativeAgentWorkspaceConfig = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: readonly Readonly<{ origin: string; holdingSlug: string; holdingId: string }>[];
}>;

export type NativeAgentWorkspaceTransport = (
  url: URL,
  headers: Headers,
  body: string,
  signal: AbortSignal,
) => Promise<Response>;

export type NativeAgentWorkspace = {
  agents: Record<string, unknown>[];
  versions: Record<string, unknown>[];
  pools: Record<string, unknown>[];
  assignments: Record<string, unknown>[];
  invocations: Record<string, unknown>[];
  people: Record<string, unknown>[];
  members: Record<string, unknown>[];
  poolHistory: Record<string, unknown>[];
  sessions: Record<string, unknown>[];
};

export type NativeAgentRecord = {
  row: Record<string, unknown>;
  sessionData: NativeAgentWorkspace | null;
};

export type NativeAgentRecordKind = 'agent_invocation' | 'agent_session';

export class NativeAgentWorkspaceError extends Error {
  readonly status?: number;
  constructor(readonly code: string, status?: number, message?: string) {
    super(message ?? 'The native Agent workspace read could not be completed.');
    this.name = 'NativeAgentWorkspaceError';
    this.status = status;
  }
}

const fail = (code: string, status = 503): never => {
  throw new NativeAgentWorkspaceError(code, status);
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const rows = (value: unknown): value is Record<string, unknown>[] =>
  Array.isArray(value) && value.every(record);
const identifier = (value: unknown, max = 200): value is string =>
  typeof value === 'string' && value.length > 0 && value.length <= max && !/[\u0000-\u001f\u007f]/.test(value);
const reserved = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

type Parsed = {
  base: URL;
  timeoutMs: number;
  holdings: Map<string, { origin: string; holdingSlug: string; holdingId: string }>;
};

function configuration(raw: string): Parsed {
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return fail('NATIVE_AGENT_WORKSPACE_CONFIG_INVALID');
  }
  if (!record(value) || typeof value.privateBaseUrl !== 'string' || !Number.isSafeInteger(value.timeoutMs) || !Array.isArray(value.holdings))
    return fail('NATIVE_AGENT_WORKSPACE_CONFIG_INVALID');
  const timeoutMs = Number(value.timeoutMs);
  if (timeoutMs < 250 || timeoutMs > 30_000) return fail('NATIVE_AGENT_WORKSPACE_CONFIG_INVALID');
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return fail('NATIVE_AGENT_WORKSPACE_CONFIG_INVALID');
  }
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    reserved.has(port) ||
    (port >= 3200 && port <= 3299) ||
    base.username ||
    base.password ||
    value.privateBaseUrl !== base.origin
  )
    return fail('NATIVE_AGENT_WORKSPACE_CONFIG_INVALID');
  const holdings = new Map<string, { origin: string; holdingSlug: string; holdingId: string }>();
  for (const item of value.holdings) {
    if (!record(item) || !identifier(item.origin, 200) || !identifier(item.holdingId) || !identifier(item.holdingSlug, 128))
      return fail('NATIVE_AGENT_WORKSPACE_CONFIG_INVALID');
    let origin: URL;
    try {
      origin = new URL(item.origin);
    } catch {
      return fail('NATIVE_AGENT_WORKSPACE_CONFIG_INVALID');
    }
    if (origin.protocol !== 'https:' || item.origin !== origin.origin || origin.port || origin.username || origin.password || holdings.has(origin.host))
      return fail('NATIVE_AGENT_WORKSPACE_CONFIG_INVALID');
    holdings.set(origin.host, { origin: item.origin, holdingId: item.holdingId, holdingSlug: item.holdingSlug });
  }
  if (holdings.size < 1 || holdings.size > 64) return fail('NATIVE_AGENT_WORKSPACE_CONFIG_INVALID');
  return { base, timeoutMs, holdings };
}

function workspace(value: unknown): asserts value is NativeAgentWorkspace {
  if (!record(value)) return fail('NATIVE_AGENT_WORKSPACE_RESPONSE_INVALID');
  const keys = ['agents', 'versions', 'pools', 'assignments', 'invocations', 'people', 'members', 'poolHistory', 'sessions'] as const;
  if (Object.keys(value).length !== keys.length || !keys.every((key) => rows(value[key])))
    return fail('NATIVE_AGENT_WORKSPACE_RESPONSE_INVALID');
}

function recordPayload(value: unknown): asserts value is NativeAgentRecord {
  if (!record(value) || Object.keys(value).length !== 2 || !record(value.row) || (value.sessionData !== null && !record(value.sessionData)))
    return fail('NATIVE_AGENT_WORKSPACE_RESPONSE_INVALID');
  if (value.sessionData !== null) workspace(value.sessionData);
}

const direct: NativeAgentWorkspaceTransport = (url, headers, body, signal) =>
  nativeRequest(url, { method: 'POST', headers: Object.fromEntries(headers), body, signal }, 30_000);

export function createNativeAgentWorkspaceClient(
  raw: string,
  transport: NativeAgentWorkspaceTransport = direct,
) {
  const config = configuration(raw);
  const call = async (
    actor: WorkContext,
    body: Record<string, unknown>,
    incoming?: Pick<Headers, 'get'>,
  ): Promise<unknown> => {
    const selected = [...config.holdings.values()].find((item) => item.holdingId === actor.holdingId && item.holdingSlug === actor.holdingSlug);
    if (!selected) return fail('NATIVE_AGENT_WORKSPACE_AUTHORITY_INVALID');
    if (!identifier(actor.companyId) || !identifier(actor.userId) || !identifier(actor.assignmentId) || !actor.authority)
      return fail('NATIVE_AGENT_WORKSPACE_REQUEST_INVALID', 422);
    const headers = new Headers({
      host: new URL(selected.origin).host,
      origin: selected.origin,
      accept: 'application/json',
      'content-type': 'application/json',
      'cache-control': 'no-cache',
      'x-erpx-context-company': actor.companyId,
      'x-erpx-context-assignment': actor.assignmentId,
      'x-erpx-context-revision': String(actor.authority.revision),
      'x-erpx-context-assignment-version': String(actor.authority.assignmentVersion),
    });
    if (incoming) {
      if (incoming.get('host') !== headers.get('host')) return fail('NATIVE_AGENT_WORKSPACE_AUTHORITY_INVALID', 403);
      const cookie = incoming.get('cookie');
      if (!cookie) return fail('NATIVE_AGENT_WORKSPACE_SESSION_REQUIRED', 401);
      if (new TextEncoder().encode(cookie).length > 8192) return fail('NATIVE_AGENT_WORKSPACE_COOKIE_LIMIT', 422);
      headers.set('cookie', cookie);
    }
    for (const [name, value] of [
      ['x-erpx-workspace-branch', actor.scope.kind === 'branch' || actor.scope.kind === 'department' ? actor.scope.branchSlug : null],
      ['x-erpx-workspace-department', actor.scope.kind === 'department' ? actor.scope.departmentSlug : null],
    ] as const) {
      if (value !== null) {
        if (!identifier(value, 128)) return fail('NATIVE_AGENT_WORKSPACE_REQUEST_INVALID', 422);
        headers.set(name, value);
      }
    }
    const url = new URL(
      `/api/v1/holdings/${encodeURIComponent(selected.holdingSlug)}/agent/workspace-read`,
      config.base,
    );
    const text = JSON.stringify(body);
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), config.timeoutMs);
    try {
      const response = await transport(url, headers, text, controller.signal);
      if (response.redirected || response.status !== 200 || !response.body) return fail('NATIVE_AGENT_WORKSPACE_UNAVAILABLE', response.status);
      if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json')
        return fail('NATIVE_AGENT_WORKSPACE_RESPONSE_INVALID');
      const bytes = await response.arrayBuffer();
      if (bytes.byteLength > 1024 * 1024) return fail('NATIVE_AGENT_WORKSPACE_RESPONSE_LIMIT');
      const payload: unknown = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
      if (!record(payload) || Object.keys(payload).length !== 1 || !record(payload.data))
        return fail('NATIVE_AGENT_WORKSPACE_RESPONSE_INVALID');
      return payload.data;
    } catch (error) {
      if (error instanceof NativeAgentWorkspaceError) throw error;
      return fail('NATIVE_AGENT_WORKSPACE_UNAVAILABLE');
    } finally {
      clearTimeout(timer);
    }
  };
  return {
    workspace: async (actor: WorkContext, includeEvidence = true, incoming?: Pick<Headers, 'get'>): Promise<NativeAgentWorkspace> => {
      const data = await call(actor, { kind: 'workspace', includeEvidence }, incoming);
      workspace(data);
      return data;
    },
    record: async (actor: WorkContext, kind: NativeAgentRecordKind, id: string, incoming?: Pick<Headers, 'get'>): Promise<NativeAgentRecord> => {
      if (kind !== 'agent_invocation' && kind !== 'agent_session') return fail('NATIVE_AGENT_WORKSPACE_REQUEST_INVALID', 422);
      if (!identifier(id)) return fail('NATIVE_AGENT_WORKSPACE_REQUEST_INVALID', 422);
      const data = await call(actor, { kind: 'record', recordKind: kind, id }, incoming);
      recordPayload(data);
      return data;
    },
  };
}

function client() {
  const raw = process.env.ERPX_NATIVE_AGENT_WORKSPACE_CONFIG;
  return raw === undefined ? undefined : createNativeAgentWorkspaceClient(raw);
}

export function nativeAgentWorkspaceAvailable(): boolean {
  return process.env.ERPX_NATIVE_AGENT_WORKSPACE_CONFIG !== undefined;
}

/** Retained `agentWorkspace` export, now served by the native owner. */
export async function nativeAgentWorkspaceFromActor(
  actor: WorkContext,
  includeEvidence = true,
): Promise<NativeAgentWorkspace> {
  const instance = client();
  if (!instance) throw new ApiError(503, 'NATIVE_AGENT_UNAVAILABLE', 'The native Agent workspace is unavailable.');
  try {
    return await instance.workspace(actor, includeEvidence, await requestHeaders());
  } catch (error) {
    throw toApiError(error);
  }
}

/** Retained `agentEvidenceRecord` export, now served by the native owner. */
export async function nativeAgentEvidenceRecordFromActor(
  actor: WorkContext,
  kind: NativeAgentRecordKind,
  id: string,
): Promise<NativeAgentRecord> {
  const instance = client();
  if (!instance) throw new ApiError(503, 'NATIVE_AGENT_UNAVAILABLE', 'The native Agent workspace is unavailable.');
  try {
    return await instance.record(actor, kind, id, await requestHeaders());
  } catch (error) {
    throw toApiError(error);
  }
}

function toApiError(error: unknown): ApiError {
  if (error instanceof NativeAgentWorkspaceError) {
    const status = error.status ?? 503;
    return new ApiError(status, error.code, 'The native Agent workspace read is unavailable.');
  }
  return new ApiError(503, 'NATIVE_AGENT_UNAVAILABLE', 'The native Agent workspace read is unavailable.');
}

