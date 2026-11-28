import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import {
  nativeLane35Configured,
  readNativeWorkReportCandidates,
  readNativeWorkReportEvents,
  validateCandidates,
  validateEvents,
  type NativeLane35Transport,
} from '../../../modules/records/server/src/adapters/native-lane-35/client';
import { ApiError } from '@erpx/platform-runtime-server/public/http';

const root = resolve(__dirname, '../../..');
const read = (p: string) => readFileSync(resolve(root, p), 'utf8');
const sha = (s: string) => createHash('sha256').update(s, 'utf8').digest('hex');

/**
 * Before-oracles captured from the exact HEAD literals of the two replaced
 * TypeScript adapters. Whole-string byte identity is the oracle; the Rust lane
 * keeps these in `postgres.rs` as LEGACY_*_SQL and executes owner-local SQL.
 */
const before = {
  candidatesSql: "SELECT approval_json,items_json,r.id,'request' kind,kind type,title,state status,requester_user_id requester,branch_id branch,department_id department,currency,NULL::bigint amount,NULL::numeric units,(SELECT MIN(created_at) FROM employee_request_events e WHERE e.request_id=r.id AND e.company_id=r.company_id AND e.action='submit') submitted_at,created_at,updated_at FROM employee_requests r WHERE company_id=? AND kind IN ('purchase','service')",
  candidatesSqlSha: '16a1c9483edda55fc0b04bb1b718ca5697c33bcf7b8c23548fdda3acc6a0e75d',
  eventsSql: 'SELECT request_id record_id,action,actor_id actor,created_at at,note reason FROM employee_request_events WHERE company_id=? AND request_id IN (SELECT jsonb_array_elements_text(?::jsonb)) ORDER BY created_at,id',
  eventsSqlSha: 'a4da158e6c69605340291d0e930f3e0162837598c5439109fbb2bdbddcc79945',
  candidatesResultSha: '6423ea20994720d28c2523417c3212681efab61d986388b13aa4228d69bd1bdb',
  eventsResultSha: 'f80f4e5c4125eb6ca8ffb76f63ca056b0acf8fbfeddf8a1d6706a02186158daf',
};

const goldenCandidates = [
  {
    approval_json: '{"steps":[]}', items_json: '[]', id: 'r1', kind: 'request', type: 'purchase',
    title: 'Desk', status: 'queued', requester: 'u1', branch: null, department: null,
    currency: 'USD', amount: null, units: null, submitted_at: 100, created_at: 90, updated_at: 110,
  },
  {
    approval_json: null, items_json: null, id: 'r2', kind: 'request', type: 'service',
    title: 'Cleaning', status: 'draft', requester: 'u2', branch: 'b1', department: null,
    currency: null, amount: null, units: null, submitted_at: null, created_at: 200, updated_at: 210,
  },
];
const goldenEvents = [
  { record_id: 'r1', action: 'submit', actor: 'u1', at: 100, reason: 'go' },
  { record_id: 'r1', action: 'approve', actor: 'u2', at: 110, reason: 'ok' },
  { record_id: 'r2', action: 'submit', actor: 'u2', at: 200, reason: 'go' },
];

const config = JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:44991',
  holdingSlug: 'acme',
  origin: 'https://acme.jesadakorn.com',
  timeoutMs: 1000,
});

const reply = (data: unknown, status = 200): Response =>
  new Response(JSON.stringify({ data }), { status, headers: { 'content-type': 'application/json' } });

const transport = (data: unknown): NativeLane35Transport => async (url, headers, body) => {
  expect(url.pathname).toBe('/api/v1/holdings/acme/records/native-lane-35/work-report');
  expect(headers.host).toBe('acme.jesadakorn.com');
  expect(headers['x-erpx-context-company']).toBe('acme');
  expect(JSON.parse(body)).toMatchObject({ companyId: 'acme' });
  return reply(data);
};

