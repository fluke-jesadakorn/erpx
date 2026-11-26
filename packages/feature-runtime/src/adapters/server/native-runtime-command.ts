import 'server-only';
import { createHash } from 'node:crypto';
import { ApiError } from '@erpx/server-runtime/public/http';
import { nativeRequest } from '@erpx/server-runtime/public/native-http';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { SqlResult, SqlStatement } from '@erpx/server-runtime/public/runtime/types';

/**
 * Strict server-side client for the native runtime axum command route. This is
 * the only write transport for the migrated statement kinds: it carries no SQL
 * and offers no TypeScript fallback. The route executes the owner Postgres
 * statement inside the caller's unit with the receipt/audit/outbox ledger. A
 * missing transport or invalid reply fails closed.
 */
type Holding = Readonly<{ origin: string; holdingSlug: string; holdingId: string; companyId: string }>;
type Settings = Readonly<{ privateBaseUrl: string; timeoutMs: number; holdings: readonly Holding[] }>;
type Reply = Readonly<{ data?: { kind?: unknown; rows?: unknown }; error?: { code?: string; message?: string } }>;
export type NativeRuntimeResult = Readonly<{ rows: Record<string, unknown>[]; rowCount: number; kind: string }>;

const unavailable = (): never => { throw new ApiError(503, 'RUNTIME_NATIVE_COMMAND_UNAVAILABLE', 'The native runtime service is unavailable.'); };
const record = (v: unknown): v is Record<string, unknown> => v !== null && typeof v === 'object' && !Array.isArray(v);
const identifier = (v: unknown): v is string => typeof v === 'string' && v.length > 0 && new TextEncoder().encode(v).length <= 200;
const reservedPorts = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function configuration(): Settings {
  const raw = process.env.ERPX_NATIVE_RUNTIME_CONFIG;
  if (!raw) return unavailable();
  let value: unknown;
  try { value = JSON.parse(raw); } catch { return unavailable(); }
  if (!record(value) || typeof value.privateBaseUrl !== 'string' || !Array.isArray(value.holdings)) return unavailable();
  const timeoutMs = value.timeoutMs;
  if (typeof timeoutMs !== 'number' || !Number.isSafeInteger(timeoutMs) || timeoutMs < 250 || timeoutMs > 30000) return unavailable();
  let base: URL;
  try { base = new URL(value.privateBaseUrl); } catch { return unavailable(); }
  const port = Number(base.port);
  if (base.protocol !== 'http:' || !['127.0.0.1', '[::1]'].includes(base.hostname) || !base.port || reservedPorts.has(port) || (port >= 3200 && port <= 3299) || base.username || base.password || value.privateBaseUrl !== base.origin) return unavailable();
  const holdings = value.holdings.map((item: unknown) => {
    if (!record(item) || typeof item.origin !== 'string' || typeof item.holdingSlug !== 'string' || typeof item.holdingId !== 'string' || typeof item.companyId !== 'string') return unavailable();
    let origin: URL;
    try { origin = new URL(item.origin); } catch { return unavailable(); }
    if (origin.protocol !== 'https:' || item.origin !== origin.origin || origin.username || origin.password || !identifier(item.companyId) || !/^[a-z0-9_-]+$/.test(item.holdingSlug)) return unavailable();
    return { origin: item.origin, holdingSlug: item.holdingSlug, holdingId: item.holdingId, companyId: item.companyId };
  });
  if (holdings.length < 1 || holdings.length > 64) return unavailable();
  return { privateBaseUrl: value.privateBaseUrl, holdings, timeoutMs };
}

function reply(value: unknown): NativeRuntimeResult {
  if (!record(value)) return unavailable();
  const body = value as Reply;
  if (!record(body.data) || typeof body.data.kind !== 'string' || !Number.isSafeInteger(body.data.rows) || Number(body.data.rows) < 0) return unavailable();
  return { rows: [], rowCount: Number(body.data.rows), kind: body.data.kind };
}

function keyFor(kind: string, values: readonly unknown[]): string {
  return 'RUNTIME_NATIVE:native:' + createHash('sha256').update(kind).update('\u0000').update(JSON.stringify(values)).digest('hex');
}

function target(config: Settings, holding: Holding, companyId: string): Record<string, string> {
  return {
    host: new URL(holding.origin).host,
    origin: holding.origin,
    'content-type': 'application/json',
    accept: 'application/json',
    'cache-control': 'no-cache',
    'idempotency-key': '',
    'x-erpx-context-company': companyId,
    'x-erpx-context-holding': holding.holdingId,
  };
}

async function post(url: URL, headers: Record<string, string>, body: string, timeoutMs: number): Promise<NativeRuntimeResult> {
  if (new TextEncoder().encode(body).length > 256 * 1024) return unavailable();
  let response: Response;
  try { response = await nativeRequest(url, { method: 'POST', headers, body }, timeoutMs); }
  catch { return unavailable(); }
  if (response.status !== 200 || response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    if (response.status >= 400 && response.status < 500) {
      const payload = await response.json().catch((): unknown => null);
      const err = record(payload) ? (payload as Reply).error : undefined;
      throw new ApiError(response.status, err?.code ?? 'RUNTIME_NATIVE_COMMAND_FAILED', err?.message ?? 'The native runtime command was rejected.');
    }
    return unavailable();
  }
  return reply(await response.json().catch((): unknown => null));
}

