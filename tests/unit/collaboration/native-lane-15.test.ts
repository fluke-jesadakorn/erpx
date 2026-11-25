import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { createHash } from 'node:crypto';
import { createServer, type IncomingMessage, type Server } from 'node:http';
import fs from 'node:fs';
import path from 'node:path';

vi.mock('server-only', () => ({}));
vi.mock('next/headers', () => ({
  headers: async () => new Headers({ host: 'acme.jesadakorn.com' }),
}));
vi.mock('@erpx/server-identity_access/public/authorization', () => ({
  requireWorkContext: async () => ({ holdingId: 'h001', holdingSlug: 'acme', companyId: 'c1' }),
}));

import {
  nativeBetaPostDrafts,
  nativeBetaReports,
  nativeBetaPostTask,
  nativeBetaRecoverPostTask,
  nativeCollaborationLane15Configured,
  NATIVE_COLLABORATION_LANE15_CONFIG_ENV,
  type HoldingActor,
} from '../../../packages/server-collaboration/src/adapters/native-lane-15/client';
import { fieldCollaborationCommand } from '../../../packages/server-collaboration/src/adapters/native-lane-15/field-command-client';
import { betaManagementFacts } from '../../../packages/server-collaboration/src/adapters/beta-management-facts';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';

const root = process.cwd();
const sha = (value: string) => createHash('sha256').update(value).digest('hex');
const ENV = NATIVE_COLLABORATION_LANE15_CONFIG_ENV;

const ACME: HoldingActor = { holdingId: 'h001', holdingSlug: 'acme', companyId: 'c1' };
const NORTHSTAR: HoldingActor = { holdingId: 'h002', holdingSlug: 'northstar', companyId: 'c2' };
const ERPX: HoldingActor = { holdingId: 'h003', holdingSlug: 'erpx', companyId: 'c3' };

const draft = {
  id: 'd1', body: 'hello', branch_id: null, department_id: 'dept',
  audience_kind: 'company', audience_id: null, participants_json: '[]', sources_json: '[]',
  purpose: 'update', version: 2, mentions_json: '[]', content_json: null, rich_content_json: null,
};
const report = { id: 'r1', reason: 'spam', post_id: 'p1', state: 'published' };

const SQL_ORACLES = [
  '145ec622c215ab9235285509afb7195d5d7efece08e92dbc9dbe1668eb0cedfa',
  'cccb02bfd08a66967adb5e2e8411572a189aff8f9809897ae51cbd2627b1e1ce',
  '33555d45dd604f822eccdcf806ef0c48cfc59a305246e24e4a8979e666fef0b5',
];
const SQL_JOINED_ORACLE = 'a85ef6fcafb2334335f84dc9988e64702872c73c483d09757be95bcb92120559';
const DRAFT_ORACLE = 'b331e4c8a23043dddea600430614dddeda68f6d30457e4b28a8d17bb37751ad4';
const REPORT_ORACLE = '6e3f13a06d17cee8be2eaa3ebae8921ac823e4724f6d5201b9521749308bc11e';

const DEV_CONFIG = (port: number) => ({
  privateBaseUrl: `http://127.0.0.1:${port}`,
  timeoutMs: 10000,
  holdings: [
    { origin: 'https://acme.jesadakorn.com', host: 'acme.jesadakorn.com', holdingId: 'h001', holdingSlug: 'acme' },
    { origin: 'https://northstar.jesadakorn.com', host: 'northstar.jesadakorn.com', holdingId: 'h002', holdingSlug: 'northstar' },
    { origin: 'https://erpx-internal.jesadakorn.com', host: 'erpx-internal.jesadakorn.com', holdingId: 'h003', holdingSlug: 'erpx' },
  ],
});

type Seen = { url: string; host: string | undefined; origin: string | undefined; company: string | undefined; body: unknown };
let server: Server;
let requests: Seen[];
let port: number;
let mode: 'ok' | 'empty' | 'bad' | 'down' | 'rejected' = 'ok';
let original: string | undefined;

