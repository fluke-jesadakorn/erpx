import { createServer, type Server } from 'node:http';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import {
  nativeLearningEvidenceConfigured,
  nativeLearningEvidenceRow,
  nativeLearningEvidenceSearch,
  type NativeLearningEvidenceRow,
} from '../../../packages/server-learning/src/adapters/native-lane-31/client';
import { ApiError } from '@erpx/server-runtime/public/http';

const ENV = 'ERPX_NATIVE_LEARNING_EVIDENCE_CONFIG';
const host = 'acme.jesadakorn.com';
const origin = `https://${host}`;
const company = 'company-1';
const actor = {
  holdingId: 'h001',
  holdingSlug: 'acme',
  companyId: company,
  branchId: 'branch-west',
  departmentId: 'dept-safety',
};

let server: Server | undefined;
afterEach(() => {
  server?.close();
  server = undefined;
  delete process.env[ENV];
});

async function listen(
  handler: (req: { headers: Record<string, unknown>; url?: string; body: string }) => {
    status: number;
    value: unknown;
  },
): Promise<string> {
  server = createServer((req, res) => {
    const chunks: Buffer[] = [];
    req.on('data', chunk => chunks.push(Buffer.from(chunk)));
    req.on('end', () => {
      const result = handler({
        headers: req.headers,
        url: req.url,
        body: Buffer.concat(chunks).toString('utf8'),
      });
      res.writeHead(result.status, { 'content-type': 'application/json' });
      res.end(JSON.stringify(result.value));
    });
  });
  await new Promise<void>(r => server!.listen(0, '127.0.0.1', r));
  const address = server.address();
  if (!address || typeof address === 'string') throw Error('no listener');
  return `http://127.0.0.1:${address.port}`;
}

function configure(base: string) {
  process.env[ENV] = JSON.stringify({
    privateBaseUrl: base,
    timeoutMs: 1000,
    holdings: [{ origin, host, holdingId: actor.holdingId, holdingSlug: actor.holdingSlug, companyId: actor.companyId }],
  });
}

const DEV_HOLDINGS = [
  { origin: 'https://acme.jesadakorn.com', host: 'acme.jesadakorn.com', holdingId: 'h001', holdingSlug: 'acme' },
  { origin: 'https://northstar.jesadakorn.com', host: 'northstar.jesadakorn.com', holdingId: 'h002', holdingSlug: 'northstar' },
  { origin: 'https://erpx-internal.jesadakorn.com', host: 'erpx-internal.jesadakorn.com', holdingId: 'h003', holdingSlug: 'erpx' },
] as const;

function configureHoldings(base: string, holdings: unknown) {
  process.env[ENV] = JSON.stringify({ privateBaseUrl: base, timeoutMs: 10000, holdings });
}

const completionRow: NativeLearningEvidenceRow = {
  id: 'event-1',
  assignment_id: 'assignment-1',
  event_version: 3,
  status: 'completed',
  created_at: 1_700_000_000_000,
  course_title: 'Workplace Safety',
};