export function nativeRuntimeStatementAvailable(): boolean { return process.env.ERPX_NATIVE_RUNTIME_CONFIG !== undefined; }

/** Execute one command through the private axum route in a browser WorkContext. */
export async function RUNTIME_NATIVECommand(
  actor: WorkContext, kind: string, values: unknown[], key = keyFor(kind, values), now = Date.now(),
): Promise<NativeRuntimeResult> {
  const config = configuration();
  const holding = config.holdings.find(h => h.holdingSlug === actor.holdingSlug && h.holdingId === actor.holdingId);
  if (!holding || !identifier(kind) || !/^[A-Za-z0-9:_.-]{8,180}$/.test(key)) return unavailable();
  const headers = target(config, holding, actor.companyId);
  headers['idempotency-key'] = key;
  headers['x-erpx-context-assignment'] = actor.assignmentId;
  if (actor.authority) {
    headers['x-erpx-context-user'] = actor.userId;
    headers['x-erpx-context-revision'] = String(actor.authority.revision);
    headers['x-erpx-context-assignment-version'] = String(actor.authority.assignmentVersion);
  }
  if (actor.branchId) headers['x-erpx-workspace-branch'] = actor.branchId;
  if (actor.departmentId) headers['x-erpx-workspace-department'] = actor.departmentId;
  const url = new URL(`/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/runtime/native-command`, config.privateBaseUrl);
  return post(url, headers, JSON.stringify({ kind, values, now }), config.timeoutMs);
}

/** Company-keyed system variant for workers without a browser WorkContext. */
export async function RUNTIME_NATIVECommandSystem(
  companyId: string, kind: string, values: unknown[], key = keyFor(kind, values), now = Date.now(),
): Promise<NativeRuntimeResult> {
  const config = configuration();
  if (!identifier(kind) || !/^[A-Za-z0-9:_.-]{8,180}$/.test(key)) return unavailable();
  const resolved =
    identifier(companyId)
      ? companyId
      : config.holdings.find(item => values.some(value => value === item.companyId))?.companyId ??
        (config.holdings.length === 1 ? config.holdings[0]!.companyId : undefined);
  const holding = config.holdings.find(h => h.companyId === resolved);
  if (!holding || !resolved) return unavailable();
  const headers = target(config, holding, resolved);
  headers['idempotency-key'] = key;
  const url = new URL(`/api/v1/companies/${encodeURIComponent(resolved)}/`+"runtime/native-command", config.privateBaseUrl);
  return post(url, headers, JSON.stringify({ kind, values, now }), config.timeoutMs);
}

/** Eager system variant for raw-client/bootstrap call sites that previously
 * issued SQL outside a D1 statement. Executes through the private axum route. */
export async function nativeRuntimeExecuteSystem(
  companyId: string, kind: string, values: unknown[], key: string, now = Date.now(),
): Promise<number> {
  const result = await RUNTIME_NATIVECommandSystem(companyId, kind, values, key, now);
  return result.rowCount;
}

type ShmContext = Readonly<{ companyId?: string; actor?: WorkContext; key?: string; now?: number }>;

/** D1-compatible statement shim whose terminal methods route to the axum
 * command route. It never prepares or executes SQL in TypeScript. */
export function nativeRuntimeStatement(kind: string, context: ShmContext = {}): SqlStatement {
  const bound: unknown[] = [];
  const run = async (): Promise<SqlResult<Record<string, unknown>>> => {
    const values = bound.slice();
    const key = context.key ?? keyFor(kind, values);
    const result = context.actor
      ? await RUNTIME_NATIVECommand(context.actor, kind, values, key, context.now ?? Date.now())
      : await RUNTIME_NATIVECommandSystem(context.companyId ?? '', kind, values, key, context.now ?? Date.now());
    return { results: result.rows as Record<string, unknown>[], success: true, meta: { changes: result.rowCount, duration: 0 } };
  };
  const statement: SqlStatement = {
    bind(...values: unknown[]): SqlStatement { bound.push(...values); return statement; },
    async first<T = Record<string, unknown>>(column?: string): Promise<T | null> {
      const result = await run();
      const row = result.results[0] as Record<string, unknown> | undefined;
      if (!row) return null;
      if (column) return (row[column] ?? null) as T;
      return row as T;
    },
    async all<T = Record<string, unknown>>(): Promise<SqlResult<T>> {
      const result = await run();
      return { results: result.results as T[], success: true, meta: { changes: result.meta.changes, duration: result.meta.duration } };
    },
    async run<T = Record<string, unknown>>(): Promise<SqlResult<T>> {
      const result = await run();
      return { results: result.results as T[], success: true, meta: { changes: result.meta.changes, duration: result.meta.duration } };
    },
    async raw<T = unknown[]>(): Promise<T[]> { return [] as T[]; },
  };
  return statement;
}
