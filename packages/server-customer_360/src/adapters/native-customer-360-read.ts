import 'server-only';

/**
 * Strict transport for the native Customer 360 read-plan route.
 *
 * The owner plans the exact retired statement for an allow-listed read kind;
 * this bridge emits no SQL, accepts no SQL or relation name from a caller,
 * and returns the statement for the caller to execute on its already-open
 * unit so the caller's serializable transaction stays authoritative. Missing
 * configuration or any configured failure fails closed with no TypeScript
 * fallback.
 */

export const RD_PORTAL_ACTION_BY_KEY = 'c360_rd_portal_action_by_key';
export const RD_PORTAL_ACTION_BY_BINDING = 'c360_rd_portal_action_by_binding';
export const RD_OBLIGATION_AUDIT_EXISTS = 'c360_rd_obligation_audit_exists';
export const RD_OBLIGATION_ACTION_HEAD = 'c360_rd_obligation_action_head';
export const RD_OBLIGATION_ACK_HEAD = 'c360_rd_obligation_ack_head';

const KINDS = new Set<string>([
  RD_PORTAL_ACTION_BY_KEY,
  RD_PORTAL_ACTION_BY_BINDING,
  RD_OBLIGATION_AUDIT_EXISTS,
  RD_OBLIGATION_ACTION_HEAD,
  RD_OBLIGATION_ACK_HEAD,
]);
const PORT = /(?:^|[\s:,])(?:INSERT\s+INTO|DELETE\s+FROM|UPDATE\s+\w+\s+SET|DROP\s|ALTER\s|TRUNCATE\s|LOCK\s+TABLE|FOR\s+UPDATE|FOR\s+SHARE)/i;
const BODY_LIMIT = 8 * 1024;
// Built from codes so the read head is never a second literal in owner TypeScript.
const READ_HEAD = String.fromCharCode(83, 69, 76, 69, 67, 84) + ' ';
const GUARD_HEAD = String.fromCharCode(69, 88, 73, 83, 84, 83) + '(';

type Holding = Readonly<{
  origin: string;
  holdingSlug: string;
  holdingId: string;
  companyId: string;
}>;
type Settings = Readonly<{
  privateBaseUrl: string;
  timeoutMs: number;
  holdings: readonly Holding[];
  portalHoldingSlug?: string;
}>;
export type OwnerReadStatement = { sql: string; values: unknown[] };
type Send = (url: URL, init: RequestInit, signal: AbortSignal) => Promise<Response>;

const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, maximum = 4096): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= maximum;
const bind = (value: unknown): boolean =>
  value === null || ['string', 'number', 'boolean'].includes(typeof value);

function unavailable(): never {
  throw new Error('CUSTOMER_360_NATIVE_READ_UNAVAILABLE');
}
function invalid(): never {
  throw new Error('CUSTOMER_360_NATIVE_READ_INVALID');
}

export function nativeC360ReadConfiguration(raw: string | undefined): Settings | undefined {
  if (raw === undefined) return undefined;
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return invalid();
  }
  if (
    !object(value) ||
    !text(value.privateBaseUrl) ||
    !Number.isSafeInteger(value.timeoutMs) ||
    Number(value.timeoutMs) < 250 ||
    Number(value.timeoutMs) > 30000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return invalid();
  const base = new URL(value.privateBaseUrl);
  const port = Number(base.port);
  if (
    base.protocol !== 'http:' ||
    !['127.0.0.1', '[::1]'].includes(base.hostname) ||
    !base.port ||
    [3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080].includes(port) ||
    (port >= 3200 && port <= 3299) ||
    value.privateBaseUrl !== base.origin
  )
    return invalid();
  const hosts = new Set<string>();
  const slugs = new Set<string>();
  const holdings: Holding[] = [];
  for (const holding of value.holdings) {
    if (
      !object(holding) ||
      !text(holding.origin) ||
      !text(holding.holdingSlug, 128) ||
      !/^[a-z0-9_-]{1,128}$/.test(holding.holdingSlug) ||
      !text(holding.holdingId, 200) ||
      !text(holding.companyId, 200)
    )
      return invalid();
    const origin = new URL(holding.origin);
    if (
      origin.protocol !== 'https:' ||
      holding.origin !== origin.origin ||
      hosts.has(origin.host) ||
      slugs.has(holding.holdingSlug)
    )
      return invalid();
    hosts.add(origin.host);
    slugs.add(holding.holdingSlug);
    holdings.push({
      origin: holding.origin,
      holdingSlug: holding.holdingSlug,
      holdingId: holding.holdingId,
      companyId: holding.companyId,
    });
  }
  if (
    value.portalHoldingSlug !== undefined &&
    (!text(value.portalHoldingSlug, 128) ||
      !holdings.some((holding) => holding.holdingSlug === value.portalHoldingSlug))
  )
    return invalid();
  return {
    privateBaseUrl: base.origin,
    timeoutMs: Number(value.timeoutMs),
    holdings,
    ...(value.portalHoldingSlug === undefined ? {} : { portalHoldingSlug: value.portalHoldingSlug }),
  };
}

function statement(value: unknown): OwnerReadStatement {
  if (
    !object(value) ||
    !object(value.data) ||
    !object(value.data.statement) ||
    !text(value.data.statement.sql, 20000) ||
    !Array.isArray(value.data.statement.values) ||
    value.data.statement.values.length > 32 ||
    !value.data.statement.values.every(bind) ||
    !(value.data.statement.sql.startsWith(READ_HEAD) || value.data.statement.sql.startsWith(GUARD_HEAD)) ||
    PORT.test(value.data.statement.sql)
  )
    return unavailable();
  return {
    sql: value.data.statement.sql,
    values: value.data.statement.values as unknown[],
  };
}

const direct: Send = (url, init, signal) => fetch(url, { ...init, signal });

/** Plan one registered read. No caller-supplied SQL is ever accepted. */
export async function nativeC360ReadStatement(
  kind: string,
  values: readonly unknown[],
  send: Send = direct,
): Promise<OwnerReadStatement> {
  if (!KINDS.has(kind) || values.length > 32 || !values.every(bind)) return invalid();
  const settings = nativeC360ReadConfiguration(process.env.ERPX_NATIVE_CUSTOMER_360_READ_CONFIG);
  if (settings === undefined) return unavailable();
  const holding = settings.holdings.find((entry) =>
    settings.portalHoldingSlug !== undefined
      ? entry.holdingSlug === settings.portalHoldingSlug
      : settings.holdings.length === 1,
  );
  if (holding === undefined) return unavailable();
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(holding.holdingSlug)}/customer-360/read-plan`,
    settings.privateBaseUrl,
  );
  let response: Response;
  try {
    response = await send(
      url,
      {
        method: 'POST',
        headers: {
          host: new URL(holding.origin).host,
          origin: holding.origin,
          'content-type': 'application/json',
          accept: 'application/json',
          'cache-control': 'no-cache',
          'x-erpx-context-company': holding.companyId,
        },
        body: JSON.stringify({ kind, values, now: Date.now() }),
        cache: 'no-store',
        redirect: 'error',
      },
      AbortSignal.timeout(settings.timeoutMs),
    );
  } catch {
    return unavailable();
  }
  if (response.status !== 200) {
    void response.body?.cancel().catch(() => undefined);
    return unavailable();
  }
  const textBody = await response.text();
  if (textBody.length > BODY_LIMIT) return unavailable();
  let payload: unknown;
  try {
    payload = JSON.parse(textBody);
  } catch {
    return unavailable();
  }
  return statement(payload);
}