describe('native Learning evidence-detail transport', () => {
  it('is configured only through the native env', () => {
    delete process.env[ENV];
    expect(nativeLearningEvidenceConfigured()).toBe(false);
    configure('http://127.0.0.1:43210');
    expect(nativeLearningEvidenceConfigured()).toBe(true);
  });

  it('fails closed when configuration is missing', async () => {
    delete process.env[ENV];
    await expect(nativeLearningEvidenceRow(actor, 'learning-completions', 'event-1', { from: 1, toExclusive: 2 }))
      .rejects.toMatchObject({ status: 503, code: 'LEARNING_EVIDENCE_READ_CONFIGURATION_REQUIRED' });
  });

  it('routes each actor to its own registered Holding and never defaults', async () => {
    const seen: string[] = [];
    const base = await listen(req => {
      seen.push(String(req.url));
      return { status: 200, value: { row: completionRow } };
    });
    configureHoldings(base, DEV_HOLDINGS);
    for (const holding of DEV_HOLDINGS) {
      const row = await nativeLearningEvidenceRow(
        { ...actor, holdingId: holding.holdingId, holdingSlug: holding.holdingSlug },
        'learning-completions',
        'event-1',
        { from: 1, toExclusive: 2 },
      );
      expect(row).toEqual(completionRow);
    }
    expect(seen).toEqual(DEV_HOLDINGS.map(holding =>
      `/api/v1/holdings/${holding.holdingSlug}/learning/evidence-detail`,
    ));
    await expect(
      nativeLearningEvidenceRow(
        { ...actor, holdingId: 'h999', holdingSlug: 'unknown' },
        'learning-completions',
        'event-1',
        { from: 1, toExclusive: 2 },
      ),
    ).rejects.toMatchObject({ status: 503, code: 'LEARNING_EVIDENCE_READ_TARGET_UNAVAILABLE' });
    expect(seen).toHaveLength(3); // unknown target made no fallback request
  });

  it('accepts 1..64 holdings and rejects empty, oversized or duplicate registries', async () => {
    const base = 'http://127.0.0.1:43210';
    const read = () => nativeLearningEvidenceRow(actor, 'overdue-required-learning', 'event-1');
    configureHoldings(base, []);
    await expect(read()).rejects.toMatchObject({ code: 'LEARNING_EVIDENCE_READ_CONFIGURATION_INVALID' });
    const many = Array.from({ length: 65 }, (_, i) => ({
      origin: `https://h${i}.jesadakorn.com`,
      host: `h${i}.jesadakorn.com`,
      holdingId: `h${i}`,
      holdingSlug: `h${i}`,
    }));
    configureHoldings(base, many);
    await expect(read()).rejects.toMatchObject({ code: 'LEARNING_EVIDENCE_READ_CONFIGURATION_INVALID' });
    configureHoldings(base, [DEV_HOLDINGS[0], DEV_HOLDINGS[0]]);
    await expect(read()).rejects.toMatchObject({ code: 'LEARNING_EVIDENCE_READ_CONFIGURATION_INVALID' });
    configureHoldings(base, [DEV_HOLDINGS[0]]);
    await expect(read()).rejects.toMatchObject({ status: 503, code: 'LEARNING_EVIDENCE_READ_UNAVAILABLE' });
  });

  it('accepts the exact dev env JSON for all three registered holdings', async () => {
    process.env[ENV] = JSON.stringify({
      privateBaseUrl: 'http://127.0.0.1:3198',
      timeoutMs: 10000,
      holdings: [
        { origin: 'https://acme.jesadakorn.com', host: 'acme.jesadakorn.com', holdingId: 'h001', holdingSlug: 'acme' },
        { origin: 'https://northstar.jesadakorn.com', host: 'northstar.jesadakorn.com', holdingId: 'h002', holdingSlug: 'northstar' },
        { origin: 'https://erpx-internal.jesadakorn.com', host: 'erpx-internal.jesadakorn.com', holdingId: 'h003', holdingSlug: 'erpx' },
      ],
    });
    expect(nativeLearningEvidenceConfigured()).toBe(true);
    const read = () => nativeLearningEvidenceRow(actor, 'overdue-required-learning', 'event-1');
    // Config parsed and the actor's h001 target was selected: any failure now
    // is transport-level, never a config or unknown-target rejection.
    const error = await read().catch((cause: unknown) => cause);
    expect(error).toBeInstanceOf(ApiError);
    expect(['LEARNING_EVIDENCE_READ_FAILED', 'LEARNING_EVIDENCE_READ_UNAVAILABLE'])
      .toContain((error as ApiError).code);
    await expect(nativeLearningEvidenceRow(
      { ...actor, holdingId: 'h999', holdingSlug: 'unknown' },
      'overdue-required-learning',
      'event-1',
    )).rejects.toMatchObject({ code: 'LEARNING_EVIDENCE_READ_TARGET_UNAVAILABLE' });
  });

  it('fails closed when the configured companyId disagrees with the actor', async () => {
    configureHoldings('http://127.0.0.1:43210', [
      { ...DEV_HOLDINGS[0], companyId: 'another-company' },
    ]);
    await expect(nativeLearningEvidenceRow(actor, 'overdue-required-learning', 'event-1'))
      .rejects.toMatchObject({ status: 503, code: 'LEARNING_EVIDENCE_READ_TARGET_CHANGED' });
  });

  it('carries only bounded identities and pins the projected row shape', async () => {
    let seen: { headers: Record<string, unknown>; url?: string; body: string } | undefined;
    const base = await listen(req => {
      seen = req;
      return { status: 200, value: { row: completionRow } };
    });
    configure(base);
    const row = await nativeLearningEvidenceRow(actor, 'learning-completions', 'event-1', {
      from: 1_700_000_000_000,
      toExclusive: 1_700_000_100_000,
    });
    expect(row).toEqual(completionRow);
    expect(seen!.headers['x-erpx-context-company']).toBe(company);
    expect(seen!.headers['x-erpx-context-branch']).toBe('branch-west');
    expect(seen!.headers['x-erpx-context-department']).toBe('dept-safety');
    expect(seen!.url).toBe(`/api/v1/holdings/acme/learning/evidence-detail`);
    const body = JSON.parse(seen!.body);
    expect(body).toEqual({
      kind: 'learning-completions',
      id: 'event-1',
      from: 1_700_000_000_000,
      toExclusive: 1_700_000_100_000,
    });
  });

  it('returns null for an absent row', async () => {
    const base = await listen(() => ({ status: 200, value: { row: null } }));
    configure(base);
    expect(await nativeLearningEvidenceRow(actor, 'overdue-required-learning', 'event-1')).toBeNull();
  });

  it('rejects a malformed row rather than forwarding it', async () => {
    const base = await listen(() => ({
      status: 200,
      value: { row: { ...completionRow, injected: 'sql' } },
    }));
    configure(base);
    await expect(nativeLearningEvidenceRow(actor, 'learning-completions', 'event-1', { from: 1, toExclusive: 2 }))
      .rejects.toMatchObject({ status: 503, code: 'LEARNING_EVIDENCE_READ_RESPONSE_INVALID' });
  });

  it('maps a 4xx rejection and fails closed on 5xx', async () => {
    let status = 422;
    const base = await listen(() => ({
      status,
      value: { error: { code: 'LEARNING_EVIDENCE_READ_REQUEST_INVALID', message: 'rejected' } },
    }));
    configure(base);
    await expect(nativeLearningEvidenceRow(actor, 'learning-completions', 'event-1', { from: 1, toExclusive: 2 }))
      .rejects.toMatchObject({ status: 422, code: 'LEARNING_EVIDENCE_READ_REQUEST_INVALID' });
    status = 502;
    await expect(nativeLearningEvidenceRow(actor, 'learning-completions', 'event-1', { from: 1, toExclusive: 2 }))
      .rejects.toMatchObject({ status: 503, code: 'LEARNING_EVIDENCE_READ_UNAVAILABLE' });
  });

  it('fails closed when the native route is unreachable', async () => {
    configure('http://127.0.0.1:1');
    await expect(nativeLearningEvidenceRow(actor, 'overdue-required-learning', 'event-1'))
      .rejects.toMatchObject({ status: 503, code: 'LEARNING_EVIDENCE_READ_UNAVAILABLE' });
  });

  it('rejects invalid request shapes locally', async () => {
    configure('http://127.0.0.1:43210');
    await expect(nativeLearningEvidenceRow(actor, 'learning-completions', 'event-1', { from: 2, toExclusive: 1 }))
      .rejects.toBeInstanceOf(ApiError);
  });
});