const respond = (res: import('node:http').ServerResponse, body: unknown, status = 200) => {
  res.writeHead(status, { 'content-type': 'application/json' });
  res.end(JSON.stringify(body));
};

const dataFor = (kind: string, empty: boolean) => {
  if (kind === 'post_drafts') return { kind, drafts: empty ? [] : [draft] };
  if (kind === 'reports') return { kind, reports: empty ? [] : [report] };
  if (kind === 'post_task') return { kind, task: empty ? null : { task_id: 't1' } };
  return { kind, task: null };
};

beforeAll(async () => {
  requests = [];
  server = createServer(async (req: IncomingMessage, res) => {
    const url = new URL(req.url ?? '/', 'http://127.0.0.1');
    const chunks: Buffer[] = [];
    for await (const chunk of req) chunks.push(Buffer.from(chunk));
    let body: unknown = undefined;
    try { body = JSON.parse(Buffer.concat(chunks).toString('utf8')); } catch { /* ignored */ }
    requests.push({
      url: url.pathname,
      host: req.headers.host,
      origin: req.headers.origin,
      company: req.headers['x-erpx-context-company'] as string | undefined,
      body,
    });
    if (mode === 'down') return respond(res, { error: { code: 'DOWN', message: 'down' } }, 503);
    if (mode === 'rejected') return respond(res, { error: { code: 'NATIVE_LANE15_REJECTED', message: 'rejected' } }, 422);
    if (mode === 'bad') return respond(res, { data: { kind: 'post_drafts', drafts: [{ id: 'only-id' }] } });
    if (url.pathname.endsWith('/field-command')) {
      const command = body as { entity?: string; id?: string; sourceType?: string; targetType?: string };
      // Simulate the owner's collaboration_* row projection for id reads.
      if (command.id === 'rel-1') return respond(res, { data: { fieldOwned: true } });
      if (command.id === 'card-1') return respond(res, { data: { fieldOwned: false } });
      const owned = command.sourceType === 'field_service_order' ||
        (command.entity === 'relation' && command.targetType === 'field_service_order');
      return respond(res, { data: { fieldOwned: owned } });
    }
    const kind = (body as { kind?: string } | undefined)?.kind ?? '';
    return respond(res, { data: dataFor(kind, mode === 'empty') });
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('no server port');
  port = address.port;
});

afterAll(async () => {
  await new Promise<void>(resolve => server.close(() => resolve()));
});

beforeEach(() => {
  mode = 'ok';
  requests = [];
  original = process.env[ENV];
  process.env[ENV] = JSON.stringify(DEV_CONFIG(port));
});

afterEach(() => {
  if (original === undefined) delete process.env[ENV];
  else process.env[ENV] = original;
});

describe('native Collaboration lane 15 beta-facts reads', () => {
  it('pins the retired SQL and result projection oracles', () => {
    const retired = [
      'SELECT id,body,branch_id,department_id,audience_kind,audience_id,participants_json,sources_json,purpose,version,mentions_json,content_json,rich_content_json FROM social_post_drafts WHERE author_id=? AND company_id=? AND published_post_id IS NULL AND comment_parent_post_id IS NULL AND edit_post_id IS NULL ORDER BY updated_at DESC LIMIT 20',
      'SELECT r.id,r.reason,r.post_id,p.state FROM social_reports r JOIN social_posts p ON p.id=r.post_id WHERE p.company_id=? ORDER BY r.created_at DESC LIMIT 100',
      'SELECT task_id FROM social_task_origins WHERE post_id=?',
    ];
    expect(retired.map(sha)).toEqual(SQL_ORACLES);
    expect(sha(retired.join('\n'))).toBe(SQL_JOINED_ORACLE);
    expect(sha(JSON.stringify(draft))).toBe(DRAFT_ORACLE);
    expect(sha(JSON.stringify(report))).toBe(REPORT_ORACLE);
  });

  it('accepts the exact three-holding dev configuration shape', () => {
    expect(nativeCollaborationLane15Configured()).toBe(true);
    const config = DEV_CONFIG(port);
    expect(config.holdings).toHaveLength(3);
    expect(config.holdings.map(holding => [holding.holdingId, holding.holdingSlug])).toEqual([
      ['h001', 'acme'], ['h002', 'northstar'], ['h003', 'erpx'],
    ]);
  });

  it('delegates all four owner reads to the native route with isolated headers', async () => {
    const db = {} as never;
    const facts = betaManagementFacts(db);
    expect(await facts.postDrafts('u1', 'c1')).toEqual([draft]);
    expect(await facts.reports('c1')).toEqual([report]);
    expect(await facts.postTask('p1')).toEqual({ task_id: 't1' });
    expect(await facts.recoverPostTask('p1')).toBeNull();
    expect(requests.map(r => r.url)).toEqual(Array(4).fill(
      '/api/v1/holdings/acme/collaboration/native-lane-15/beta-facts',
    ));
    expect(requests.map(r => r.host)).toEqual(Array(4).fill('acme.jesadakorn.com'));
    expect(requests.map(r => r.origin)).toEqual(Array(4).fill('https://acme.jesadakorn.com'));
    expect(requests.map(r => r.company)).toEqual(['c1', 'c1', undefined, undefined]);
    expect(requests.map(r => (r.body as { kind: string }).kind)).toEqual([
      'post_drafts', 'reports', 'post_task', 'recover_post_task',
    ]);
    expect(requests[0].body).toEqual({ kind: 'post_drafts', userId: 'u1', companyId: 'c1' });
    expect(requests[3].body).toEqual({ kind: 'recover_post_task', postId: 'p1' });
  });

  it('selects every registered Holding by WorkContext holdingId+holdingSlug', async () => {
    await nativeBetaReports(ACME, 'c1');
    await nativeBetaReports(NORTHSTAR, 'c2');
    await nativeBetaReports(ERPX, 'c3');
    expect(requests.map(r => r.url)).toEqual([
      '/api/v1/holdings/acme/collaboration/native-lane-15/beta-facts',
      '/api/v1/holdings/northstar/collaboration/native-lane-15/beta-facts',
      '/api/v1/holdings/erpx/collaboration/native-lane-15/beta-facts',
    ]);
    expect(requests.map(r => r.host)).toEqual([
      'acme.jesadakorn.com', 'northstar.jesadakorn.com', 'erpx-internal.jesadakorn.com',
    ]);
    expect(requests.map(r => r.company)).toEqual(['c1', 'c2', 'c3']);
  });

  it('fails closed on unknown or mismatched WorkContext targets with no default', async () => {
    await expect(nativeBetaReports({ holdingId: 'h999', holdingSlug: 'acme' }, 'c1'))
      .rejects.toMatchObject({ status: 409, code: 'WORK_CONTEXT_CHANGED' });
    await expect(nativeBetaReports({ holdingId: 'h002', holdingSlug: 'acme' }, 'c1'))
      .rejects.toMatchObject({ status: 409, code: 'WORK_CONTEXT_CHANGED' });
    await expect(nativeBetaReports({ holdingId: 'h001', holdingSlug: 'northstar' }, 'c1'))
      .rejects.toMatchObject({ status: 409, code: 'WORK_CONTEXT_CHANGED' });
    expect(requests).toHaveLength(0);
  });

  it('rejects configurations with duplicate targets or an invalid optional companyId', async () => {
    process.env[ENV] = JSON.stringify({
      ...DEV_CONFIG(port),
      holdings: [DEV_CONFIG(port).holdings[0], DEV_CONFIG(port).holdings[0]],
    });
    await expect(nativeBetaReports(ACME, 'c1')).rejects.toMatchObject({ status: 503 });
    process.env[ENV] = JSON.stringify({
      ...DEV_CONFIG(port),
      holdings: [{ ...DEV_CONFIG(port).holdings[0], companyId: 'bad company!' }],
    });
    await expect(nativeBetaReports(ACME, 'c1')).rejects.toMatchObject({ status: 503 });
  });

  it('preserves empty results and a missing task as null', async () => {
    mode = 'empty';
    expect(await nativeBetaPostDrafts(ACME, 'u1', 'c1')).toEqual([]);
    expect(await nativeBetaReports(ACME, 'c1')).toEqual([]);
    expect(await nativeBetaPostTask(ACME, 'p1')).toBeNull();
  });

  it('fails closed when the configuration is absent or malformed', async () => {
    delete process.env[ENV];
    expect(nativeCollaborationLane15Configured()).toBe(false);
    await expect(nativeBetaPostDrafts(ACME, 'u1', 'c1')).rejects.toMatchObject({
      status: 503, code: 'NATIVE_COLLABORATION_LANE15_UNAVAILABLE',
    });
    process.env[ENV] = 'not-json';
    await expect(nativeBetaReports(ACME, 'c1')).rejects.toMatchObject({ status: 503 });
    process.env[ENV] = JSON.stringify({
      privateBaseUrl: `http://127.0.0.1:${port}`,
      timeoutMs: 10000,
      holdings: [{ origin: 'https://unknown.example.com', host: 'unknown.example.com', holdingId: 'h009', holdingSlug: 'unknown' }],
    });
    await expect(nativeBetaPostTask(ACME, 'p1')).rejects.toMatchObject({ status: 409 });
  });

  it('fails closed when the service is down, rejects, or replies malformed', async () => {
    mode = 'down';
    await expect(nativeBetaReports(ACME, 'c1')).rejects.toMatchObject({ status: 503 });
    mode = 'rejected';
    await expect(nativeBetaReports(ACME, 'c1')).rejects.toMatchObject({
      status: 422, code: 'NATIVE_LANE15_REJECTED',
    });
    mode = 'bad';
    await expect(nativeBetaPostDrafts(ACME, 'u1', 'c1')).rejects.toMatchObject({
      status: 503, code: 'NATIVE_COLLABORATION_LANE15_RESPONSE_INVALID',
    });
    await expect(betaManagementFacts({} as never).reports('c1')).rejects.toMatchObject({ status: 503 });
  });

  it('rejects unbounded identities before issuing a request', async () => {
    await expect(nativeBetaPostDrafts(ACME, 'bad user!', 'c1')).rejects.toMatchObject({ status: 503 });
    await expect(nativeBetaPostTask(ACME, undefined)).rejects.toMatchObject({ status: 503 });
    expect(requests).toHaveLength(0);
  });

  it('removed every owner statement from the TypeScript read source', () => {
    const source = fs.readFileSync(
      path.join(root, 'packages/server-collaboration/src/adapters/beta-management-facts.ts'),
      'utf8',
    );
    expect(source).not.toMatch(/SELECT /);
    expect(source).not.toMatch(/\.prepare\(/);
    expect(source).not.toMatch(/\.bind\(/);
    expect(source).toContain("from './native-lane-15/client'");
  });

  it('lowers the lane SELECT-space KPI from 52 to 31 across both families', () => {
    const brief = fs.readFileSync(
      path.join(root, '.scratch/wave0/briefs/lane_15.md'), 'utf8',
    );
    const files = [...brief.matchAll(/^- `(packages\/server-collaboration\/src\/adapters\/[^`]+\.ts)`$/gm)]
      .map(match => match[1]);
    let count = 0;
    for (const file of files) {
      const absolute = path.join(root, file);
      if (!fs.existsSync(absolute)) continue;
      count += fs.readFileSync(absolute, 'utf8').split('SELECT ').length - 1;
    }
    expect(count).toBe(31);
  });
});

describe('native Collaboration lane 15 field-command routing read', () => {
  const asActor = (actor: HoldingActor) => actor as unknown as WorkContext;

  it('owns the route decision and the boolean response shape', async () => {
    expect(await fieldCollaborationCommand(asActor(ACME), {
      entity: 'relation',
      id: 'rel-1',
      sourceType: 'support_ticket',
      targetType: 'field_service_order',
    })).toBe(true);
    expect(await fieldCollaborationCommand(asActor(ACME), {
      id: 'card-1',
      sourceType: 'support_ticket',
    })).toBe(false);
    expect(await fieldCollaborationCommand(asActor(ACME), {
      entity: 'relation',
      sourceType: 'staff_task',
      targetType: 'field_service_order',
    })).toBe(true);
    expect(await fieldCollaborationCommand(asActor(ACME), null)).toBe(false);
    expect(requests.map(r => r.url)).toEqual([
      '/api/v1/holdings/acme/collaboration/native-lane-15/field-command',
      '/api/v1/holdings/acme/collaboration/native-lane-15/field-command',
      '/api/v1/holdings/acme/collaboration/native-lane-15/field-command',
    ]);
    expect(requests.map(r => r.host)).toEqual(Array(3).fill('acme.jesadakorn.com'));
    expect(requests.map(r => r.company)).toEqual(Array(3).fill('c1'));
    expect(requests[0].body).toEqual({
      entity: 'relation', id: 'rel-1', companyId: 'c1',
    });
  });

  it('selects every registered Holding by WorkContext holdingId+holdingSlug', async () => {
    await fieldCollaborationCommand(asActor(ACME), { id: 'card-1' });
    await fieldCollaborationCommand(asActor(NORTHSTAR), { id: 'card-1' });
    await fieldCollaborationCommand(asActor(ERPX), { id: 'card-1' });
    expect(requests.map(r => r.host)).toEqual([
      'acme.jesadakorn.com', 'northstar.jesadakorn.com', 'erpx-internal.jesadakorn.com',
    ]);
  });

  it('fails closed on an unknown or mismatched WorkContext target', async () => {
    await expect(fieldCollaborationCommand(
      { holdingId: 'h999', holdingSlug: 'acme', companyId: 'c1' } as WorkContext,
      { id: 'card-1' },
    )).rejects.toMatchObject({ status: 409, code: 'WORK_CONTEXT_CHANGED' });
    await expect(fieldCollaborationCommand(
      { holdingId: 'h002', holdingSlug: 'acme', companyId: 'c1' } as WorkContext,
      { id: 'card-1' },
    )).rejects.toMatchObject({ status: 409, code: 'WORK_CONTEXT_CHANGED' });
    expect(requests).toHaveLength(0);
  });

  it('fails closed when config is absent or the native service is down', async () => {
    delete process.env[ENV];
    await expect(fieldCollaborationCommand(asActor(ACME), { id: 'card-1' }))
      .rejects.toMatchObject({ status: 503, code: 'NATIVE_COLLABORATION_LANE15_UNAVAILABLE' });
    process.env[ENV] = JSON.stringify(DEV_CONFIG(port));
    mode = 'down';
    await expect(fieldCollaborationCommand(asActor(ACME), { id: 'card-1' }))
      .rejects.toMatchObject({ status: 503 });
  });

  it('removed every owner statement from the field-command TypeScript source', () => {
    const source = fs.readFileSync(
      path.join(root, 'packages/server-collaboration/src/adapters/field-command-owner.ts'),
      'utf8',
    );
    expect(source).not.toMatch(/SELECT /);
    expect(source).not.toMatch(/\.prepare\(/);
    expect(source).not.toMatch(/\.bind\(/);
    expect(source).toContain("from './native-lane-15/field-command-client'");
  });
});
