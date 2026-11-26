import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { SqlDatabase, SqlResult, SqlStatement } from '@erpx/server-runtime/public/runtime/types';

/**
 * Strict server-side client for the native Finance owner-write axum command
 * route. This is the only write path for the retired `native*-sql` bridges: it
 * carries no SQL and offers no TypeScript fallback. The owner crate owns the
 * typed kind allow-list and the canonical SQL; composition resolves the Account
 * session, current authority and serializable owner unit. The caller executes
 * the returned statement in its own unit together with receipt/audit/outbox
 * effects. A missing transport, an unregistered kind or an invalid reply fails
 * closed.
 */

type Holding = Readonly<{ origin: string; holdingSlug: string; holdingId: string }>;
type Settings = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: readonly Holding[];
}>;

const CONFIG_KEY = 'ERPX_NATIVE_FINANCE_OWNER_WRITE_CONFIG';
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);

function fail(status?: number): never {
  throw new ApiError(
    status ?? 503,
    'NATIVE_FINANCE_OWNER_WRITE_UNAVAILABLE',
    'The native Finance owner write is unavailable.',
  );
}

function record(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function identifier(value: unknown): value is string {
  return typeof value === 'string' && value.length > 0 && new TextEncoder().encode(value).length <= 200;
}

function config(): Settings {
  const raw = process.env[CONFIG_KEY];
  if (raw === undefined) return fail();
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return fail();
  }
  if (!record(value) || typeof value.privateBaseUrl !== 'string' || !Array.isArray(value.holdings))
    return fail();
  const timeoutMs = value.timeoutMs;
  if (
    typeof timeoutMs !== 'number' ||
    !Number.isSafeInteger(timeoutMs) ||
    timeoutMs < 250 ||
    timeoutMs > 30000
  )
    return fail();
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return fail();
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
    value.privateBaseUrl !== base.origin
  )
    return fail();
  const holdings = value.holdings.map((item: unknown): Holding => {
    if (
      !record(item) ||
      typeof item.origin !== 'string' ||
      typeof item.holdingSlug !== 'string' ||
      typeof item.holdingId !== 'string'
    )
      return fail();
    let origin: URL;
    try {
      origin = new URL(item.origin);
    } catch {
      return fail();
    }
    if (
      origin.protocol !== 'https:' ||
      item.origin !== origin.origin ||
      origin.username ||
      origin.password ||
      !identifier(item.holdingId) ||
      !/^[a-z0-9_-]+$/.test(item.holdingSlug)
    )
      return fail();
    return { origin: item.origin, holdingSlug: item.holdingSlug, holdingId: item.holdingId };
  });
  if (holdings.length < 1 || holdings.length > 64) return fail();
  return { privateBaseUrl: value.privateBaseUrl, holdings, timeoutMs };
}

function validKey(key: string): boolean {
  return /^[A-Za-z0-9:_.-]{8,180}$/.test(key);
}

/** Deterministic transport key for one (kind, values) owner statement. The
 * business command keeps its own idempotency; this key only distinguishes the
 * owner statement fetch. */
function financeOwnerKey(kind: string, values: readonly unknown[]): string {
  const text = `${kind}:${JSON.stringify(values)}`;
  let hash = 0x811c9dc5;
  for (let i = 0; i < text.length; i += 1) {
    hash ^= text.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193) >>> 0;
  }
  return `fow-${kind}-${hash.toString(16).padStart(8, '0')}`;
}

function validKind(kind: string): boolean {
  return /^[a-z0-9_]{1,96}$/.test(kind);
}

/** Validate the bounded native reply. The axum command has already executed the
 * write in Rust with receipt/audit/outbox in the caller unit. The only SQL this
 * client prepares is the trusted no-op sequencing statement returned by the
 * owner router; no business SQL literal is authored here and no fallback path
 * exists. */
function parseStatement(db: SqlDatabase, payload: unknown): SqlStatement {
  if (!record(payload) || !record(payload.data)) return fail();
  const data = payload.data;
  if (
    data.ok !== true ||
    typeof data.kind !== 'string' ||
    !validKind(data.kind) ||
    typeof data.rows !== 'number' ||
    !Number.isSafeInteger(data.rows) ||
    data.rows < 0 ||
    !record(data.sequencing)
  )
    return fail();
  const sequencing = data.sequencing;
  if (
    typeof sequencing.sql !== 'string' ||
    sequencing.sql.length === 0 ||
    sequencing.sql.length > 64 ||
    /\b(INSERT|UPDATE|DELETE|DROP|ALTER|TRUNCATE)\b/i.test(sequencing.sql) ||
    !Array.isArray(sequencing.values) ||
    sequencing.values.length !== 0
  )
    return fail();
  return db.prepare(sequencing.sql).bind();
}