const searchInput = {
  metric: 'learning-completions' as const,
  query: '',
  filters: { from: '2026-11-01', to: '2026-11-30', currency: 'USD' },
  limit: 25,
};

describe('native Learning evidence-search transport', () => {
  it('sends only bounded filters/cursor and pins the projected rows', async () => {
    let seen: { headers: Record<string, unknown>; url?: string; body: string } | undefined;
    const base = await listen(req => {
      seen = req;
      return { status: 200, value: { rows: [completionRow] } };
    });
    configureHoldings(base, DEV_HOLDINGS);
    const rows = await nativeLearningEvidenceSearch(actor, searchInput, { createdAt: 100, id: 'event-0' });
    expect(rows).toEqual([completionRow]);
    expect(seen!.url).toBe('/api/v1/holdings/acme/learning/evidence-search');
    expect(seen!.headers['x-erpx-context-company']).toBe(company);
    expect(JSON.parse(seen!.body)).toEqual({
      kind: 'learning-completions',
      query: '',
      from: Date.parse('2026-11-01'),
      toExclusive: Date.parse('2026-11-30') + 86400000,
      limit: 25,
      cursor: { createdAt: 100, id: 'event-0' },
    });
  });

  it('sends the overdue family without a millisecond range and pins its due-date cursor', async () => {
    let body = '';
    const overdueRow: NativeLearningEvidenceRow = {
      id: 'assignment-1',
      version: 2,
      status: 'assigned',
      due_date: '2026-11-20',
      updated_at: 1_700_000_000_000,
      course_title: 'Workplace Safety',
      required_for: 'onboarding',
    };
    const base = await listen(req => {
      body = req.body;
      return { status: 200, value: { rows: [overdueRow] } };
    });
    configureHoldings(base, DEV_HOLDINGS);
    const rows = await nativeLearningEvidenceSearch(
      actor,
      { ...searchInput, metric: 'overdue-required-learning' },
      { dueDate: '2026-11-20', id: 'assignment-0' },
    );
    expect(rows).toEqual([overdueRow]);
    expect(JSON.parse(body)).toEqual({
      kind: 'overdue-required-learning',
      query: '',
      from: null,
      toExclusive: null,
      limit: 25,
      cursor: { dueDate: '2026-11-20', id: 'assignment-0' },
    });
  });

  it('routes every holding to its own search path and never defaults', async () => {
    const seen: string[] = [];
    const base = await listen(req => {
      seen.push(String(req.url));
      return { status: 200, value: { rows: [] } };
    });
    configureHoldings(base, DEV_HOLDINGS);
    for (const holding of DEV_HOLDINGS) {
      await nativeLearningEvidenceSearch(
        { ...actor, holdingId: holding.holdingId, holdingSlug: holding.holdingSlug },
        searchInput,
        null,
      );
    }
    expect(seen).toEqual(DEV_HOLDINGS.map(holding =>
      `/api/v1/holdings/${holding.holdingSlug}/learning/evidence-search`,
    ));
    await expect(nativeLearningEvidenceSearch(
      { ...actor, holdingId: 'h999', holdingSlug: 'unknown' }, searchInput, null,
    )).rejects.toMatchObject({ code: 'LEARNING_EVIDENCE_READ_TARGET_UNAVAILABLE' });
    expect(seen).toHaveLength(3);
  });

  it('fails closed on malformed rows, unknown shape and 5xx', async () => {
    let status = 200;
    let value: unknown = { rows: [{ ...completionRow, injected: true }] };
    const base = await listen(() => ({ status, value }));
    configureHoldings(base, DEV_HOLDINGS);
    await expect(nativeLearningEvidenceSearch(actor, searchInput, null))
      .rejects.toMatchObject({ status: 503, code: 'LEARNING_EVIDENCE_READ_RESPONSE_INVALID' });
    value = { items: [] };
    await expect(nativeLearningEvidenceSearch(actor, searchInput, null))
      .rejects.toMatchObject({ status: 503, code: 'LEARNING_EVIDENCE_READ_UNAVAILABLE' });
    status = 502;
    value = { error: { code: 'BOOM', message: 'boom' } };
    await expect(nativeLearningEvidenceSearch(actor, searchInput, null))
      .rejects.toMatchObject({ status: 503, code: 'LEARNING_EVIDENCE_READ_UNAVAILABLE' });
  });

  it('rejects out-of-range limits and malformed cursors locally', async () => {
    configureHoldings('http://127.0.0.1:43210', DEV_HOLDINGS);
    await expect(nativeLearningEvidenceSearch(actor, { ...searchInput, limit: 51 }, null))
      .rejects.toMatchObject({ status: 422, code: 'LEARNING_EVIDENCE_READ_REQUEST_INVALID' });
    await expect(nativeLearningEvidenceSearch(actor, searchInput, { createdAt: 1, dueDate: '2026-11-25', id: 'x' } as never))
      .rejects.toBeInstanceOf(ApiError);
  });
});

