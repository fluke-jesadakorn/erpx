import 'server-only';
import {Agent, request as httpRequest} from 'node:http';
import {Readable} from 'node:stream';
import {ApiError} from '@erpx/server-runtime/public/http';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';

/**
 * Strict private transport for the native lane 29 Work availability reads.
 * The axum owner executes the SQL and owns the projection, Company scope and
 * response shape. This client emits no SQL, accepts no SQL from a caller and
 * never falls back to TypeScript business logic: missing configuration, a
 * transport failure or a malformed payload fails closed with an ApiError.
 */

export type NativeLane29Kind =
  | 'occurrence_exception'
  | 'moved_occurrences'
  | 'availability_sources'
  | 'series_batch';

export type NativeOccurrenceException = {
  start_local: string | null;
  end_local: string | null;
  cancelled: boolean;
};

export type NativeMovedOccurrence = {
  occurrence_key: string;
  start_local: string;
  end_local: string;
};

export type NativeAvailabilitySource = {
  id: string;
  starts_at: number;
  ends_at: number;
  timezone: string;
  status: string;
  all_day: boolean;
  frequency: string | null;
  interval_count: number | null;
  weekdays: string | null;
  end_kind: string | null;
  occurrence_count: number | null;
  until_date: string | null;
  user_id: string;
  response: string;
};

export type NativeSeriesRow = {
  appointment_id: string;
  company_id: string;
  frequency: string;
  interval_count: number;
  weekdays: string;
  end_kind: string;
  occurrence_count: number | null;
  until_date: string | null;
  version: number;
};

export type NativeLane29Input =
  | {kind: 'occurrence_exception'; companyId: string; appointmentId: string; occurrenceKey: string}
  | {kind: 'moved_occurrences'; companyId: string; appointmentId: string}
  | {kind: 'availability_sources'; companyId: string; before: number; userIds: string[]}
  | {kind: 'series_batch'; companyId: string; appointmentIds: string[]};

export type NativeLane29Transport = (
  url: URL,
  headers: Headers,
  body: string,
  signal: AbortSignal,
) => Promise<Response>;

/** Exact registered Holding entry. `companyId` is optional metadata used only
 * to resolve a Company-only caller; it never grants authority by itself. */
export type Lane29Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;

export type Lane29Context = Readonly<{
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;

type Lane29Config = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: readonly Lane29Holding[];
}>;

const CONFIG_ENV = 'ERPX_NATIVE_WORK_PROJECTION_CONFIG';
const BODY_LIMIT = 192 * 1024;
const MAX_BEFORE_MS = 4_102_444_800_000;
const AVAILABILITY_KEYS = [
  'all_day',
  'end_kind',
  'ends_at',
  'frequency',
  'id',
  'interval_count',
  'occurrence_count',
  'response',
  'starts_at',
  'status',
  'timezone',
  'until_date',
  'user_id',
  'weekdays',
] as const;

const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max = 8192): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const identifier = (value: unknown): value is string =>
  text(value, 200) && !/[\u0000-\u001f\u007f/\\]/.test(value);
const occurrenceKey = (value: unknown): value is string =>
  text(value, 128) && /^[A-Za-z0-9._:+/-]+$/.test(value);
const integer = (value: unknown, max = Number.MAX_SAFE_INTEGER): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 && value <= max;

const fail = (status: number, code: string, message: string): never => {
  throw new ApiError(status, code, message);
};
const unavailable = (code = 'NATIVE_WORK_LANE29_UNAVAILABLE'): never =>
  fail(503, `${code}_UNAVAILABLE`, 'The native Work availability read is unavailable.');
const invalidResponse = (): never =>
  fail(502, 'NATIVE_WORK_LANE29_RESPONSE_INVALID', 'The native Work availability read did not match its contract.');

const configurationInvalid = (): never =>
  fail(503, 'NATIVE_WORK_LANE29_CONFIGURATION_INVALID', 'The native Work availability read is not configured.');

/** Strict 1..64 Holding configuration. Every entry carries its exact Origin,
 * Host, Holding id and slug; Company is optional metadata, never a selector by
 * itself except for Company-only callers resolved below. */