async function readJson(db: SqlDatabase, response: Response): Promise<SqlStatement> {
  if (response.redirected || response.status !== 200 || !response.body) {
    void response.body?.cancel().catch(() => undefined);
    return fail(response.status);
  }
  if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    void response.body?.cancel().catch(() => undefined);
    return fail(response.status);
  }
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let bytes = 0;
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      bytes += next.value.byteLength;
      if (bytes > 512 * 1024) {
        await reader.cancel();
        return fail();
      }
      chunks.push(next.value);
    }
  } finally {
    reader.releaseLock();
  }
  const data = new Uint8Array(bytes);
  let offset = 0;
  for (const chunk of chunks) {
    data.set(chunk, offset);
    offset += chunk.byteLength;
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data));
  } catch {
    return fail();
  }
  return parseStatement(db, parsed);
}

function post(url: URL, headers: Record<string, string>, body: string, timeoutMs: number) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  return fetch(url, {
    method: 'POST',
    headers,
    body,
    cache: 'no-store',
    redirect: 'error',
    signal: controller.signal,
  }).finally(() => clearTimeout(timer));
}

function bodyFor(kind: string, values: readonly unknown[], now: number): string {
  if (!validKind(kind) || values.length > 8192 || !Number.isSafeInteger(now)) return fail();
  // PostgreSQL numeric binds may arrive as bigint in the owner adapter; JSON
  // has no bigint, so carry them as decimal strings with no precision loss.
  const portable = Array.from(values, value => (typeof value === 'bigint' ? value.toString() : value));
  const body = JSON.stringify({ kind, values: portable, now });
  if (new TextEncoder().encode(body).length > 512 * 1024) return fail();
  return body;
}

/** Ask the native Finance owner for an authoritative write statement in the
 * current browser WorkContext. The caller executes it in its own unit. */
