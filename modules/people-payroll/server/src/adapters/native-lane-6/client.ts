import 'server-only';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import { ApiError } from '@erpx/platform-runtime-server/public/http';

/**
 * Strict private transport for lane 6's native calendar-availability leave-day
 * read. The axum owner executes the retired company-scoped query and owns the
 * projection; this bridge carries no SQL and has no fallback reader.
 *
 * Every registered Holding is configured explicitly (1..64). A read selects
 * its target by the caller's exact (holdingId, holdingSlug) pair: there is no
 * first-holding default and an unknown target fails closed with a 503. The
 * optional `companyId` on a Holding lets the still-unscoped legacy caller
 * resolve its one explicitly registered Holding; that is also exact, never a
 * positional default.
 */
export type LaneHoldingScope = Pick<WorkContext, 'holdingId' | 'holdingSlug' | 'companyId'>;

type Holding = Readonly<{
  origin: string;
  host: string;
  holdingId: string;
  holdingSlug: string;
  companyId?: string;
}>;
type Config = Readonly<{ base: string; timeoutMs: number; holdings: readonly Holding[] }>;

export const LANE6_READ = 'people_payroll_lane6_calendar_availability_leave';
export const LANE6_UNAVAILABLE = 'PEOPLE_PAYROLL_LANE6_READ_UNAVAILABLE';
export const LANE6_CONFIG_INVALID = 'PEOPLE_PAYROLL_LANE6_CONFIG_INVALID';
export const LANE6_TARGET_UNAVAILABLE = 'PEOPLE_PAYROLL_LANE6_TARGET_UNAVAILABLE';
export const LANE6_RESPONSE_INVALID = 'PEOPLE_PAYROLL_LANE6_RESPONSE_INVALID';

const ENV = 'ERPX_NATIVE_PEOPLE_PAYROLL_CONFIG';
const RESERVED_PORTS = new Set([3000, 3001, 3100, 3101, 3103, 5432, 5433, 5435, 8080]);
const MAX_USERS = 100;
const DATE = /^\d{4}-\d{2}-\d{2}$/;

const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

const unavailable = (
  code = LANE6_UNAVAILABLE,
  message = 'The native People Payroll availability service is unavailable.',
): never => {
  throw new ApiError(503, code, message);
};

const text = (value: unknown, max = 8192): value is string =>
  typeof value === 'string' && new TextEncoder().encode(value).length <= max;

const id = (value: unknown, max = 200): value is string =>
  text(value, max) && /^[A-Za-z0-9_.:-]+$/.test(value);

const slug = (value: unknown): value is string =>
  id(value, 128) && /^[a-z0-9_-]+$/.test(value);

const date = (value: unknown): value is string => {
  if (typeof value !== 'string' || !DATE.test(value)) return false;
  const year = Number(value.slice(0, 4));
  const month = Number(value.slice(5, 7));
  const day = Number(value.slice(8, 10));
  const leap = year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0);
  const days = [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
  return month >= 1 && month <= 12 && day >= 1 && day <= days[month - 1]!;
};

