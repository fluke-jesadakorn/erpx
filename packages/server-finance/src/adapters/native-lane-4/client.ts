import 'server-only';
import { ApiError } from '@erpx/server-runtime/public/http';
import { nativeRequest } from '@erpx/server-runtime/public/native-http';

/**
 * Strict transport for the native Finance reference-option reads (lane 4).
 *
 * The axum owner executes the three SELECTs and projects the rows; this client
 * emits no SQL, accepts no SQL from a caller and offers no TypeScript fallback.
 * Every registered Holding carries its own origin, Host, identity and optional
 * Company binding. The target is selected only from the caller's WorkContext
 * `(holdingId, holdingSlug)`; there is no first-Holding default and an unknown
 * or ambiguous target fails closed.
 */

/** Bounded WorkContext identity carried by the caller. */
export type Lane4Actor = Readonly<{
  holdingId?: string;
  holdingSlug?: string;
  companyId?: string;
}>;
type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
type Settings = Readonly<{
  base: string;
  timeoutMs: number;
  holdings: readonly Holding[];
}>;

export type ReferenceKind = 'accounts' | 'charts' | 'parties';
export type GuardKind = 'chart_active' | 'account_liability' | 'party_customer';
export type GuardArgs = Readonly<{ id: string; chartId?: string; currency: string }>;
export type AccountOption = Readonly<{
  id: string;
  name: string;
  code: string;
  chart_id: string;
  category: string;
}>;
export type NamedOption = Readonly<{ id: string; name: string }>;

const ENV = 'ERPX_NATIVE_FINANCE_LANE4_CONFIG';
const BODY_LIMIT = 2 * 1024 * 1024;
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const KINDS: readonly ReferenceKind[] = ['accounts', 'charts', 'parties'];
const GUARD_KINDS: readonly GuardKind[] = ['chart_active', 'account_liability', 'party_customer'];

const fail = (status = 503, code = 'NATIVE_FINANCE_LANE4_UNAVAILABLE'): never => {
  throw new ApiError(status, code, 'The native Finance reference-option read is unavailable.');
};

const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max = 200): value is string =>
  typeof value === 'string' && value.length > 0 && new TextEncoder().encode(value).length <= max;
