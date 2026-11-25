import { describe, expect, test, vi, beforeEach, afterEach } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';

vi.mock('server-only', () => ({}));
import { ApiError } from '@erpx/server-runtime/public/http';
import {
  LANE6_CONFIG_INVALID,
  LANE6_RESPONSE_INVALID,
  LANE6_TARGET_UNAVAILABLE,
  LANE6_UNAVAILABLE,
  nativeCalendarAvailabilityLeaveDays,
  nativeCalendarAccessPeople,
  nativeCalendarPersonScope,
  type LaneHoldingScope,
} from '../../../packages/server-people_payroll/src/adapters/native-lane-6/client';
import { calendarAvailabilityPeoplePayrollFacts } from '../../../packages/server-people_payroll/src/adapters/calendar-availability-facts';
import { calendarIndexPeoplePayrollFacts } from '../../../packages/server-people_payroll/src/adapters/calendar-index-facts';
import { calendarAccessPeoplePayrollFacts } from '../../../packages/server-people_payroll/src/adapters/calendar-access-facts';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-people_payroll/src/${name}`, import.meta.url), 'utf8');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');

/** Verbatim retired `db.prepare` literal from calendar-availability-facts.ts. */
const RETIRED_SQL = `SELECT DISTINCT p.user_id,d.leave_date::text leave_date
    FROM leave_requests l JOIN hr_people p ON p.id=l.person_id AND p.company_id=l.company_id
    CROSS JOIN LATERAL (
      SELECT s.leave_date::date leave_date FROM leave_day_segments s WHERE s.request_id=l.id AND s.company_id=l.company_id
      UNION ALL
      SELECT generate_series(l.start_date::date,coalesce(l.end_date::date,l.start_date::date+l.days::int-1),interval '1 day')::date
      WHERE NOT EXISTS(SELECT 1 FROM leave_day_segments s WHERE s.request_id=l.id AND s.company_id=l.company_id)
    ) d
    WHERE l.company_id=? AND l.state='approved' AND p.user_id IN (\${facts.value2.map(() => "?").join(",")})
    AND d.leave_date BETWEEN ?::date AND ?::date`;

/** Verbatim retired `read1` literal from calendar-index-facts.ts. */
const RETIRED_PERSON_SCOPE_SQL = `SELECT 1 FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id WHERE p.id=? AND p.company_id=? AND \${inList("coalesce(e.department_id,e.branch_id,e.company_id)")} LIMIT 1`;

/** Verbatim retired `people` literal from calendar-access-facts.ts. */
const RETIRED_CALENDAR_PEOPLE_SQL = `SELECT * FROM (SELECT DISTINCT ON(p.id) p.id,p.legal_name name,p.user_id "userId",e.department_id "departmentId",e.branch_id "branchId",e.manager_person_id "managerId",e.status
  FROM hr_people p JOIN hr_engagements e ON e.person_id=p.id AND e.company_id=p.company_id
  WHERE p.company_id=? AND p.merged_into_id IS NULL AND \${inList("coalesce(e.department_id,e.branch_id,e.company_id)")}
  AND ((? AND (e.status!='ended' OR \${PP_RD29_LIFECYCLE_CASE_FOR_ENGAGEMENT_SQL})) OR (e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))) AND (?='' OR p.legal_name ILIKE '%'||?||'%')
  ORDER BY p.id,CASE WHEN e.status='active' THEN 0 ELSE 1 END,e.start_date DESC,e.id) x ORDER BY name,id LIMIT ? OFFSET ?`;

const ENV = 'ERPX_NATIVE_PEOPLE_PAYROLL_CONFIG';
const CONFIG = {
  privateBaseUrl: 'http://127.0.0.1:3198',
  timeoutMs: 10000,
  holdings: [
    { origin: 'https://acme.jesadakorn.com', host: 'acme.jesadakorn.com', holdingId: 'h001', holdingSlug: 'acme', companyId: 'company-1' },
    { origin: 'https://northstar.jesadakorn.com', host: 'northstar.jesadakorn.com', holdingId: 'h002', holdingSlug: 'northstar', companyId: 'company-2' },
    { origin: 'https://erpx-internal.jesadakorn.com', host: 'erpx-internal.jesadakorn.com', holdingId: 'h003', holdingSlug: 'erpx', companyId: 'company-3' },
  ],
};
const scopeOf = (index: number): LaneHoldingScope => {
  const h = CONFIG.holdings[index]!;
  return { holdingId: h.holdingId, holdingSlug: h.holdingSlug, companyId: h.companyId! };
};
const original = process.env[ENV];
beforeEach(() => {
  process.env[ENV] = JSON.stringify(CONFIG);
});
afterEach(() => {
  vi.unstubAllGlobals();
  if (original === undefined) delete process.env[ENV];
  else process.env[ENV] = original;
});

const reply = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });

async function expect503(promise: Promise<unknown>, code = LANE6_UNAVAILABLE) {
  try {
    await promise;
    throw new Error('expected ApiError');
  } catch (error) {
    expect(error).toBeInstanceOf(ApiError);
    expect((error as ApiError).status).toBe(503);
    expect((error as ApiError).code).toBe(code);
  }
}

describe('native lane 6 — People Payroll calendar availability leave reads', () => {
  test('SQL oracle: retired literal is byte-identical in the Rust owner and gone from TS', () => {
    expect(sha(RETIRED_SQL)).toBe('7abbf7e047b1e1bc94f62a116ad13b3922208aec4e6bd7324fbb04dbd83797b7');
    expect(RETIRED_SQL.match(/\bSELECT\b/g)).toHaveLength(4);
    const rs = src('adapters/native_lane_6/postgres.rs');
    expect(rs).toContain(RETIRED_SQL);
    expect(rs).toContain('l.company_id=$1');
    expect(rs).toContain('p.user_id IN ({users})');
    const ts = src('adapters/calendar-availability-facts.ts');
    expect(ts).not.toMatch(/\bSELECT\b/i);
    expect(ts).not.toMatch(/\bleave_day_segments\b/i);
    expect(ts).toContain('nativeCalendarAvailabilityLeaveDays');
    expect(src('adapters/native-lane-6/client.ts')).not.toMatch(/\bSELECT\b/i);
  });

  test('renders exact wire request and pins result shape + result oracle hash', async () => {
    const calls: { url: string; headers: Record<string, string>; body: unknown }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (url: string | URL, init: RequestInit) => {
      const headers: Record<string, string> = {};
      new Headers(init.headers).forEach((value, key) => { headers[key] = value; });
      calls.push({ url: String(url), headers, body: JSON.parse(String(init.body)) });
      return reply({ leaveDays: [
        { userId: 'u1', leaveDate: '2026-03-02' },
        { userId: 'u2', leaveDate: '2026-03-03' },
      ] });
    }));
    const result = await calendarAvailabilityPeoplePayrollFacts({} as never, scopeOf(0)).read7({
      value1: 'company-1',
      value2: ['u1', 'u2'],
      value3: '2026-03-01',
      value4: '2026-03-31',
    });
    expect(result).toEqual({
      results: [
        { user_id: 'u1', leave_date: '2026-03-02' },
        { user_id: 'u2', leave_date: '2026-03-03' },
      ],
    });
    expect(sha(JSON.stringify(result))).toBe('468fb4fa8954ededaca3282e8236ac5c0e9c4393087950eb054a704fe902a484');
    expect(calls).toHaveLength(1);
    expect(calls[0]?.url).toBe('http://127.0.0.1:3198/api/v1/holdings/acme/people-payroll/lane6/calendar-availability-leave');
    expect(calls[0]?.headers['host']).toBe('acme.jesadakorn.com');
    expect(calls[0]?.headers['origin']).toBe('https://acme.jesadakorn.com');
    expect(calls[0]?.headers['x-erpx-context-company']).toBe('company-1');
    expect(calls[0]?.body).toEqual({ userIds: ['u1', 'u2'], from: '2026-03-01', to: '2026-03-31' });
  });

  test('scopes every registered Holding by its exact (holdingId, holdingSlug) pair', async () => {
    const urls: string[] = [];
    const hosts: string[] = [];
    vi.stubGlobal('fetch', vi.fn(async (url: string | URL, init: RequestInit) => {
      urls.push(String(url));
      hosts.push(new Headers(init.headers).get('host') ?? '');
      return reply({ leaveDays: [] });
    }));
    for (let i = 0; i < CONFIG.holdings.length; i++) {
      const h = CONFIG.holdings[i]!;
      const result = await nativeCalendarAvailabilityLeaveDays(
        scopeOf(i), h.companyId!, ['u1'], '2026-03-01', '2026-03-02',
      );
      expect(result).toEqual({ results: [] });
    }
    expect(urls).toEqual(CONFIG.holdings.map(h =>
      `http://127.0.0.1:3198/api/v1/holdings/${h.holdingSlug}/people-payroll/lane6/calendar-availability-leave`));
    expect(hosts).toEqual(CONFIG.holdings.map(h => h.host));
  });

  test('never falls back to the first Holding for an unknown scope', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('must not be called'); }));
    await expect503(nativeCalendarAvailabilityLeaveDays(
      { holdingId: 'h999', holdingSlug: 'acme', companyId: 'company-1' },
      'company-1', ['u1'], '2026-03-01', '2026-03-02',
    ), LANE6_TARGET_UNAVAILABLE);
    await expect503(nativeCalendarAvailabilityLeaveDays(
      { holdingId: 'h001', holdingSlug: 'northstar', companyId: 'company-1' },
      'company-1', ['u1'], '2026-03-01', '2026-03-02',
    ), LANE6_TARGET_UNAVAILABLE);
    await expect503(nativeCalendarAvailabilityLeaveDays(
      { holdingId: 'h003', holdingSlug: 'erpx', companyId: 'company-3' },
      'company-1', ['u1'], '2026-03-01', '2026-03-02',
    ), LANE6_TARGET_UNAVAILABLE);
    expect(fetch).not.toHaveBeenCalled();
  });

  test('unscoped legacy caller selects the one Holding registered for its Company', async () => {
    const urls: string[] = [];
    vi.stubGlobal('fetch', vi.fn(async (url: string | URL) => {
      urls.push(String(url));
      return reply({ leaveDays: [] });
    }));
    await nativeCalendarAvailabilityLeaveDays(undefined, 'company-3', ['u1'], '2026-03-01', '2026-03-02');
    expect(urls).toEqual(['http://127.0.0.1:3198/api/v1/holdings/erpx/people-payroll/lane6/calendar-availability-leave']);
  });

  test('fails closed on duplicate or malformed holding configuration', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('must not be called'); }));
    const bad = JSON.stringify({ ...CONFIG, holdings: [CONFIG.holdings[0]!, CONFIG.holdings[0]!] });
    process.env[ENV] = bad;
    await expect503(nativeCalendarAvailabilityLeaveDays(undefined, 'company-1', ['u1'], '2026-03-01', '2026-03-02'), LANE6_CONFIG_INVALID);
    process.env[ENV] = JSON.stringify({ ...CONFIG, holdings: [{ ...CONFIG.holdings[0]!, host: 'wrong.example.com' }] });
    await expect503(nativeCalendarAvailabilityLeaveDays(undefined, 'company-1', ['u1'], '2026-03-01', '2026-03-02'), LANE6_CONFIG_INVALID);
    process.env[ENV] = JSON.stringify(CONFIG);
  });

  test('direct client keeps company scope in a header, never in the body', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => reply({ leaveDays: [] })));
    await expect(nativeCalendarAvailabilityLeaveDays(undefined, 'company-1', ['u1'], '2026-03-01', '2026-03-02')).resolves.toEqual({ results: [] });
    const [url, init] = vi.mocked(fetch).mock.calls[0] as unknown as [URL, RequestInit];
    expect(new URL(String(url)).pathname).toContain('/api/v1/holdings/acme/people-payroll/lane6/');
    expect(new Headers(init.headers).get('x-erpx-context-company')).toBe('company-1');
    expect(String(init.body)).not.toContain('company-1');
  });

  test.each([
    ['non-200', () => reply({ error: { code: 'X', message: 'no' } }, 503), LANE6_UNAVAILABLE],
    ['bad JSON', () => new Response('{', { status: 200 }), LANE6_RESPONSE_INVALID],
    ['extra envelope key', () => reply({ leaveDays: [], extra: true }), LANE6_RESPONSE_INVALID],
    ['missing list', () => reply({ days: [] }), LANE6_RESPONSE_INVALID],
    ['unknown row keys', () => reply({ leaveDays: [{ userId: 'u1', leaveDate: '2026-03-02', x: 1 }] }), LANE6_RESPONSE_INVALID],
    ['non-date row', () => reply({ leaveDays: [{ userId: 'u1', leaveDate: '2026/03/02' }] }), LANE6_RESPONSE_INVALID],
    ['non-string user', () => reply({ leaveDays: [{ userId: 1, leaveDate: '2026-03-02' }] }), LANE6_RESPONSE_INVALID],
  ])('fails closed on %s', async (_name, build, code) => {
    vi.stubGlobal('fetch', vi.fn(async () => build()));
    await expect503(nativeCalendarAvailabilityLeaveDays(undefined, 'company-1', ['u1'], '2026-03-01', '2026-03-02'), code);
  });

  test('fails closed when the native transport throws', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('ECONNREFUSED'); }));
    await expect503(nativeCalendarAvailabilityLeaveDays(undefined, 'company-1', ['u1'], '2026-03-01', '2026-03-02'));
  });

  test('fails closed on missing, malformed or reserved-port configuration', async () => {
    delete process.env[ENV];
    await expect503(nativeCalendarAvailabilityLeaveDays(undefined, 'company-1', ['u1'], '2026-03-01', '2026-03-02'), LANE6_CONFIG_INVALID);
    process.env[ENV] = '{';
    await expect503(nativeCalendarAvailabilityLeaveDays(undefined, 'company-1', ['u1'], '2026-03-01', '2026-03-02'), LANE6_CONFIG_INVALID);
    process.env[ENV] = JSON.stringify({ ...CONFIG, privateBaseUrl: 'http://127.0.0.1:3100' });
    await expect503(nativeCalendarAvailabilityLeaveDays(undefined, 'company-1', ['u1'], '2026-03-01', '2026-03-02'), LANE6_CONFIG_INVALID);
    process.env[ENV] = JSON.stringify(CONFIG);
  });

  test('rejects unbounded or malformed caller input before any fetch', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('must not be called'); }));
    await expect503(nativeCalendarAvailabilityLeaveDays(undefined, '', ['u1'], '2026-03-01', '2026-03-02'), LANE6_RESPONSE_INVALID);
    await expect503(nativeCalendarAvailabilityLeaveDays(undefined, 'company-1', [], '2026-03-01', '2026-03-02'), LANE6_RESPONSE_INVALID);
    await expect503(nativeCalendarAvailabilityLeaveDays(undefined, 'company-1', ['u1'], '2026-02-30', '2026-03-02'), LANE6_RESPONSE_INVALID);
    expect(fetch).not.toHaveBeenCalled();
  });

  test('person-scope SQL oracle: retired literal is byte-identical in Rust and gone from TS', () => {
    expect(sha(RETIRED_PERSON_SCOPE_SQL)).toBe('d60b2b715e9da1d1d2f1ff3222c6749c856e875f7f097161443667049c696702');
    const rs = src('adapters/native_lane_6/postgres.rs');
    expect(rs).toContain(RETIRED_PERSON_SCOPE_SQL);
    expect(rs).toContain('p.id=$1 AND p.company_id=$2');
    expect(rs).toContain('jsonb_array_elements_text($3::jsonb)');
    const ts = src('adapters/calendar-index-facts.ts');
    expect(ts).not.toMatch(/\bSELECT\b/i);
    expect(ts).toContain('nativeCalendarPersonScope');
  });

  test('person-scope client sends the exact bounded request and pins the boolean reply', async () => {
    const calls: { url: string; headers: Record<string, string>; body: unknown }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (url: string | URL, init: RequestInit) => {
      const headers: Record<string, string> = {};
      new Headers(init.headers).forEach((value, key) => { headers[key] = value; });
      calls.push({ url: String(url), headers, body: JSON.parse(String(init.body)) });
      return reply({ present: true });
    }));
    const present = await calendarIndexPeoplePayrollFacts({} as never, scopeOf(1)).read1({
      value1: 'person-7',
      value2: 'company-2',
      value3: '["branch-1","department-2"]',
    });
    expect(present).toEqual({ present: 1 });
    expect(calls).toHaveLength(1);
    expect(calls[0]?.url).toBe('http://127.0.0.1:3198/api/v1/holdings/northstar/people-payroll/lane6/calendar-person-scope');
    expect(calls[0]?.headers['host']).toBe('northstar.jesadakorn.com');
    expect(calls[0]?.headers['x-erpx-context-company']).toBe('company-2');
    expect(calls[0]?.body).toEqual({ personId: 'person-7', scopeIds: '["branch-1","department-2"]' });
  });

  test('person-scope client pins the absence reply', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => reply({ present: false })));
    await expect(nativeCalendarPersonScope(scopeOf(2), 'company-3', 'person-9', '["company-3"]')).resolves.toBe(false);
  });

  test.each([
    ['non-boolean', () => reply({ present: 'yes' }), LANE6_RESPONSE_INVALID],
    ['extra key', () => reply({ present: true, extra: 1 }), LANE6_RESPONSE_INVALID],
    ['missing key', () => reply({}), LANE6_RESPONSE_INVALID],
    ['non-200', () => reply({}, 503), LANE6_UNAVAILABLE],
  ])('person-scope fails closed on %s', async (_name, build, code) => {
    vi.stubGlobal('fetch', vi.fn(async () => build()));
    await expect503(nativeCalendarPersonScope(undefined, 'company-1', 'person-1', '["company-1"]'), code);
  });

  test('person-scope rejects malformed scope or unknown target before any fetch', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('must not be called'); }));
    await expect503(nativeCalendarPersonScope(undefined, 'company-1', 'person-1', 'not-json'), LANE6_RESPONSE_INVALID);
    await expect503(nativeCalendarPersonScope(undefined, 'company-1', 'person-1', '[]'), LANE6_RESPONSE_INVALID);
    await expect503(nativeCalendarPersonScope(undefined, 'company-1', 'person-1', '["bad id"]'), LANE6_RESPONSE_INVALID);
    await expect503(nativeCalendarPersonScope(
      { holdingId: 'h999', holdingSlug: 'acme', companyId: 'company-1' },
      'company-1', 'person-1', '["company-1"]',
    ), LANE6_TARGET_UNAVAILABLE);
    expect(fetch).not.toHaveBeenCalled();
  });

  test('calendar-people SQL oracle: retired literal is byte-identical in Rust and gone from TS', () => {
    expect(sha(RETIRED_CALENDAR_PEOPLE_SQL)).toBe('94666e17fa2d64329b3e7a2f94ddba1687c10d7f8ce5a1c53c11fc4e8d76abbb');
    // Two literal SELECTs here; the third comes from the owner-local
    // PP_RD29 lifecycle fragment inlined byte-identically in Rust.
    expect(RETIRED_CALENDAR_PEOPLE_SQL.match(/\bSELECT\b/g)).toHaveLength(2);
    expect(RETIRED_CALENDAR_PEOPLE_SQL).toContain('${PP_RD29_LIFECYCLE_CASE_FOR_ENGAGEMENT_SQL}');
    const rs = src('adapters/native_lane_6/postgres.rs');
    expect(rs).toContain(RETIRED_CALENDAR_PEOPLE_SQL);
    expect(rs).toContain('p.company_id=$1');
    expect(rs).toContain('jsonb_array_elements_text($2::jsonb)');
    expect(rs).toContain('LIMIT $6 OFFSET $7');
    const ts = src('adapters/calendar-access-facts.ts');
    expect(ts).not.toMatch(/\bSELECT\b/i);
    expect(ts).toContain('nativeCalendarAccessPeople');
  });

  test('calendar-people client sends the exact bounded request and pins the response shape', async () => {
    const calls: { url: string; headers: Record<string, string>; body: unknown }[] = [];
    vi.stubGlobal('fetch', vi.fn(async (url: string | URL, init: RequestInit) => {
      const headers: Record<string, string> = {};
      new Headers(init.headers).forEach((value, key) => { headers[key] = value; });
      calls.push({ url: String(url), headers, body: JSON.parse(String(init.body)) });
      return reply({ people: [{
        id: 'p1', name: 'Alice', userId: 'u1', departmentId: null,
        branchId: 'b1', managerId: null, status: 'active',
      }] });
    }));
    const people = await calendarAccessPeoplePayrollFacts({} as never, scopeOf(0)).people({
      companyId: 'company-1',
      peopleIds: ['branch-1', 'department-2'],
      lifecycleReader: true,
      search: 'Alice',
      take: 25,
      cursor: 0,
    });
    expect(people).toEqual([{
      id: 'p1', name: 'Alice', userId: 'u1', departmentId: null,
      branchId: 'b1', managerId: null, status: 'active',
    }]);
    expect(calls).toHaveLength(1);
    expect(calls[0]?.url).toBe('http://127.0.0.1:3198/api/v1/holdings/acme/people-payroll/lane6/calendar-access-people');
    expect(calls[0]?.headers['host']).toBe('acme.jesadakorn.com');
    expect(calls[0]?.headers['x-erpx-context-company']).toBe('company-1');
    expect(calls[0]?.body).toEqual({
      peopleIds: ['branch-1', 'department-2'], lifecycleReader: true,
      search: 'Alice', take: 25, cursor: 0,
    });
  });

  test.each([
    ['extra envelope key', () => reply({ people: [], extra: 1 }), LANE6_RESPONSE_INVALID],
    ['missing list', () => reply({ rows: [] }), LANE6_RESPONSE_INVALID],
    ['unknown row keys', () => reply({ people: [{ id: 'p1', name: 'A', userId: null, departmentId: null, branchId: null, managerId: null, status: 'active', x: 1 }] }), LANE6_RESPONSE_INVALID],
    ['non-nullable user', () => reply({ people: [{ id: 'p1', name: 'A', userId: 1, departmentId: null, branchId: null, managerId: null, status: 'active' }] }), LANE6_RESPONSE_INVALID],
    ['non-200', () => reply({}, 503), LANE6_UNAVAILABLE],
  ])('calendar-people fails closed on %s', async (_name, build, code) => {
    vi.stubGlobal('fetch', vi.fn(async () => build()));
    await expect503(nativeCalendarAccessPeople(undefined, 'company-1', ['branch-1'], true, '', 25, 0), code);
  });

  test('calendar-people rejects unbounded input before any fetch', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('must not be called'); }));
    await expect503(nativeCalendarAccessPeople(undefined, 'company-1', [], true, '', 25, 0), LANE6_RESPONSE_INVALID);
    await expect503(nativeCalendarAccessPeople(undefined, 'company-1', ['branch-1'], true, '', 0, 0), LANE6_RESPONSE_INVALID);
    await expect503(nativeCalendarAccessPeople(undefined, 'company-1', ['branch-1'], true, '', 25, -1), LANE6_RESPONSE_INVALID);
    expect(fetch).not.toHaveBeenCalled();
  });

  test('calendar-people enforces byte bounds before any fetch', async () => {
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('must not be called'); }));
    // 201 ASCII bytes, and a multibyte search over 200 bytes: both rejected by the Rust 200-byte bound.
    await expect503(nativeCalendarAccessPeople(undefined, 'company-1', ['branch-1'], true, 'x'.repeat(201), 25, 0), LANE6_RESPONSE_INVALID);
    await expect503(nativeCalendarAccessPeople(undefined, 'company-1', ['branch-1'], true, 'ก'.repeat(70), 25, 0), LANE6_RESPONSE_INVALID);
    // 200 ids of 200 chars each would exceed the axum 8 KiB body limit.
    const ids = Array.from({ length: 200 }, (_, index) => `${'i'.repeat(190)}${index}`);
    await expect503(nativeCalendarAccessPeople(undefined, 'company-1', ids, true, '', 25, 0), LANE6_RESPONSE_INVALID);
    expect(fetch).not.toHaveBeenCalled();
  });
});
