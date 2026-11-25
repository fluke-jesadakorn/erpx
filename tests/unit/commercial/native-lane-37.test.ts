import { afterEach, describe, expect, it, vi } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));

import {
  nativeLane37Configured,
  readNativeSeatHistory,
  type Lane37Actor,
  type NativeSeatHistory,
} from '../../../packages/server-commercial/src/adapters/native-lane-37/client';

const ENV = 'ERPX_NATIVE_LANE37_COMMERCIAL_CONFIG';
/** Exact development env value for the three registered Holding hosts. */
const DEV_ENV_JSON =
  '{"privateBaseUrl":"http://127.0.0.1:3198","timeoutMs":10000,"holdings":[{"origin":"https://acme.jesadakorn.com","host":"acme.jesadakorn.com","holdingId":"h001","holdingSlug":"acme","companyId":"company-acme"},{"origin":"https://northstar.jesadakorn.com","host":"northstar.jesadakorn.com","holdingId":"h002","holdingSlug":"northstar","companyId":"company-northstar"},{"origin":"https://erpx-internal.jesadakorn.com","host":"erpx-internal.jesadakorn.com","holdingId":"h003","holdingSlug":"erpx","companyId":"company-erpx"}]}';
const ACME: Lane37Actor = { holdingId: 'h001', holdingSlug: 'acme' };
const NORTHSTAR: Lane37Actor = { holdingId: 'h002', holdingSlug: 'northstar' };
const SQL = [
  'SELECT started_at FROM billing_seat_history_coverage WHERE singleton=1',
  'SELECT event_id,assignment_id,captured_at,present,state::text state_json FROM billing_seat_assignment_events WHERE captured_at<? ORDER BY assignment_id,captured_at,event_id',
  'SELECT event_id,company_id,user_id,captured_at,present,state::text state_json FROM billing_seat_member_events WHERE captured_at<? ORDER BY company_id,user_id,captured_at,event_id',
  'SELECT event_id,company_id,role_id,captured_at,present,state::text state_json FROM billing_seat_role_events WHERE captured_at<? ORDER BY company_id,role_id,captured_at,event_id',
];
const SQL_ORACLE = '09a6e67ece0a798f73826c179f846944675d3acd868682485afac2470385f661';
const config = (overrides: Record<string, unknown> = {}) => {
  const value = JSON.parse(DEV_ENV_JSON) as Record<string, unknown>;
  return JSON.stringify({ ...value, ...overrides });
};
const result: NativeSeatHistory = {
  rows: {
    historyStartedAt: 100,
    assignments: [
      {
        eventId: 1,
        assignmentId: 'a1',
        capturedAt: 101,
        present: true,
        stateJson: JSON.stringify({
          userId: 'human-1',
          companyId: 'company-a',
          role: 'employee',
          source: 'manual',
          parentId: null,
          validFrom: 100,
          validUntil: 200,
          revokedAt: null,
        }),
      },
    ],
    members: [],
    roles: [
      {
        eventId: 2,
        companyId: '*',
        roleId: 'employee',
        capturedAt: 99,
        present: true,
        stateJson: JSON.stringify({ status: 'active', publishedRevision: 1 }),
      },
    ],
  },
  meter: { coverage: 'measured', users: [{ userId: 'human-1', intervals: [[100, 200]] }] },
};
const ok = (value: unknown) =>
  new Response(JSON.stringify(value), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });

afterEach(() => {
  delete process.env[ENV];
  vi.unstubAllGlobals();
});