export async function nativeFinanceOwnerWrite(
  db: SqlDatabase,
  actor: WorkContext,
  kind: string,
  values: readonly unknown[],
  key?: string,
  now = Date.now(),
): Promise<SqlStatement> {
  const settings = config();
  // Resolve the registered target by the authoritative holding id; suite and
  // dev fixtures may use a workspace slug (test) that differs from the
  // registered Holding slug (acme).
  const holding = settings.holdings.find(item => item.holdingId === actor.holdingId);
  const idempotencyKey = key ?? financeOwnerKey(kind, values);
  if (!holding || !validKey(idempotencyKey)) return fail();
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/finance/owner-write`,
    settings.privateBaseUrl,
  );
  const headers: Record<string, string> = {
    host: new URL(holding.origin).host,
    origin: holding.origin,
    'content-type': 'application/json',
    'idempotency-key': idempotencyKey,
    'x-erpx-context-company': actor.companyId,
  };
  if (actor.assignmentId) headers['x-erpx-context-assignment'] = actor.assignmentId;
  if (actor.authority) {
    headers['x-erpx-context-revision'] = String(actor.authority.revision);
    headers['x-erpx-context-assignment-version'] = String(actor.authority.assignmentVersion);
  }
  if (actor.branchId) headers['x-erpx-workspace-branch'] = actor.branchId;
  if (actor.departmentId) headers['x-erpx-workspace-department'] = actor.departmentId;
  let response: Response;
  try {
    response = await post(url, headers, bodyFor(kind, values, now), settings.timeoutMs);
  } catch {
    return fail();
  }
  return readJson(db, response);
}

/** Company-keyed system variant for workers without a browser WorkContext. */
export async function nativeFinanceOwnerWriteSystem(
  db: SqlDatabase,
  companyId: string,
  kind: string,
  values: readonly unknown[],
  key?: string,
  now = Date.now(),
): Promise<SqlStatement> {
  const settings = config();
  const idempotencyKey = key ?? financeOwnerKey(kind, values);
  if (!identifier(companyId) || !validKey(idempotencyKey)) return fail();
  const url = new URL(
    `/api/v1/companies/${encodeURIComponent(companyId)}/finance/owner-write`,
    settings.privateBaseUrl,
  );
  const headers: Record<string, string> = {
    'content-type': 'application/json',
    'idempotency-key': idempotencyKey,
    'x-erpx-context-company': companyId,
  };
  let response: Response;
  try {
    response = await post(url, headers, bodyFor(kind, values, now), settings.timeoutMs);
  } catch {
    return fail();
  }
  return readJson(db, response);
}

/**
 * Native attachment-version guard input. The Files attachment service supplies
 * the already-authorized actor; the Finance owner rechecks Company scope and
 * the live target version inside its own serializable unit.
 */
export type NativeAttachmentVersionGuardInput = Readonly<{
  companyId: string;
  targetType: string;
  targetId: string;
  expectedVersion: number;
  userId: string;
  assignmentId: string;
  role: string;
  scopeKind: string;
}>;

type GuardContext = Readonly<{ companyId?: string; key?: string; now?: number }>;

/** SqlStatement-shaped descriptor carrying no SQL. The Files native-effects
 * resolver recognizes `__nativeWrite` and executes `run()` in the command's
 * Company/key context; a missing transport or a stale version fails closed. */
export type NativeAttachmentVersionGuard = SqlStatement & {
  readonly __nativeWrite: true;
  withContext(context: GuardContext): NativeAttachmentVersionGuard;
};

async function dispatchGuard(
  context: GuardContext | undefined,
  input: NativeAttachmentVersionGuardInput,
): Promise<void> {
  const settings = config();
  const company = context?.companyId ?? input.companyId;
  const key = context?.key ?? `finance-guard-${input.targetId}-${input.expectedVersion}`;
  const now = context?.now ?? Date.now();
  if (
    !identifier(company) ||
    !validKey(key) ||
    !validKind(input.targetType) ||
    !identifier(input.targetId) ||
    !Number.isSafeInteger(input.expectedVersion) ||
    input.expectedVersion <= 0 ||
    !identifier(input.userId) ||
    !identifier(input.assignmentId) ||
    input.role.length === 0 ||
    input.role.length > 128 ||
    !identifier(input.scopeKind) ||
    !Number.isSafeInteger(now)
  )
    return fail();
  const url = new URL(
    `/api/v1/companies/${encodeURIComponent(company)}/finance/attachment-version/guard`,
    settings.privateBaseUrl,
  );
  const body = JSON.stringify({
    targetType: input.targetType,
    targetId: input.targetId,
    expectedVersion: input.expectedVersion,
    userId: input.userId,
    assignmentId: input.assignmentId,
    role: input.role,
    scopeKind: input.scopeKind,
    now,
  });
  let response: Response;
  try {
    response = await post(
      url,
      {
        'content-type': 'application/json',
        'idempotency-key': key,
        'x-erpx-context-company': company,
      },
      body,
      settings.timeoutMs,
    );
  } catch {
    return fail();
  }
  if (response.redirected || response.status !== 200 || !response.body) {
    void response.body?.cancel().catch(() => undefined);
    return fail(response.status);
  }
  if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    void response.body.cancel().catch(() => undefined);
    return fail();
  }
  let payload: unknown;
  try {
    payload = await response.json();
  } catch {
    return fail();
  }
  if (
    !record(payload) ||
    !record(payload.data) ||
    payload.data.ok !== true ||
    payload.data.version !== input.expectedVersion ||
    typeof payload.data.changed !== 'boolean' ||
    typeof payload.data.targetType !== 'string' ||
    typeof payload.data.targetId !== 'string'
  )
    return fail();
}

export function nativeFinanceAttachmentVersionGuard(
  input: NativeAttachmentVersionGuardInput,
): NativeAttachmentVersionGuard {
  const descriptor = {
    __nativeWrite: true as const,
    context: undefined as GuardContext | undefined,
    withContext(context: GuardContext) {
      descriptor.context = { ...descriptor.context, ...context };
      return descriptor;
    },
    bind() {
      return descriptor;
    },
    async run<T>() {
      await dispatchGuard(descriptor.context, input);
      return { results: [], success: true, meta: { changes: 0, duration: 0 } } as SqlResult<T>;
    },
    async first<T>() {
      await dispatchGuard(descriptor.context, input);
      return null as T | null;
    },
    async all<T>() {
      await dispatchGuard(descriptor.context, input);
      return { results: [], success: true, meta: { changes: 0, duration: 0 } } as SqlResult<never>;
    },
    async raw() {
      await dispatchGuard(descriptor.context, input);
      return [];
    },
  };
  return descriptor as unknown as NativeAttachmentVersionGuard;
}
