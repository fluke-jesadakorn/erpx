import 'server-only';
import { ApiError } from '@erpx/platform-runtime-server/public/http';

/**
 * Strict private transport for the native Legal rule-impact read. The Axum
 * owner executes the SQL and owns the Company scope, authority gate and
 * response shape; this bridge emits no SQL and accepts no SQL from a caller.
 * Missing configuration, an unregistered target, a transport failure or a
 * malformed payload all fail closed with no TypeScript fallback.
 */

type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
type Config = Readonly<{ base: string; timeoutMs: number; holdings: readonly Holding[] }>;

export type NativeRuleImpactActor = Readonly<{
  holdingId: string;
  holdingSlug: string;
  companyId: string;
  userId: string;
  branchId?: string | null;
  departmentId?: string | null;
}>;

export type NativeRuleImpactAuthority = Readonly<{
  companyEnabled: boolean;
  rulePurchased: boolean;
}>;

export type NativeRuleImpactItem = Readonly<{
  id: string;
  kind: string;
  title: string;
  state: string;
  due_date: string | null;
  version: number;
}>;

export type NativeRuleImpactRead = Readonly<{
  rule_id: string;
  rule_version: number;
  items: readonly NativeRuleImpactItem[];
  notice: string;
}>;

const ENV = 'ERPX_NATIVE_LEGAL_RULE_IMPACT_CONFIG';
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const unavailable = (code = 'NATIVE_LEGAL_RULE_IMPACT_UNAVAILABLE', status = 503): never => {
  throw new ApiError(status, code, 'The native Legal rule-impact service is unavailable.');
};
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const text = (value: unknown, max = 8192): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;
const safeInteger = (value: unknown): value is number =>
  typeof value === 'number' && Number.isSafeInteger(value);