function configuration(raw: string | undefined): Lane29Config {
  if (raw === undefined) return unavailable('NATIVE_WORK_LANE29_CONFIGURATION_REQUIRED');
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return configurationInvalid();
  }
  if (!object(value)) return configurationInvalid();
  const base = value.privateBaseUrl;
  const timeoutMs = value.timeoutMs;
  const holdings = value.holdings;
  if (
    !text(base, 512) ||
    typeof timeoutMs !== 'number' ||
    !Number.isSafeInteger(timeoutMs) ||
    timeoutMs < 250 ||
    timeoutMs > 30_000 ||
    !Array.isArray(holdings) ||
    holdings.length < 1 ||
    holdings.length > 64
  )
    return configurationInvalid();
  let url: URL;
  try {
    url = new URL(base);
  } catch {
    return configurationInvalid();
  }
  const port = Number(url.port);
  if (
    url.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(url.hostname) ||
    !url.port ||
    !Number.isInteger(port) ||
    port < 1 ||
    port > 65535 ||
    url.username ||
    url.password ||
    url.origin !== base
  )
    return configurationInvalid();
  const seenHosts = new Set<string>();
  const seenIds = new Set<string>();
  const seenSlugs = new Set<string>();
  const entries: Lane29Holding[] = [];
  for (const item of holdings) {
    if (!object(item)) return configurationInvalid();
    const {origin, host, holdingId, holdingSlug, companyId} = item;
    if (
      !text(origin, 512) ||
      !text(host, 256) ||
      !identifier(holdingId) ||
      !text(holdingSlug, 128) ||
      !/^[a-z0-9_-]+$/.test(holdingSlug) ||
      (companyId !== undefined && !identifier(companyId))
    )
      return configurationInvalid();
    let originUrl: URL;
    try {
      originUrl = new URL(origin);
    } catch {
      return configurationInvalid();
    }
    if (
      originUrl.protocol !== 'https:' ||
      originUrl.origin !== origin ||
      originUrl.username ||
      originUrl.password ||
      originUrl.host !== host ||
      seenHosts.has(host) ||
      seenIds.has(holdingId) ||
      seenSlugs.has(holdingSlug)
    )
      return configurationInvalid();
    seenHosts.add(host);
    seenIds.add(holdingId);
    seenSlugs.add(holdingSlug);
    entries.push({
      origin,
      host,
      holdingId,
      holdingSlug,
      ...(companyId === undefined ? {} : {companyId}),
    });
  }
  return {privateBaseUrl: url.origin, timeoutMs, holdings: entries};
}

/** Resolve the caller's WorkContext against the exact registered Holding. No
 * first-holding default exists: a missing or mismatched pair fails closed. */
export function selectLane29Holding(config: Lane29Config, context: Lane29Context): Lane29Holding {
  if (!identifier(context.holdingId) || !text(context.holdingSlug, 128) || !/^[a-z0-9_-]+$/.test(context.holdingSlug))
    return fail(409, 'WORK_CONTEXT_CHANGED', 'The current Work context is unavailable.');
  const selected = config.holdings.find(
    holding => holding.holdingId === context.holdingId && holding.holdingSlug === context.holdingSlug,
  );
  if (!selected || (context.companyId !== undefined && selected.companyId !== undefined && context.companyId !== selected.companyId))
    return fail(503, 'NATIVE_WORK_LANE29_TARGET_UNAVAILABLE', 'The native Work availability read target is unavailable.');
  return selected;
}

/** Company-only callers resolve through the configured optional `companyId`
 * metadata; exactly one Holding may match. There is no positional default. */
export function lane29ContextForCompany(companyId: string): Lane29Context {
  if (!identifier(companyId)) return fail(422, 'LANE29_COMPANY_INVALID', 'Use a bounded Company.');
  const config = configuration(process.env[CONFIG_ENV]);
  const matches = config.holdings.filter(holding => holding.companyId === companyId);
  if (matches.length !== 1)
    return fail(503, 'NATIVE_WORK_LANE29_TARGET_UNAVAILABLE', 'The native Work availability read target is unavailable.');
  const holding = matches[0]!;
  return {holdingId: holding.holdingId, holdingSlug: holding.holdingSlug, companyId};
}

