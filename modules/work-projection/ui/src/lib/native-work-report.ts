import 'server-only';
import { Agent, request as httpRequest } from 'node:http';
import { Readable } from 'node:stream';

/**
 * The single read transport for the Work report projection. It sends the
 * already-authorized, policy-scoped rows as JSON to the private native
 * `/work/native-report` axum route, which EXECUTES the matching, aggregation
 * and pagination PostgreSQL read on the caller's Holding transaction.
 *
 * No SQL, no JSON SQL mirror and no fallback exist here: an absent transport or
 * invalid reply fails closed. Transport settings come from the validated
 * Holding loopback owner-read (or owner-write) configuration.
 */
export type NativeWorkReportConfig = Readonly<{
  privateBaseUrl: string;
  holdingSlug: string;
  origin: string;
  timeoutMs: number;
}>;

export class NativeWorkReportError extends Error {
  readonly status?: number;
  constructor(code: string, status?: number) {
    super(code);
    this.name = 'NativeWorkReportError';
    this.status = status;
  }
}

const fail = (status?: number): never => {
  throw new NativeWorkReportError('NATIVE_WORK_REPORT_UNAVAILABLE', status);
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const RESPONSE_LIMIT = 8 * 1024 * 1024;

function configuration(raw: string | undefined): NativeWorkReportConfig {
  if (raw === undefined) return fail();
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return fail();
  }
  if (!record(value) || typeof value.privateBaseUrl !== 'string') return fail();
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
  if (
    typeof value.holdingSlug !== 'string' ||
    !/^[a-z0-9_-]+$/.test(value.holdingSlug) ||
    typeof value.origin !== 'string' ||
    value.origin !== new URL(value.origin).origin ||
    !value.origin.startsWith('https://')
  )
    return fail();
  return {
    privateBaseUrl: value.privateBaseUrl,
    holdingSlug: value.holdingSlug,
    origin: value.origin,
    timeoutMs,
  };
}

function directHttp(
  url: URL,
  headers: Record<string, string>,
  body: string,
  signal: AbortSignal,
): Promise<Response> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({ keepAlive: false, maxSockets: 1, maxTotalSockets: 1, proxyEnv: {} as import('node:http').ProxyEnv });
    const outgoing = httpRequest(
      url,
      { method: 'POST', agent, headers: { ...headers, 'content-length': String(Buffer.byteLength(body)) }, signal, maxHeaderSize: 8192 },
      incoming => {
        incoming.once('close', () => agent.destroy());
        try {
          const status = incoming.statusCode ?? 502;
          if (status !== 200) {
            incoming.destroy();
            resolve(new Response(null, { status: status >= 200 && status <= 599 ? status : 502 }));
            return;
          }
          const values = new Headers();
          for (let i = 0; i < incoming.rawHeaders.length; i += 2)
            values.append(incoming.rawHeaders[i]!, incoming.rawHeaders[i + 1]!);
          resolve(new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, { status, headers: values }));
        } catch (error) {
          incoming.destroy();
          agent.destroy();
          reject(error);
        }
      },
    );
    outgoing.once('error', error => { agent.destroy(); reject(error); });
    outgoing.write(body);
    outgoing.end();
  });
}

export type NativeWorkReportTransport = (
  url: URL,
  headers: Record<string, string>,
  body: string,
  signal: AbortSignal,
) => Promise<Response>;

export type NativeWorkReportActor = {
  companyId: string;
  assignmentId: string;
  revision: number;
  assignmentVersion: number;
  branchSlug?: string | null;
  departmentSlug?: string | null;
};

export type NativeWorkReportInput = {
  mode: 'period' | 'open' | 'drafts';
  scoped: string;
  start: number;
  end: number;
  page: number;
  exportAll: boolean;
};

export type NativeWorkReportProjection = {
  counts: { total: number; open: number; drafts: number; unknown: number; blocked: number; units: number };
  money: { currency: string; amount: number }[];
  estimates: { currency: string; amount: number }[];
  stageCounts: { stage: string; count: number; oldest: number | null }[];
  rows: Record<string, unknown>[];
  allRows: Record<string, unknown>[];
};

const integer = (value: unknown, min = 0): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= min;
const text = (value: unknown, max = 8192): value is string =>
  typeof value === 'string' && Buffer.byteLength(value) <= max;