const srcRoot = resolve(__dirname, '../../../packages/server-learning/src');
const postgres = readFileSync(resolve(srcRoot, 'adapters/native_lane_31/postgres.rs'), 'utf8');
const learning = readFileSync(resolve(srcRoot, 'adapters/learning.ts'), 'utf8');
const evidenceSources = readFileSync(resolve(srcRoot, 'adapters/executive-evidence-sources.ts'), 'utf8');

const completionSql = [
  'SELECT e.id,e.assignment_id,e.version event_version,e.status,e.created_at,c.title course_title',
  ' FROM learning_assignment_events e',
  ' JOIN learning_assignments a ON a.id=e.assignment_id',
  ' JOIN learning_courses c ON c.id=a.course_id',
  ' WHERE e.id=$1 AND a.company_id=$2 AND ($3::text IS NULL OR a.branch_id=$3)',
  ' AND ($4::text IS NULL OR a.department_id=$4)',
  " AND e.action='complete' AND e.status='completed' AND e.created_at>=$5 AND e.created_at<$6",
  ' LIMIT 1',
].join('');
const overdueSql = [
  'SELECT a.id,a.version,a.status,a.due_date,a.updated_at,c.title course_title,c.required_for',
  ' FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id',
  ' WHERE a.id=$1 AND a.company_id=$2 AND ($3::text IS NULL OR a.branch_id=$3)',
  " AND ($4::text IS NULL OR a.department_id=$4) AND c.required_for IS NOT NULL",
  " AND a.due_date<to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')",
  " AND a.status IN ('assigned','in_progress')",
  ' LIMIT 1',
].join('');