function configuration(): Config {
  const raw = process.env[ENV];
  if (!raw) return unavailable(LANE6_CONFIG_INVALID);
  let value: unknown;
  try {
    value = JSON.parse(raw);
  } catch {
    return unavailable(LANE6_CONFIG_INVALID);
  }
  if (
    !record(value) ||
    typeof value.privateBaseUrl !== 'string' ||
    typeof value.timeoutMs !== 'number' ||
    !Number.isSafeInteger(value.timeoutMs) ||
    value.timeoutMs < 250 ||
    value.timeoutMs > 30_000 ||
    !Array.isArray(value.holdings) ||
    value.holdings.length < 1 ||
    value.holdings.length > 64
  )
    return unavailable(LANE6_CONFIG_INVALID);
  let base: URL;
  try {
    base = new URL(value.privateBaseUrl);
  } catch {
    return unavailable(LANE6_CONFIG_INVALID);
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
    return unavailable(LANE6_CONFIG_INVALID);

  const holdings: Holding[] = [];
  const targetKeys = new Set<string>();
  const scopeKeys = new Set<string>();
  const companyKeys = new Set<string>();
  for (const item of value.holdings) {
    if (
      !record(item) ||
      !text(item.origin, 512) ||
      !id(item.holdingId) ||
      !slug(item.holdingSlug) ||
      !text(item.host, 512) ||
      (item.companyId !== undefined && !id(item.companyId))
    )
      return unavailable(LANE6_CONFIG_INVALID);
    let origin: URL;
    try {
      origin = new URL(item.origin);
    } catch {
      return unavailable(LANE6_CONFIG_INVALID);
    }
    if (
      origin.protocol !== 'https:' ||
      origin.origin !== item.origin ||
      origin.username ||
      origin.password ||
      item.host !== origin.host
    )
      return unavailable(LANE6_CONFIG_INVALID);
    const targetKey = `${item.host}\u0000${item.holdingSlug}`;
    const scopeKey = `${item.holdingId}\u0000${item.holdingSlug}`;
    if (targetKeys.has(targetKey) || scopeKeys.has(scopeKey))
      return unavailable(LANE6_CONFIG_INVALID);
    targetKeys.add(targetKey);
    scopeKeys.add(scopeKey);
    if (item.companyId !== undefined) {
      if (companyKeys.has(item.companyId)) return unavailable(LANE6_CONFIG_INVALID);
      companyKeys.add(item.companyId);
    }
    holdings.push({
      origin: origin.origin,
      host: item.host,
      holdingId: item.holdingId,
      holdingSlug: item.holdingSlug,
      ...(item.companyId === undefined ? {} : { companyId: item.companyId }),
    });
  }
  return { base: base.origin, timeoutMs: value.timeoutMs, holdings };
}

/** Exact registered target. No first-holding fallback and no caller SQL. */
function targetFor(config: Config, scope: LaneHoldingScope | undefined, companyId: string): Holding {
  if (scope !== undefined) {
    if (
      !id(scope.holdingId) ||
      !slug(scope.holdingSlug) ||
      !id(scope.companyId) ||
      scope.companyId !== companyId
    )
      return unavailable(LANE6_TARGET_UNAVAILABLE, 'Unknown native Holding target.');
    const found = config.holdings.filter(
      item => item.holdingId === scope.holdingId && item.holdingSlug === scope.holdingSlug,
    );
    if (found.length !== 1)
      return unavailable(LANE6_TARGET_UNAVAILABLE, 'Unknown native Holding target.');
    return found[0]!;
  }
  // Legacy compatibility for the caller that has no WorkContext: the single
  // Holding explicitly registered for this Company. Never holdings[0].
  const found = config.holdings.filter(item => item.companyId === companyId);
  if (found.length !== 1)
    return unavailable(LANE6_TARGET_UNAVAILABLE, 'Unknown native Holding target.');
  return found[0]!;
}

/**
 * Owner-local Company person-scope probe. The retired `read1` existence query
 * now runs in Rust against the caller's exact registered Holding; this bridge
 * carries no SQL and has no fallback reader. Only a boolean is returned.
 */
export async function nativeCalendarPersonScope(
  scope: LaneHoldingScope | undefined,
  companyId: string,
  personId: string,
  scopeIdsJson: string,
): Promise<boolean> {
  if (!id(companyId) || !id(personId) || scopeIdsJson.length > 16 * 1024)
    return unavailable(LANE6_RESPONSE_INVALID, 'Invalid calendar person scope request.');
  let ids: unknown;
  try {
    ids = JSON.parse(scopeIdsJson);
  } catch {
    return unavailable(LANE6_RESPONSE_INVALID, 'Invalid calendar person scope request.');
  }
  if (!Array.isArray(ids) || ids.length < 1 || ids.length > 200 || ids.some(value => !id(value)))
    return unavailable(LANE6_RESPONSE_INVALID, 'Invalid calendar person scope request.');
  const config = configuration();
  const target = targetFor(config, scope, companyId);
  const body = JSON.stringify({ personId, scopeIds: scopeIdsJson });
  if (new TextEncoder().encode(body).length > 8 * 1024)
    return unavailable(LANE6_RESPONSE_INVALID, 'Invalid calendar person scope request.');
  let response: globalThis.Response;
  try {
    response = await fetch(
      `${config.base}/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/people-payroll/lane6/calendar-person-scope`,
      {
        method: 'POST',
        headers: new Headers({
          host: target.host,
          origin: target.origin,
          accept: 'application/json',
          'content-type': 'application/json',
          'cache-control': 'no-cache',
          'x-erpx-context-company': companyId,
        }),
        body,
        cache: 'no-store',
        redirect: 'error',
        signal: AbortSignal.timeout(config.timeoutMs),
      },
    );
  } catch {
    return unavailable();
  }
  if (response.status !== 200) {
    void response.body?.cancel().catch(() => undefined);
    return unavailable();
  }
  let value: unknown;
  try {
    value = await response.json();
  } catch {
    return unavailable(LANE6_RESPONSE_INVALID);
  }
  if (!record(value) || Object.keys(value).length !== 1 || typeof value.present !== 'boolean')
    return unavailable(LANE6_RESPONSE_INVALID);
  return value.present;
}

export type NativeCalendarPerson = Readonly<{
  id: string;
  name: string;
  userId: string | null;
  departmentId: string | null;
  branchId: string | null;
  managerId: string | null;
  status: string;
}>;

/** Owner-local Company calendar-access people list. The retired `people`
 * query now runs in Rust against the caller's exact registered Holding; this
 * bridge carries no SQL and has no fallback reader. */
export async function nativeCalendarAccessPeople(
  scope: LaneHoldingScope | undefined,
  companyId: string,
  peopleIds: readonly string[],
  lifecycleReader: boolean,
  search: string,
  take: number,
  cursor: number,
): Promise<NativeCalendarPerson[]> {
  if (
    !id(companyId) ||
    peopleIds.length < 1 ||
    peopleIds.length > 200 ||
    peopleIds.some(value => !id(value)) ||
    typeof lifecycleReader !== 'boolean' ||
    !text(search, 200) ||
    !Number.isSafeInteger(take) || take < 1 || take > 1000 ||
    !Number.isSafeInteger(cursor) || cursor < 0
  )
    return unavailable(LANE6_RESPONSE_INVALID, 'Invalid calendar people request.');
  const config = configuration();
  const target = targetFor(config, scope, companyId);
  const body = JSON.stringify({ peopleIds, lifecycleReader, search, take, cursor });
  if (new TextEncoder().encode(body).length > 8 * 1024)
    return unavailable(LANE6_RESPONSE_INVALID, 'Invalid calendar people request.');
  let response: globalThis.Response;
  try {
    response = await fetch(
      `${config.base}/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/people-payroll/lane6/calendar-access-people`,
      {
        method: 'POST',
        headers: new Headers({
          host: target.host,
          origin: target.origin,
          accept: 'application/json',
          'content-type': 'application/json',
          'cache-control': 'no-cache',
          'x-erpx-context-company': companyId,
        }),
        body,
        cache: 'no-store',
        redirect: 'error',
        signal: AbortSignal.timeout(config.timeoutMs),
      },
    );
  } catch {
    return unavailable();
  }
  if (response.status !== 200) {
    void response.body?.cancel().catch(() => undefined);
    return unavailable();
  }
  let value: unknown;
  try {
    value = await response.json();
  } catch {
    return unavailable(LANE6_RESPONSE_INVALID);
  }
  if (!record(value) || Object.keys(value).length !== 1 || !Array.isArray(value.people))
    return unavailable(LANE6_RESPONSE_INVALID);
  const people: NativeCalendarPerson[] = [];
  const nullable = (v: unknown): v is string | null => v === null || text(v, 200);
  for (const row of value.people) {
    if (
      !record(row) ||
      Object.keys(row).length !== 7 ||
      !id(row.id) ||
      !text(row.name, 320) ||
      !nullable(row.userId) ||
      !nullable(row.departmentId) ||
      !nullable(row.branchId) ||
      !nullable(row.managerId) ||
      !text(row.status, 64)
    )
      return unavailable(LANE6_RESPONSE_INVALID);
    people.push({
      id: row.id,
      name: row.name,
      userId: row.userId,
      departmentId: row.departmentId,
      branchId: row.branchId,
      managerId: row.managerId,
      status: row.status,
    });
  }
  return people;
}

export type NativeLeaveDay = Readonly<{ user_id: string; leave_date: string }>;

/**
 * Native-backed replacement for the retired `read7` SQL reader. The Company
 * travels in a unique trusted header; the body carries only bounded identities
 * and dates. The return shape is the exact `CalendarReadResult` the caller
 * expects, so no caller change is needed and no SQL remains in TypeScript.
 */
export async function nativeCalendarAvailabilityLeaveDays(
  scope: LaneHoldingScope | undefined,
  companyId: string,
  userIds: readonly string[],
  from: string,
  to: string,
): Promise<{ results: NativeLeaveDay[] }> {
  if (
    !id(companyId) ||
    userIds.length < 1 ||
    userIds.length > MAX_USERS ||
    userIds.some(value => !id(value)) ||
    !date(from) ||
    !date(to) ||
    from > to
  )
    return unavailable(LANE6_RESPONSE_INVALID, 'Invalid calendar availability request.');
  const config = configuration();
  const target = targetFor(config, scope, companyId);
  let response: globalThis.Response;
  try {
    response = await fetch(
      `${config.base}/api/v1/holdings/${encodeURIComponent(target.holdingSlug)}/people-payroll/lane6/calendar-availability-leave`,
      {
        method: 'POST',
        headers: new Headers({
          host: target.host,
          origin: target.origin,
          accept: 'application/json',
          'content-type': 'application/json',
          'cache-control': 'no-cache',
          'x-erpx-context-company': companyId,
        }),
        body: JSON.stringify({ userIds, from, to }),
        cache: 'no-store',
        redirect: 'error',
        signal: AbortSignal.timeout(config.timeoutMs),
      },
    );
  } catch {
    return unavailable();
  }
  if (response.status !== 200) {
    void response.body?.cancel().catch(() => undefined);
    return unavailable();
  }
  let value: unknown;
  try {
    value = await response.json();
  } catch {
    return unavailable(LANE6_RESPONSE_INVALID);
  }
  if (
    !record(value) ||
    Object.keys(value).length !== 1 ||
    !Array.isArray(value.leaveDays) ||
    value.leaveDays.length > MAX_USERS * 62
  )
    return unavailable(LANE6_RESPONSE_INVALID);
  const results: NativeLeaveDay[] = [];
  for (const row of value.leaveDays) {
    if (!record(row) || Object.keys(row).length !== 2 || !id(row.userId) || !date(row.leaveDate))
      return unavailable(LANE6_RESPONSE_INVALID);
    results.push({ user_id: row.userId, leave_date: row.leaveDate });
  }
  return { results };
}