describe('native lane 35 SQL and result oracles', () => {
  test('before SQL literals pin the byte-identical HEAD statements', () => {
    expect(sha(before.candidatesSql)).toBe(before.candidatesSqlSha);
    expect(sha(before.eventsSql)).toBe(before.eventsSqlSha);
    expect(before.candidatesSql.match(/SELECT/g)).toHaveLength(2);
    expect(before.eventsSql.match(/SELECT/g)).toHaveLength(2);
  });

  test('golden result shapes are stable', () => {
    expect(sha(JSON.stringify(goldenCandidates))).toBe(before.candidatesResultSha);
    expect(sha(JSON.stringify(goldenEvents))).toBe(before.eventsResultSha);
    expect(validateCandidates(goldenCandidates)).toEqual(goldenCandidates);
    expect(validateEvents(goldenEvents)).toEqual(goldenEvents);
  });

  test('replaced TypeScript adapters retain no SQL and delegate to the native client', () => {
    const candidates = read('modules/records/server/src/adapters/work-report-candidates.ts');
    const facts = read('modules/records/server/src/adapters/work-report-facts.ts');
    for (const source of [candidates, facts]) {
      expect(source).not.toMatch(/SELECT\s|INSERT\s+INTO|DELETE\s+FROM|UPDATE\s+\w+\s+SET/);
      expect(source).not.toContain('.prepare(');
      expect(source).toContain('native-lane-35/client');
    }
    expect(facts).toContain('results:');
  });
});

describe('native lane 35 strict transport', () => {
  test('reads the candidates envelope through the exact owner route', async () => {
    process.env.ERPX_NATIVE_RECORDS_LANE35_OWNER_READ_CONFIG = config;
    const rows = await readNativeWorkReportCandidates('acme', transport({ candidates: goldenCandidates }));
    expect(rows).toEqual(goldenCandidates);
    expect(nativeLane35Configured()).toBe(true);
  });

  test('reads the events envelope through the exact owner route', async () => {
    process.env.ERPX_NATIVE_RECORDS_LANE35_OWNER_READ_CONFIG = config;
    const rows = await readNativeWorkReportEvents(
      'acme',
      '["r1","r2"]',
      async (url, headers, body) => {
        expect(url.pathname).toBe('/api/v1/holdings/acme/records/native-lane-35/work-report');
        expect(JSON.parse(body)).toMatchObject({ kind: 'events', ids: '["r1","r2"]' });
        return reply({ events: goldenEvents });
      },
    );
    expect(rows).toEqual(goldenEvents);
  });

  test('rejects a malformed row instead of forwarding it', () => {
    const bad = [{ ...goldenCandidates[0], kind: 'event' }];
    expect(() => validateCandidates(bad)).toThrowError(ApiError);
    expect(() => validateCandidates([{ ...goldenCandidates[0], extra: 1 }])).toThrowError(ApiError);
    expect(() => validateEvents([{ ...goldenEvents[0], at: 'now' }])).toThrowError(ApiError);
  });

  test('fails closed on missing configuration', async () => {
    delete process.env.ERPX_NATIVE_RECORDS_LANE35_OWNER_READ_CONFIG;
    await expect(readNativeWorkReportCandidates('acme', transport({ candidates: [] }))).rejects.toMatchObject({
      status: 503,
      code: 'NATIVE_RECORDS_WORK_REPORT_UNAVAILABLE',
    });
  });

  test('fails closed on a rejected or malformed native reply', async () => {
    process.env.ERPX_NATIVE_RECORDS_LANE35_OWNER_READ_CONFIG = config;
    await expect(
      readNativeWorkReportCandidates('acme', async () => reply(null, 503)),
    ).rejects.toMatchObject({ status: 503, code: 'NATIVE_RECORDS_WORK_REPORT_UNAVAILABLE' });
    await expect(
      readNativeWorkReportEvents('acme', '["r1"]', async () => reply({ candidates: [] })),
    ).rejects.toMatchObject({ status: 503, code: 'NATIVE_RECORDS_WORK_REPORT_UNAVAILABLE' });
    await expect(
      readNativeWorkReportCandidates('acme', async () => reply({ candidates: [{ id: 'x' }] })),
    ).rejects.toMatchObject({ status: 503 });
  });

  test('fails closed on a rejected 422 without any fallback read', async () => {
    process.env.ERPX_NATIVE_RECORDS_LANE35_OWNER_READ_CONFIG = config;
    await expect(
      readNativeWorkReportCandidates(
        'bad id',
        async () => new Response('{}', { status: 422 }),
      ),
    ).rejects.toMatchObject({ status: 503, code: 'NATIVE_RECORDS_WORK_REPORT_UNAVAILABLE' });
  });
});