/** Adapter units bind the caller's WorkContext to their own database unit so
 * fact functions can select the exact Holding without carrying it per read. */
const contexts = new WeakMap<SqlDatabase, Lane29Context>();
export function bindLane29Context(db: SqlDatabase, context: Lane29Context): void {
  contexts.set(db, context);
}
export function lane29ContextFor(db: SqlDatabase, companyId: string): Lane29Context {
  const bound = contexts.get(db);
  if (bound && (bound.companyId === undefined || bound.companyId === companyId)) return bound;
  return lane29ContextForCompany(companyId);
}

/** Reject malformed inputs before any network work. Mirrors the owner
 * validation in `domain.rs`; no input becomes SQL on either side. */
export function validateNativeLane29Input(input: NativeLane29Input): void {
  if (!identifier(input.companyId)) return fail(422, 'LANE29_COMPANY_INVALID', 'Use a bounded Company.');
  if (input.kind === 'occurrence_exception') {
    if (!identifier(input.appointmentId) || !occurrenceKey(input.occurrenceKey))
      return fail(422, 'LANE29_OCCURRENCE_REQUEST_INVALID', 'Use one bounded appointment and occurrence key.');
  } else if (input.kind === 'moved_occurrences') {
    if (!identifier(input.appointmentId)) return fail(422, 'LANE29_APPOINTMENT_INVALID', 'Use one bounded appointment.');
  } else if (input.kind === 'availability_sources') {
    if (!integer(input.before, MAX_BEFORE_MS) || input.before === 0)
      return fail(422, 'LANE29_TIME_WINDOW_INVALID', 'Use a bounded millisecond time window.');
    if (
      !Array.isArray(input.userIds) ||
      input.userIds.length < 1 ||
      input.userIds.length > 64 ||
      !input.userIds.every(identifier)
    )
      return fail(422, 'LANE29_RECIPIENTS_INVALID', 'Use between one and 64 bounded recipients.');
  } else {
    if (
      !Array.isArray(input.appointmentIds) ||
      input.appointmentIds.length < 1 ||
      input.appointmentIds.length > 64 ||
      !input.appointmentIds.every(identifier)
    )
      return fail(422, 'LANE29_SERIES_REQUEST_INVALID', 'Use between one and 64 bounded appointments.');
  }
}

function validateException(value: unknown): NativeOccurrenceException {
  if (
    !object(value) ||
    Object.keys(value).length !== 3 ||
    !(value.start_local === null || text(value.start_local, 128)) ||
    !(value.end_local === null || text(value.end_local, 128)) ||
    typeof value.cancelled !== 'boolean'
  )
    return invalidResponse();
  return value as unknown as NativeOccurrenceException;
}

function validateMoved(value: unknown): NativeMovedOccurrence {
  if (
    !object(value) ||
    Object.keys(value).length !== 3 ||
    !text(value.occurrence_key, 128) ||
    !text(value.start_local, 128) ||
    !text(value.end_local, 128)
  )
    return invalidResponse();
  return value as unknown as NativeMovedOccurrence;
}

function validateAvailability(value: unknown): NativeAvailabilitySource {
  if (!object(value) || Object.keys(value).length !== AVAILABILITY_KEYS.length) return invalidResponse();
  if (
    !AVAILABILITY_KEYS.every(key => Object.prototype.hasOwnProperty.call(value, key)) ||
    !identifier(value.id) ||
    !integer(value.starts_at, MAX_BEFORE_MS) ||
    !integer(value.ends_at, MAX_BEFORE_MS) ||
    !text(value.timezone, 128) ||
    !text(value.status, 64) ||
    typeof value.all_day !== 'boolean' ||
    !(value.frequency === null || text(value.frequency, 32)) ||
    !(value.interval_count === null || integer(value.interval_count, 99)) ||
    !(value.weekdays === null || text(value.weekdays, 64)) ||
    !(value.end_kind === null || text(value.end_kind, 32)) ||
    !(value.occurrence_count === null || integer(value.occurrence_count, 1000)) ||
    !(value.until_date === null || text(value.until_date, 64)) ||
    !identifier(value.user_id) ||
    !text(value.response, 64)
  )
    return invalidResponse();
  return value as unknown as NativeAvailabilitySource;
}