const sha256 = (value: string) => createHash('sha256').update(value).digest('hex');

describe('native Learning evidence-detail SQL/result oracle', () => {
  it('pins the owner-planned rendered SQL', () => {
    for (const line of [
      'SELECT e.id,e.assignment_id,e.version event_version,e.status,e.created_at,c.title course_title',
      'FROM learning_assignment_events e',
      'AND e.action=\'complete\' AND e.status=\'completed\' AND e.created_at>=$5 AND e.created_at<$6',
      'SELECT a.id,a.version,a.status,a.due_date,a.updated_at,c.title course_title,c.required_for',
      'FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id',
      'AND a.status IN (\'assigned\',\'in_progress\')',
    ]) expect(postgres).toContain(line);
    expect(sha256(completionSql)).toBe('9c0e1b6715fbe5e26abaeb7eb07922c959566d2a40d2718d86ecc3621a307298');
    expect(sha256(overdueSql)).toBe('e2ca860ac39c0223d6f81ee46c627eeca7caa8bab33bf71f19b6e2e2414732f5');
  });

  it('pins the result shape and the company scope predicates', () => {
    for (const predicate of [
      'a.company_id=$2',
      '($3::text IS NULL OR a.branch_id=$3)',
      '($4::text IS NULL OR a.department_id=$4)',
    ]) expect(postgres).toContain(predicate);
    expect(Object.keys(completionRow).sort()).toEqual([
      'assignment_id', 'course_title', 'created_at', 'event_version', 'id', 'status',
    ]);
  });

  it('retires the two inline TypeScript detail readers with no fallback', () => {
    expect(learning).toContain('nativeLearningEvidenceRow(');
    expect(learning).not.toContain('db.prepare(`${completionEvidenceColumns}');
    expect(learning).not.toContain('db.prepare(`${overdueEvidenceColumns}');
    expect(learning).not.toContain("AND e.action='complete' AND e.status='completed'");
    expect(learning).not.toContain("AND a.status IN ('assigned','in_progress')");
  });
});