export function validateReportProjection(value: unknown, input: NativeWorkReportInput): NativeWorkReportProjection {
  if (!record(value)) return fail();
  const counts = value.counts;
  if (
    !record(counts) ||
    !integer(counts.total) ||
    !integer(counts.open) ||
    !integer(counts.drafts) ||
    !integer(counts.unknown) ||
    !integer(counts.blocked) ||
    typeof counts.units !== 'number' ||
    !Number.isFinite(counts.units)
  )
    return fail();
  const totals = (value: unknown) => {
    if (!Array.isArray(value) || value.length > 100_000) return fail();
    return value.map(row => {
      if (!record(row) || !text(row.currency, 64) || !Number.isSafeInteger(row.amount)) return fail();
      return { currency: row.currency as string, amount: row.amount as number };
    });
  };
  const stages = (value: unknown) => {
    if (!Array.isArray(value) || value.length > 1000) return fail();
    return value.map(row => {
      if (
        !record(row) ||
        !text(row.stage, 128) ||
        !Number.isSafeInteger(row.count) ||
        !(row.oldest === null || (typeof row.oldest === 'number' && Number.isSafeInteger(row.oldest)))
      )
        return fail();
      return { stage: row.stage as string, count: row.count as number, oldest: row.oldest as number | null };
    });
  };
  const rows = (value: unknown, limit: number) => {
    if (!Array.isArray(value) || value.length > limit || !value.every(record)) return fail();
    return value as Record<string, unknown>[];
  };
  const projection: NativeWorkReportProjection = {
    counts: {
      total: counts.total,
      open: counts.open,
      drafts: counts.drafts,
      unknown: counts.unknown,
      blocked: counts.blocked,
      units: counts.units,
    },
    money: totals(value.money),
    estimates: totals(value.estimates),
    stageCounts: stages(value.stageCounts),
    rows: rows(value.rows, 50),
    allRows: input.exportAll ? rows(value.allRows, 100_000) : [],
  };
  return projection;
}

/** True when the native Work report consumer switch is configured. */
export function nativeWorkReportConfigured(): boolean {
  return (
    process.env.ERPX_NATIVE_WORK_PROJECTION_OWNER_READ_CONFIG !== undefined ||
    process.env.ERPX_NATIVE_WORK_PROJECTION_OWNER_WRITE_CONFIG !== undefined
  );
}

/** Reads the native report projection through the private loopback route. */
export async function readNativeWorkReport(
  input: NativeWorkReportInput,
  actor: NativeWorkReportActor,
  transport: NativeWorkReportTransport = directHttp,
): Promise<NativeWorkReportProjection> {
  const config = configuration(
    process.env.ERPX_NATIVE_WORK_PROJECTION_OWNER_READ_CONFIG ??
      process.env.ERPX_NATIVE_WORK_PROJECTION_OWNER_WRITE_CONFIG,
  );
  if (
    !text(actor.companyId, 200) ||
    !text(actor.assignmentId, 200) ||
    !integer(actor.revision) ||
    !integer(actor.assignmentVersion, 1) ||
    !['period', 'open', 'drafts'].includes(input.mode) ||
    !integer(input.start) ||
    !integer(input.end) ||
    !integer(input.page, 1) ||
    !input.scoped.startsWith('[')
  )
    return fail();
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(config.holdingSlug)}/work/native-report`,
    config.privateBaseUrl,
  );
  const body = JSON.stringify({
    mode: input.mode,
    scoped: input.scoped,
    start: input.start,
    end: input.end,
    page: input.page,
    exportAll: input.exportAll,
  });
  if (Buffer.byteLength(body) > 5 * 1024 * 1024) return fail();
  const headers: Record<string, string> = {
    host: new URL(config.origin).host,
    origin: config.origin,
    accept: 'application/json',
    'content-type': 'application/json',
    'cache-control': 'no-cache',
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-assignment': actor.assignmentId,
    'x-erpx-context-revision': String(actor.revision),
    'x-erpx-context-assignment-version': String(actor.assignmentVersion),
  };
  if (actor.branchSlug) headers['x-erpx-workspace-branch'] = actor.branchSlug;
  if (actor.departmentSlug) headers['x-erpx-workspace-department'] = actor.departmentSlug;
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), config.timeoutMs);
  try {
    const response = await transport(url, headers, body, controller.signal);
    if (response.redirected || response.status !== 200 || !response.body) return fail(response.status);
    const raw = await response.text();
    if (Buffer.byteLength(raw) > RESPONSE_LIMIT) return fail();
    const payload = JSON.parse(raw) as { data?: unknown };
    if (!record(payload) || !record(payload.data)) return fail();
    return validateReportProjection(payload.data, input);
  } catch (error) {
    if (error instanceof NativeWorkReportError) throw error;
    return fail();
  } finally {
    clearTimeout(timer);
  }
}