const validId = (value: unknown): value is string =>
  text(value, 200) && !/[\u0000-\u001f/?#\\]/.test(value);
const validSlug = (value: unknown): value is string =>
  text(value, 128) && /^[a-z0-9_-]+$/.test(value);

function configuration(): Settings {
  const raw = process.env[ENV];
  if (raw === undefined) return fail();
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return fail();
  }
  if (
    !object(value) ||
    !text(value.privateBaseUrl, 512) ||
    typeof value.timeoutMs !== 'number' ||
    !Number.isSafeInteger(value.timeoutMs) ||
    value.timeoutMs < 250 ||
    value.timeoutMs > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
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
    base.origin !== value.privateBaseUrl
  )
    return fail();
  const holdings: Holding[] = [];
  const hosts = new Set<string>();
  const identities = new Set<string>();
  for (const item of value.holdings) {
    if (
      !object(item) ||
      !text(item.origin, 512) ||
      !text(item.host, 512) ||
      !text(item.holdingId, 200) ||
      !validSlug(item.holdingSlug) ||
      (item.companyId !== undefined && !text(item.companyId, 200))
    )
      return fail();
    let origin: URL;
    try {
      origin = new URL(item.origin);
    } catch {
      return fail();
    }
    // The configured Host must be the exact origin Host: a config cannot
    // redirect one Holding's request to another registered target.
    if (
      origin.protocol !== 'https:' ||
      origin.origin !== item.origin ||
      origin.username ||
      origin.password ||
      item.host !== origin.host
    )
      return fail();
    const identity = JSON.stringify([item.holdingId, item.holdingSlug]);
    if (hosts.has(item.host) || identities.has(identity)) return fail();
    hosts.add(item.host);
    identities.add(identity);
    holdings.push({
      origin: origin.origin,
      host: String(item.host),
      holdingId: String(item.holdingId),
      holdingSlug: String(item.holdingSlug),
      ...(item.companyId !== undefined ? { companyId: String(item.companyId) } : {}),
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs, holdings };
}

async function readBody(body: ReadableStream<Uint8Array>): Promise<string> {
  const reader = body.getReader();
  const chunks: Uint8Array[] = [];
  let bytes = 0;
  try {
    for (;;) {
      const next = await reader.read();
      if (next.done) break;
      bytes += next.value.byteLength;
      if (bytes > BODY_LIMIT) {
        await reader.cancel();
        return fail(502, 'NATIVE_FINANCE_LANE4_RESPONSE_INVALID');
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
  return new TextDecoder('utf-8', { fatal: true }).decode(data);
}

function rows(value: unknown, kind: ReferenceKind): unknown[] | never {
  if (!object(value) || !Array.isArray(value.data)) return fail(502, 'NATIVE_FINANCE_LANE4_RESPONSE_INVALID');
  for (const row of value.data) {
    if (!object(row) || !text(row.id) || !text(row.name)) return fail(502, 'NATIVE_FINANCE_LANE4_RESPONSE_INVALID');
    if (kind === 'accounts') {
      if (!text(row.code) || !text(row.chart_id) || !text(row.category))
        return fail(502, 'NATIVE_FINANCE_LANE4_RESPONSE_INVALID');
      if (Object.keys(row).length !== 5) return fail(502, 'NATIVE_FINANCE_LANE4_RESPONSE_INVALID');
    } else if (Object.keys(row).length !== 2) {
      return fail(502, 'NATIVE_FINANCE_LANE4_RESPONSE_INVALID');
    }
  }
  return value.data;
}

/** Resolve exactly one registered Holding from the caller's WorkContext. */
function target(actor: Lane4Actor, companyId: string): Holding {
  if (
    !actor ||
    !validId(actor.holdingId) ||
    !validSlug(actor.holdingSlug) ||
    !validId(companyId)
  )
    return fail(422, 'NATIVE_FINANCE_LANE4_REQUEST_INVALID');
  const config = configuration();
  const selected = config.holdings.find(
    (holding) =>
      holding.holdingId === actor.holdingId && holding.holdingSlug === actor.holdingSlug,
  );
  if (!selected) throw new ApiError(409, 'WORK_CONTEXT_CHANGED', 'The selected Holding is unavailable.');
  if (
    (selected.companyId !== undefined && selected.companyId !== companyId) ||
    (actor.companyId !== undefined && actor.companyId !== companyId)
  )
    throw new ApiError(409, 'WORK_CONTEXT_CHANGED', 'The selected Holding is unavailable.');
  return selected;
}

/** Call one registered native read and validate the exact response shape. */
export async function financeReferenceOptions(
  kind: 'accounts',
  companyId: string,
  actor: Lane4Actor,
): Promise<AccountOption[]>;
export async function financeReferenceOptions(
  kind: 'charts' | 'parties',
  companyId: string,
  actor: Lane4Actor,
): Promise<NamedOption[]>;
export async function financeReferenceOptions(
  kind: ReferenceKind,
  companyId: string,
  actor: Lane4Actor,
): Promise<AccountOption[] | NamedOption[]> {
  if (!KINDS.includes(kind)) return fail(422, 'NATIVE_FINANCE_LANE4_REQUEST_INVALID');
  const config = configuration();
  const selected = target(actor, companyId);
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(selected.holdingSlug)}/finance/native-lane-4/reference-options`,
    config.base,
  );
  const headers = new Headers({
    host: selected.host,
    origin: selected.origin,
    accept: 'application/json',
    'cache-control': 'no-cache',
    'x-erpx-context-company': companyId,
    'x-erpx-lane4-kind': kind,
  });
  let response: Response;
  try {
    response = await nativeRequest(
      url,
      {
        method: 'GET',
        headers: Object.fromEntries(headers),
        signal: AbortSignal.timeout(config.timeoutMs),
      },
      config.timeoutMs,
    );
  } catch {
    return fail();
  }
  if (response.redirected || response.status !== 200 || !response.body) {
    void response.body?.cancel().catch(() => undefined);
    return fail(response.status >= 400 && response.status < 500 ? response.status : 503);
  }
  if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    void response.body.cancel().catch(() => undefined);
    return fail(502, 'NATIVE_FINANCE_LANE4_RESPONSE_INVALID');
  }
  let payload: unknown;
  try {
    payload = JSON.parse(await readBody(response.body));
  } catch {
    return fail(502, 'NATIVE_FINANCE_LANE4_RESPONSE_INVALID');
  }
  if (!object(payload)) return fail(502, 'NATIVE_FINANCE_LANE4_RESPONSE_INVALID');
  if (payload.error && object(payload.error)) {
    return fail(502, 'NATIVE_FINANCE_LANE4_RESPONSE_INVALID');
  }
  return rows(payload, kind) as AccountOption[] | NamedOption[];
}

function guardRow(value: unknown): { id: string } | null | never {
  if (value === null) return null;
  if (
    !object(value) ||
    Object.keys(value).length !== 1 ||
    !text(value.id)
  )
    return fail(502, 'NATIVE_FINANCE_LANE4_RESPONSE_INVALID');
  return { id: String(value.id) };
}

/** Run one lock-free native posting guard probe on the caller's Holding. */
export async function financePostingGuard(
  kind: GuardKind,
  companyId: string,
  actor: Lane4Actor,
  args: GuardArgs,
): Promise<{ id: string } | null> {
  if (!GUARD_KINDS.includes(kind)) return fail(422, 'NATIVE_FINANCE_LANE4_REQUEST_INVALID');
  if (
    !validId(args.id) ||
    !text(args.currency, 16) ||
    !/^[A-Z0-9]{1,16}$/.test(args.currency) ||
    (kind === 'account_liability' && !validId(args.chartId)) ||
    (args.chartId !== undefined && !validId(args.chartId))
  )
    return fail(422, 'NATIVE_FINANCE_LANE4_REQUEST_INVALID');
  const config = configuration();
  const selected = target(actor, companyId);
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(selected.holdingSlug)}/finance/native-lane-4/posting-guard`,
    config.base,
  );
  const headers = new Headers({
    host: selected.host,
    origin: selected.origin,
    accept: 'application/json',
    'cache-control': 'no-cache',
    'x-erpx-context-company': companyId,
    'x-erpx-lane4-kind': kind,
    'x-erpx-lane4-id': args.id,
    'x-erpx-lane4-currency': args.currency,
    ...(args.chartId !== undefined ? { 'x-erpx-lane4-chart': args.chartId } : {}),
  });
  let response: Response;
  try {
    response = await nativeRequest(
      url,
      {
        method: 'GET',
        headers: Object.fromEntries(headers),
        signal: AbortSignal.timeout(config.timeoutMs),
      },
      config.timeoutMs,
    );
  } catch {
    return fail();
  }
  if (response.redirected || response.status !== 200 || !response.body) {
    void response.body?.cancel().catch(() => undefined);
    return fail(response.status >= 400 && response.status < 500 ? response.status : 503);
  }
  if (response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') {
    void response.body.cancel().catch(() => undefined);
    return fail(502, 'NATIVE_FINANCE_LANE4_RESPONSE_INVALID');
  }
  let payload: unknown;
  try {
    payload = JSON.parse(await readBody(response.body));
  } catch {
    return fail(502, 'NATIVE_FINANCE_LANE4_RESPONSE_INVALID');
  }
  if (!object(payload)) return fail(502, 'NATIVE_FINANCE_LANE4_RESPONSE_INVALID');
  if (payload.error && object(payload.error)) return fail(502, 'NATIVE_FINANCE_LANE4_RESPONSE_INVALID');
  return guardRow(payload.data);
}

/** True when the native lane-4 consumer switch is configured. */
export function nativeFinanceLane4Configured(
  environment: Readonly<Record<string, string | undefined>> = process.env,
): boolean {
  return environment[ENV] !== undefined;
}