function validateSeriesRow(value: unknown): NativeSeriesRow {
  if (
    !object(value) ||
    Object.keys(value).length !== 9 ||
    !identifier(value.appointment_id) ||
    !identifier(value.company_id) ||
    !text(value.frequency, 32) ||
    !integer(value.interval_count, 99) ||
    !text(value.weekdays, 64) ||
    !text(value.end_kind, 32) ||
    !(value.occurrence_count === null || integer(value.occurrence_count, 1000)) ||
    !(value.until_date === null || text(value.until_date, 64)) ||
    !integer(value.version, 1_000_000)
  )
    return invalidResponse();
  return value as unknown as NativeSeriesRow;
}

/** Pin the exact owner response shape. Exported for the contract test. */
export function parseNativeLane29Response(
  payload: unknown,
  kind: NativeLane29Kind,
  companyId: string,
):
  | NativeOccurrenceException
  | null
  | NativeMovedOccurrence[]
  | NativeAvailabilitySource[]
  | NativeSeriesRow[] {
  if (!object(payload) || Object.keys(payload).length !== 1) return invalidResponse();
  if (kind === 'occurrence_exception') {
    if (!Object.prototype.hasOwnProperty.call(payload, 'exception')) return invalidResponse();
    return payload.exception === null ? null : validateException(payload.exception);
  }
  if (kind === 'moved_occurrences') {
    if (!Array.isArray(payload.occurrences) || Object.keys(payload).length !== 1) return invalidResponse();
    return payload.occurrences.map(validateMoved);
  }
  if (kind === 'availability_sources') {
    if (!Array.isArray(payload.appointments)) return invalidResponse();
    // The native route also binds the Company; reject a response that cannot
    // be tied to the caller's Company if the transport were misconfigured.
    if (!identifier(companyId)) return fail(409, 'WORK_CONTEXT_CHANGED', 'The current Company changed.');
    return payload.appointments.map(validateAvailability);
  }
  if (!Array.isArray(payload.series)) return invalidResponse();
  return payload.series.map(validateSeriesRow);
}

function direct(
  url: URL,
  headers: Headers,
  body: string,
  signal: AbortSignal,
): Promise<Response> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as import('node:http').ProxyEnv,
    });
    const outgoing = httpRequest(
      url,
      {
        method: 'POST',
        headers: Object.fromEntries(headers),
        signal,
        agent,
        maxHeaderSize: 8192,
      },
      incoming => {
        incoming.once('close', () => agent.destroy());
        try {
          const output = new Headers();
          for (let i = 0; i < incoming.rawHeaders.length; i += 2)
            output.append(incoming.rawHeaders[i]!, incoming.rawHeaders[i + 1]!);
          resolve(
            new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, {
              status: incoming.statusCode ?? 502,
              headers: output,
            }),
          );
        } catch (error) {
          incoming.destroy();
          agent.destroy();
          reject(error);
        }
      },
    );
    outgoing.once('error', error => {
      agent.destroy();
      reject(error);
    });
    outgoing.end(body);
  });
}

async function boundedJson(response: Response, signal: AbortSignal): Promise<unknown> {
  if (
    response.redirected ||
    response.status !== 200 ||
    response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json' ||
    !response.body
  ) {
    void response.body?.cancel().catch(() => undefined);
    return unavailable();
  }
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let size = 0;
  const cancel = () => {
    void reader.cancel().catch(() => undefined);
  };
  signal.addEventListener('abort', cancel, {once: true});
  try {
    for (;;) {
      const part = await reader.read();
      if (signal.aborted) return unavailable();
      if (part.done) break;
      size += part.value.length;
      if (size > BODY_LIMIT) {
        await reader.cancel();
        return invalidResponse();
      }
      chunks.push(part.value);
    }
  } finally {
    signal.removeEventListener('abort', cancel);
    reader.releaseLock();
  }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.length;
  }
  try {
    return JSON.parse(new TextDecoder('utf-8', {fatal: true}).decode(bytes));
  } catch {
    return invalidResponse();
  }
}