describe('native lane 37 Commercial seat-history transport', () => {
  it('pins the rendered SQL oracle and deletes the retired literals from TypeScript', () => {
    const hash = createHash('sha256').update(JSON.stringify(SQL)).digest('hex');
    expect(hash).toBe(SQL_ORACLE);
    const root = process.cwd();
    const rust = readFileSync(
      resolve(root, 'packages/server-commercial/src/adapters/native_lane_37/domain.rs'),
      'utf8',
    );
    const ts = readFileSync(
      resolve(root, 'packages/server-commercial/src/adapters/seats.ts'),
      'utf8',
    );
    const client = readFileSync(
      resolve(root, 'packages/server-commercial/src/adapters/native-lane-37/client.ts'),
      'utf8',
    );
    for (const fragment of [
      'billing_seat_history_coverage WHERE singleton=1',
      'billing_seat_assignment_events WHERE captured_at<$1',
      'billing_seat_member_events WHERE captured_at<$1',
      'billing_seat_role_events WHERE captured_at<$1',
      'ORDER BY assignment_id,captured_at,event_id',
      'ORDER BY company_id,user_id,captured_at,event_id',
      'ORDER BY company_id,role_id,captured_at,event_id',
    ])
      expect(rust).toContain(fragment);
    for (const sql of SQL) expect(ts).not.toContain(sql);
    expect(ts).not.toContain('seatHistorySql');
    expect(ts).not.toContain('coverageProbe');
    expect(ts.match(/SELECT\s/g) ?? []).toHaveLength(0);
    expect(ts.match(/readNativeSeatHistory\(/g) ?? []).toHaveLength(2);
    // Cutover KPI pins: all 3 registered holdings are selectable; no default.
    expect(client).not.toMatch(/holdings\[0\]/);
    expect(client).toContain('holding.holdingId === actor.holdingId');
    expect(client).toContain('holding.holdingSlug === actor.holdingSlug');
    expect(client).not.toContain('database.prepare');
  });

  it('pins the exact three-holding development configuration', () => {
    const value = JSON.parse(DEV_ENV_JSON) as {
      privateBaseUrl: string;
      timeoutMs: number;
      holdings: { origin: string; host: string; holdingId: string; holdingSlug: string; companyId: string }[];
    };
    expect(value.privateBaseUrl).toBe('http://127.0.0.1:3198');
    expect(value.timeoutMs).toBe(10000);
    expect(value.holdings).toHaveLength(3);
    expect(value.holdings.map((h) => h.holdingSlug)).toEqual(['acme', 'northstar', 'erpx']);
    expect(value.holdings.map((h) => h.holdingId)).toEqual(['h001', 'h002', 'h003']);
    expect(value.holdings.map((h) => h.host)).toEqual([
      'acme.jesadakorn.com',
      'northstar.jesadakorn.com',
      'erpx-internal.jesadakorn.com',
    ]);
  });

  it('selects the caller Holding by holdingId + holdingSlug with isolated headers', async () => {
    process.env[ENV] = config();
    const fetch = vi.fn().mockImplementation(async () => ok(result));
    vi.stubGlobal('fetch', fetch);
    expect(nativeLane37Configured()).toBe(true);
    expect(await readNativeSeatHistory(ACME, 100, 200, true)).toEqual(result);
    expect(fetch).toHaveBeenCalledTimes(1);
    let [url, init] = fetch.mock.calls[0] as [URL, RequestInit];
    expect(url.pathname).toBe('/api/v1/holdings/acme/commercial/seat-history');
    expect(url.searchParams.get('start')).toBe('100');
    expect(url.searchParams.get('end')).toBe('200');
    let headers = init.headers as Headers;
    expect(headers.get('host')).toBe('acme.jesadakorn.com');
    expect(headers.get('x-erpx-context-holding')).toBe('h001');
    expect(headers.get('x-erpx-context-company')).toBe('company-acme');
    expect(headers.get('x-erpx-lane37-cutoff-dependency')).toBe('true');

    await readNativeSeatHistory(NORTHSTAR, 100, 200, false);
    [url, init] = fetch.mock.calls[1] as [URL, RequestInit];
    expect(url.pathname).toBe('/api/v1/holdings/northstar/commercial/seat-history');
    headers = init.headers as Headers;
    expect(headers.get('host')).toBe('northstar.jesadakorn.com');
    expect(headers.get('x-erpx-context-holding')).toBe('h002');
    expect(headers.get('x-erpx-context-company')).toBe('company-northstar');
    expect(headers.get('x-erpx-lane37-cutoff-dependency')).toBe('false');
  });

  it('rejects unknown targets, mismatched holding ids and does not default to the first holding', async () => {
    process.env[ENV] = config();
    const fetch = vi.fn().mockResolvedValue(ok(result));
    vi.stubGlobal('fetch', fetch);
    for (const actor of [
      { holdingId: 'unknown', holdingSlug: 'acme' },
      { holdingId: 'h001', holdingSlug: 'northstar' },
      { holdingId: 'h002', holdingSlug: 'acme' },
      { holdingId: 'h001', holdingSlug: 'unknown' },
    ] as Lane37Actor[])
      await expect(readNativeSeatHistory(actor, 100, 200, false)).rejects.toMatchObject({
        status: 503,
        code: 'LANE37_TARGET_UNAVAILABLE',
      });
    expect(fetch).not.toHaveBeenCalled();
  });

  it('validates 1..64 holdings, reserved ports, non-loopback bases and bounded input', async () => {
    process.env[ENV] = config({ holdings: [] });
    await expect(readNativeSeatHistory(ACME, 100, 200, false)).rejects.toMatchObject({
      status: 503,
      code: 'LANE37_CONFIGURATION_INVALID',
    });
    process.env[ENV] = config({
      holdings: Array.from({ length: 65 }, (_, i) => ({
        origin: 'https://acme.jesadakorn.com',
        host: 'acme.jesadakorn.com',
        holdingId: `h${i}`,
        holdingSlug: 'acme',
      })),
    });
    await expect(readNativeSeatHistory(ACME, 100, 200, false)).rejects.toMatchObject({
      status: 503,
      code: 'LANE37_CONFIGURATION_INVALID',
    });
    for (const port of [3000, 3001, 3100, 3101, 3103, 5432, 8080, 3200]) {
      process.env[ENV] = config({ privateBaseUrl: `http://127.0.0.1:${port}` });
      await expect(readNativeSeatHistory(ACME, 100, 200, false)).rejects.toMatchObject({
        status: 503,
      });
    }
    process.env[ENV] = config({ privateBaseUrl: 'http://192.168.1.1:3198' });
    await expect(readNativeSeatHistory(ACME, 100, 200, false)).rejects.toMatchObject({
      status: 503,
    });
    process.env[ENV] = config();
    await expect(readNativeSeatHistory({ holdingId: '', holdingSlug: 'acme' }, 100, 200, false)).rejects.toMatchObject({
      status: 503,
      code: 'LANE37_INPUT_INVALID',
    });
    await expect(readNativeSeatHistory(ACME, 200, 100, false)).rejects.toMatchObject({
      status: 503,
      code: 'LANE37_INPUT_INVALID',
    });
  });

  it('fails closed with 503 on malformed responses, non-200 and missing config', async () => {
    process.env[ENV] = config();
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(ok({ rows: {}, meter: {} })));
    await expect(readNativeSeatHistory(ACME, 100, 200, false)).rejects.toMatchObject({
      status: 503,
      code: 'LANE37_RESPONSE_INVALID',
    });

    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('{}', { status: 502 })));
    await expect(readNativeSeatHistory(ACME, 100, 200, false)).rejects.toMatchObject({
      status: 503,
      code: 'LANE37_READ_UNAVAILABLE',
    });

    vi.stubGlobal('fetch', vi.fn().mockRejectedValue(new Error('offline')));
    await expect(readNativeSeatHistory(ACME, 100, 200, false)).rejects.toMatchObject({
      status: 503,
      code: 'LANE37_READ_UNAVAILABLE',
    });

    delete process.env[ENV];
    await expect(readNativeSeatHistory(ACME, 100, 200, false)).rejects.toMatchObject({
      status: 503,
      code: 'LANE37_CONFIGURATION_REQUIRED',
    });
    expect(nativeLane37Configured()).toBe(false);
  });
});