function configuration(): Config {
  const raw = process.env[ENV];
  if (!raw) return unavailable('NATIVE_LEGAL_RULE_IMPACT_CONFIGURATION_REQUIRED');
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailable('NATIVE_LEGAL_RULE_IMPACT_CONFIGURATION_INVALID');
  }
  if (
    !object(value) ||
    !text(value.privateBaseUrl, 512) ||
    !safeInteger(value.timeoutMs) ||
    (value.timeoutMs as number) < 250 ||
    (value.timeoutMs as number) > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return unavailable('NATIVE_LEGAL_RULE_IMPACT_CONFIGURATION_INVALID');
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl as string);
  } catch {
    return unavailable('NATIVE_LEGAL_RULE_IMPACT_CONFIGURATION_INVALID');
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
    return unavailable('NATIVE_LEGAL_RULE_IMPACT_CONFIGURATION_INVALID');
  const holdings: Holding[] = [];
  const seenHosts = new Set<string>();
  for (const item of value.holdings) {
    if (
      !object(item) ||
      !text(item.origin, 512) ||
      !text(item.host, 512) ||
      !text(item.holdingId, 200) ||
      !text(item.holdingSlug, 128) ||
      (item.companyId !== undefined && !text(item.companyId, 200)) ||
      !/^[a-z0-9_-]+$/.test(String(item.holdingSlug)) ||
      !/^[A-Za-z0-9._:-]+$/.test(String(item.host))
    )
      return unavailable('NATIVE_LEGAL_RULE_IMPACT_CONFIGURATION_INVALID');
    let origin: URL;
    try {
      origin = new URL(String(item.origin));
    } catch {
      return unavailable('NATIVE_LEGAL_RULE_IMPACT_CONFIGURATION_INVALID');
    }
    if (
      origin.protocol !== 'https:' ||
      origin.origin !== item.origin ||
      origin.username ||
      origin.password ||
      String(item.host) !== origin.host ||
      seenHosts.has(String(item.host))
    )
      return unavailable('NATIVE_LEGAL_RULE_IMPACT_CONFIGURATION_INVALID');
    seenHosts.add(String(item.host));
    holdings.push({
      origin: origin.origin,
      host: String(item.host),
      holdingId: String(item.holdingId),
      holdingSlug: String(item.holdingSlug),
      ...(item.companyId === undefined ? {} : { companyId: String(item.companyId) }),
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs as number, holdings };
}

function isItem(value: unknown): value is NativeRuleImpactItem {
  return (
    object(value) &&
    text(value.id, 200) &&
    text(value.kind, 100) &&
    text(value.title, 8192) &&
    text(value.state, 100) &&
    (value.due_date === null || text(value.due_date, 64)) &&
    safeInteger(value.version)
  );
}

function isRead(value: unknown): value is NativeRuleImpactRead {
  return (
    object(value) &&
    text(value.rule_id, 200) &&
    safeInteger(value.rule_version) &&
    Array.isArray(value.items) &&
    value.items.every(isItem) &&
    text(value.notice, 4096)
  );
}

function selected(actor: NativeRuleImpactActor): { config: Config; target: Holding } {
  if (
    !text(actor.holdingId, 200) ||
    !text(actor.holdingSlug, 128) ||
    !/^[a-z0-9_-]+$/.test(actor.holdingSlug) ||
    !text(actor.companyId, 200) ||
    !text(actor.userId, 200) ||
    (actor.branchId != null && !text(actor.branchId, 200)) ||
    (actor.departmentId != null && !text(actor.departmentId, 200))
  )
    return unavailable('NATIVE_LEGAL_RULE_IMPACT_AUTHORITY_INVALID');
  const config = configuration();
  const target = config.holdings.find(
    (holding) =>
      holding.holdingId === actor.holdingId &&
      holding.holdingSlug === actor.holdingSlug &&
      (holding.companyId === undefined || holding.companyId === actor.companyId),
  );
  if (!target) return unavailable('NATIVE_LEGAL_RULE_IMPACT_AUTHORITY_INVALID');
  return { config, target };
}

/** Read the complete owner-local rule-impact projection. The registered Holding
 * is selected from the caller's context; an unknown actor never defaults to
 * another Holding. Purchase/enabled authority comes from composition and is
 * re-checked in the owner before any row is released. */
export async function nativeRuleImpactRead(
  actor: NativeRuleImpactActor,
  authority: NativeRuleImpactAuthority,
  id: string,
): Promise<NativeRuleImpactRead> {
  const { config, target } = selected(actor);
  if (
    !object(authority) ||
    typeof authority.companyEnabled !== 'boolean' ||
    typeof authority.rulePurchased !== 'boolean' ||
    !text(id, 200) ||
    !/^[A-Za-z0-9._:-]+$/.test(id)
  )
    return unavailable('NATIVE_LEGAL_RULE_IMPACT_REQUEST_INVALID');
  const url = new URL(
    `/api/v1/holdings/${encodeURIComponent(actor.holdingSlug)}/legal/rule-impact/${encodeURIComponent(id)}`,
    config.base,
  );
  const headers = new Headers({
    host: target.host,
    origin: target.origin,
    accept: 'application/json',
    'cache-control': 'no-cache',
    'x-erpx-context-company': actor.companyId,
    'x-erpx-context-user': actor.userId,
    'x-erpx-legal-rule-impact-enabled': String(authority.companyEnabled),
    'x-erpx-legal-rule-impact-purchased': String(authority.rulePurchased),
  });
  if (actor.branchId) headers.set('x-erpx-context-branch', actor.branchId);
  if (actor.departmentId) headers.set('x-erpx-context-department', actor.departmentId);
  let response: Response;
  try {
    response = await fetch(url, {
      method: 'GET',
      headers: Object.fromEntries(headers),
      cache: 'no-store',
      redirect: 'error',
      signal: AbortSignal.timeout(config.timeoutMs),
    });
  } catch {
    return unavailable();
  }
  if (response.status !== 200) {
    void response.body?.cancel().catch(() => undefined);
    const payload = (await response.json().catch((): unknown => null)) as
      | { error?: { code?: string; message?: string } }
      | null;
    if (response.status >= 400 && response.status < 500)
      throw new ApiError(
        response.status,
        text(payload?.error?.code, 100) ? String(payload!.error!.code) : 'NATIVE_LEGAL_RULE_IMPACT_FAILED',
        text(payload?.error?.message, 300)
          ? String(payload!.error!.message)
          : 'The native Legal read was rejected.',
      );
    return unavailable();
  }
  const payload = (await response.json().catch((): unknown => null)) as { data?: unknown } | null;
  if (!isRead(payload?.data)) return unavailable('NATIVE_LEGAL_RULE_IMPACT_RESPONSE_INVALID');
  return payload!.data!;
}