function wire(input: NativeLane29Input) {
  switch (input.kind) {
    case 'occurrence_exception':
      return {
        kind: input.kind,
        companyId: input.companyId,
        appointmentId: input.appointmentId,
        occurrenceKey: input.occurrenceKey,
      };
    case 'moved_occurrences':
      return {kind: input.kind, companyId: input.companyId, appointmentId: input.appointmentId};
    case 'availability_sources':
      return {
        kind: input.kind,
        companyId: input.companyId,
        before: input.before,
        userIds: input.userIds,
      };
    case 'series_batch':
      return {kind: input.kind, companyId: input.companyId, appointmentIds: input.appointmentIds};
  }
}

/** Execute one owner read on the native route. `transport` is injectable only
 * so the contract test can exercise the fail-closed path without a process. */
export async function readNativeLane29<T>(
  input: NativeLane29Input,
  context: Lane29Context,
  transport: NativeLane29Transport = direct,
): Promise<T> {
  validateNativeLane29Input(input);
  const config = configuration(process.env[CONFIG_ENV]);
  const target = selectLane29Holding(config, context);
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), config.timeoutMs);
  try {
    const origin = target.origin;
    const headers = new Headers({
      host: new URL(origin).host,
      origin,
      accept: 'application/json',
      'content-type': 'application/json',
      'cache-control': 'no-cache',
      'x-erpx-context-company': input.companyId,
    });
    const body = JSON.stringify(wire(input));
    if (new TextEncoder().encode(body).length > BODY_LIMIT) return invalidResponse();
    const response = await transport(
      new URL(`/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/work/native-lane-29`, config.privateBaseUrl),
      headers,
      body,
      controller.signal,
    );
    if (response.status !== 200) {
      const payload = await response.json().catch((): unknown => null);
      void response.body?.cancel().catch(() => undefined);
      const error = object(payload) && object(payload.error) ? payload.error : undefined;
      if (response.status >= 400 && response.status < 500)
        throw new ApiError(
          response.status,
          text(error?.code, 100) ? String(error!.code) : 'NATIVE_WORK_LANE29_FAILED',
          text(error?.message, 300) ? String(error!.message) : 'The native Work availability read was rejected.',
        );
      return unavailable();
    }
    return parseNativeLane29Response(await boundedJson(response, controller.signal), input.kind, input.companyId) as T;
  } catch (error) {
    if (error instanceof ApiError) throw error;
    return unavailable();
  } finally {
    clearTimeout(timer);
  }
}

export const nativeOccurrenceException = (
  context: Lane29Context,
  companyId: string,
  appointmentId: string,
  occurrenceKey: string,
): Promise<NativeOccurrenceException | null> =>
  readNativeLane29({kind: 'occurrence_exception', companyId, appointmentId, occurrenceKey}, context);

export const nativeMovedOccurrences = (
  context: Lane29Context,
  companyId: string,
  appointmentId: string,
): Promise<NativeMovedOccurrence[]> =>
  readNativeLane29({kind: 'moved_occurrences', companyId, appointmentId}, context);

export const nativeAvailabilitySources = (
  context: Lane29Context,
  companyId: string,
  before: number,
  userIds: string[],
): Promise<NativeAvailabilitySource[]> =>
  readNativeLane29({kind: 'availability_sources', companyId, before, userIds}, context);

export const nativeSeriesBatch = (
  context: Lane29Context,
  companyId: string,
  appointmentIds: string[],
): Promise<NativeSeriesRow[]> =>
  readNativeLane29({kind: 'series_batch', companyId, appointmentIds}, context);