const completionSearchSql = [
  'SELECT e.id,e.assignment_id,e.version event_version,e.status,e.created_at,c.title course_title',
  ' FROM learning_assignment_events e',
  ' JOIN learning_assignments a ON a.id=e.assignment_id AND a.company_id=$1',
  ' AND ($2::text IS NULL OR a.branch_id=$3)',
  ' AND ($4::text IS NULL OR a.department_id=$5)',
  " AND e.action='complete' AND e.status='completed' AND e.created_at>=$6 AND e.created_at<$7",
  ' ORDER BY e.created_at DESC,e.id DESC LIMIT $8',
].join('');
const overdueSearchSql = [
  'SELECT a.id,a.version,a.status,a.due_date,a.updated_at,c.title course_title,c.required_for',
  ' FROM learning_assignments a JOIN learning_courses c ON c.id=a.course_id AND c.company_id=a.company_id',
  ' WHERE a.company_id=$1',
  ' AND ($2::text IS NULL OR a.branch_id=$3)',
  ' AND ($4::text IS NULL OR a.department_id=$5)',
  ' AND c.required_for IS NOT NULL',
  " AND a.due_date<to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')",
  " AND a.status IN ('assigned','in_progress')",
  ' ORDER BY a.due_date,a.id LIMIT $6',
].join('');

describe('native Learning evidence-search SQL/result oracle and cutover KPI', () => {
  it('pins the owner-planned search SQL and company scope', () => {
    for (const fragment of [
      'JOIN learning_assignments a ON a.id=e.assignment_id AND a.company_id=',
      '::text IS NULL OR a.branch_id=',
      '::text IS NULL OR a.department_id=',
      "e.action='complete' AND e.status='completed'",
      "to_char(to_timestamp(e.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM-DD')",
      'ORDER BY e.created_at DESC,e.id DESC LIMIT ',
      'c.required_for IS NOT NULL',
      "a.status IN ('assigned','in_progress')",
      'ORDER BY a.due_date,a.id LIMIT ',
      'position(lower(',
    ]) expect(postgres).toContain(fragment);
    expect(sha256(completionSearchSql)).toBe('d12a1b1f85e946057c840b3c0fb424130b667df9e685049bbf9ab8f7d18a1e87');
    expect(sha256(overdueSearchSql)).toBe('5c5668373bb81c45cf6e6487c2c81a25eb41669ecec045a7311d2c887edbc8a3');
  });

  it('retires both search SQL readers with no TypeScript fallback', () => {
    expect(evidenceSources).toContain('nativeLearningEvidenceSearch(');
    expect(evidenceSources).not.toContain('db.prepare(');
    expect(evidenceSources).not.toContain('${completionEvidenceColumns}');
    expect(evidenceSources).not.toContain('${overdueEvidenceColumns}');
    expect(evidenceSources).not.toContain("AND e.action='complete' AND e.status='completed'");
    expect(evidenceSources).not.toContain("AND a.status IN ('assigned','in_progress')");
    expect(evidenceSources).toContain('learningMetricSource('); // authority gate retained
  });
});
