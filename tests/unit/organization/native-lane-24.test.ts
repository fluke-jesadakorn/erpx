import { createServer, type Server } from 'node:http';
import { afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import {
  configuration,
  holdingFor,
  LANE24_EVAL_LEVEL_ACTIVE,
  LANE24_EVAL_MARKET_LEVELS,
  LANE24_EVAL_POSITION_BY_LEVEL,
  LANE24_EVAL_WORKSPACE_LEVELS,
  LANE24_EVAL_WORKSPACE_POSITIONS,
  LANE24_ORG_ACCESS_NODES,
  nativeLane24AccessNodes,
  nativeLane24LevelActive,
  nativeLane24MarketLevels,
  nativeLane24PositionByLevel,
  nativeLane24WorkspaceLevels,
  nativeLane24WorkspacePositions,
  validateFacts,
  type Lane24Target,
} from '../../../packages/server-organization/src/adapters/native-lane-24/client';
import { peopleEvaluationOrganizationFacts } from '../../../packages/server-organization/src/adapters/people-evaluation-facts';
import { peopleAccessNodes } from '../../../packages/server-organization/src/adapters/people-access-nodes';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

const HOLDINGS = [
  { origin: 'https://acme.jesadakorn.com', host: 'acme.jesadakorn.com', holdingId: 'h001', holdingSlug: 'acme', companyId: 'acme-company' },
  { origin: 'https://northstar.jesadakorn.com', host: 'northstar.jesadakorn.com', holdingId: 'h002', holdingSlug: 'northstar', companyId: 'northstar-company' },
  { origin: 'https://erpx-internal.jesadakorn.com', host: 'erpx-internal.jesadakorn.com', holdingId: 'h003', holdingSlug: 'erpx', companyId: 'erpx-company' },
];
const CONFIG = 'ERPX_NATIVE_ORGANIZATION_LANE24_CONFIG';
const config = (base: string, holdings: unknown = HOLDINGS) => JSON.stringify({
  privateBaseUrl: base, timeoutMs: 1000, holdings,
});
const acme: Lane24Target = { holdingId: 'h001', holdingSlug: 'acme' };

type Capture = { headers: Record<string, unknown>; url?: string; body: string };
let server: Server | undefined;
afterEach(() => {
  server?.close();
  server = undefined;
  vi.restoreAllMocks();
  delete process.env[CONFIG];
});

async function listen(handler: (req: Capture) => { status: number; value: unknown }): Promise<string> {
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
  await new Promise<void>(resolve => server!.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('no listener');
  return `http://127.0.0.1:${address.port}`;
}

const data = (value: Record<string, unknown>) => ({ data: value });
const codeOf = (run: () => unknown): string | undefined => {
  try { run(); } catch (error) { return (error as { code?: string }).code; }
  return undefined;
};

describe('native lane-24 organization transport', () => {
  it('fails closed when the native transport is not configured or misconfigured', async () => {
    await expect(nativeLane24MarketLevels(acme, 'acme-company')).rejects.toMatchObject({
      status: 503, code: 'ORGANIZATION_LANE24_UNAVAILABLE',
    });
    process.env[CONFIG] = JSON.stringify({ privateBaseUrl: 'https://example.com', timeoutMs: 5, holdings: [] });
    await expect(nativeLane24MarketLevels(acme, 'acme-company')).rejects.toMatchObject({
      status: 503, code: 'ORGANIZATION_LANE24_CONFIG_INVALID',
    });
  });

  it('resolves one holding by WorkContext or browser headers with no default', () => {
    const settings = configuration(config('http://127.0.0.1:3198'));
    expect(settings.holdings).toHaveLength(3);
    expect(holdingFor({ holdingId: 'h002', holdingSlug: 'northstar' }, settings).host).toBe('northstar.jesadakorn.com');
    expect(holdingFor(new Headers({ origin: 'https://erpx-internal.jesadakorn.com' }), settings).holdingSlug).toBe('erpx');
    expect(holdingFor(new Headers({ host: 'acme.jesadakorn.com:443' }), settings).holdingId).toBe('h001');
    expect(holdingFor({ host: 'northstar.jesadakorn.com' }, settings).holdingId).toBe('h002');
    expect(codeOf(() => holdingFor({}, settings))).toBe('ORGANIZATION_LANE24_TARGET_INVALID');
    expect(codeOf(() => holdingFor(new Headers(), settings))).toBe('ORGANIZATION_LANE24_TARGET_INVALID');
    expect(codeOf(() => holdingFor({ holdingId: 'h404', holdingSlug: 'ghost' }, settings))).toBe('ORGANIZATION_LANE24_TARGET_INVALID');
    expect(codeOf(() => holdingFor({ holdingId: 'h001', holdingSlug: 'acme', origin: 'https://northstar.jesadakorn.com' }, settings))).toBe('ORGANIZATION_LANE24_TARGET_INVALID');
  });

  it('accepts the exact development env JSON', () => {
    const raw = '{"privateBaseUrl":"http://127.0.0.1:3198","timeoutMs":10000,"holdings":[' +
      '{"origin":"https://acme.jesadakorn.com","host":"acme.jesadakorn.com","holdingId":"h001","holdingSlug":"acme"},' +
      '{"origin":"https://northstar.jesadakorn.com","host":"northstar.jesadakorn.com","holdingId":"h002","holdingSlug":"northstar"},' +
      '{"origin":"https://erpx-internal.jesadakorn.com","host":"erpx-internal.jesadakorn.com","holdingId":"h003","holdingSlug":"erpx"}' +
      ']}';
    const settings = configuration(raw);
    expect(settings.base.origin).toBe('http://127.0.0.1:3198');
    expect(settings.timeout).toBe(10000);
    expect(settings.holdings.map(holding => holding.holdingId)).toEqual(['h001', 'h002', 'h003']);
    expect(holdingFor({ holdingId: 'h002', holdingSlug: 'northstar' }, settings).host)
      .toBe('northstar.jesadakorn.com');
  });

  it('rejects invalid 1..64 holding configurations', () => {
    const invalid = 'ORGANIZATION_LANE24_CONFIG_INVALID';
    expect(codeOf(() => configuration(config('http://127.0.0.1:3198', [])))).toBe(invalid);
    expect(codeOf(() => configuration(config('http://127.0.0.1:3198', Array(65).fill(HOLDINGS[0]))))).toBe(invalid);
    expect(codeOf(() => configuration(config('http://127.0.0.1:3198', [{ ...HOLDINGS[0], origin: 'http://acme.jesadakorn.com' }])))).toBe(invalid);
    expect(codeOf(() => configuration(config('http://127.0.0.1:3198', [{ ...HOLDINGS[0], holdingSlug: 'Acme' }])))).toBe(invalid);
    const dup = [...HOLDINGS, { ...HOLDINGS[0], holdingId: 'h004' }];
    expect(codeOf(() => configuration(config('http://127.0.0.1:3198', dup)))).toBe(invalid);
  });

  it('sends only kind, Company scope and bounded ids, selected from the holding', async () => {
    const seen: Capture[] = [];
    const replies: Record<string, Record<string, unknown>> = {
      [LANE24_EVAL_POSITION_BY_LEVEL]: { kind: LANE24_EVAL_POSITION_BY_LEVEL, row: { id: 'p1' } },
      [LANE24_EVAL_LEVEL_ACTIVE]: { kind: LANE24_EVAL_LEVEL_ACTIVE, exists: true },
      [LANE24_EVAL_WORKSPACE_POSITIONS]: { kind: LANE24_EVAL_WORKSPACE_POSITIONS, rows: [{ id: 'p1', title: 'Engineer', level_id: 'l1' }] },
      [LANE24_EVAL_WORKSPACE_LEVELS]: { kind: LANE24_EVAL_WORKSPACE_LEVELS, rows: [{ id: 'l1', name: 'L1' }] },
      [LANE24_EVAL_MARKET_LEVELS]: { kind: LANE24_EVAL_MARKET_LEVELS, rows: [{ id: 'l1' }] },
      [LANE24_ORG_ACCESS_NODES]: { kind: LANE24_ORG_ACCESS_NODES, rows: [{ id: 'n1', parent_id: null, kind: 'branch', display_name: 'Main', status: 'active' }] },
    };
    vi.spyOn(globalThis, 'fetch').mockImplementation(async (input, init) => {
      seen.push({
        url: String(input),
        headers: Object.fromEntries(new Headers(init?.headers as HeadersInit | undefined).entries()),
        body: String(init?.body ?? ''),
      });
      const kind = (JSON.parse(String(init?.body)) as { kind: string }).kind;
      return new Response(JSON.stringify(data(replies[kind]!)), {
        status: 200, headers: { 'content-type': 'application/json' },
      });
    });
    process.env[CONFIG] = config('http://127.0.0.1:3198');
    await expect(nativeLane24PositionByLevel(acme, 'acme-company', 'p1', 'l1')).resolves.toEqual({ id: 'p1' });
    await expect(nativeLane24LevelActive(acme, 'acme-company', 'l1')).resolves.toEqual({ one: 1 });
    await expect(nativeLane24WorkspacePositions(acme, 'acme-company')).resolves.toEqual([{ id: 'p1', title: 'Engineer', level_id: 'l1' }]);
    await expect(nativeLane24WorkspaceLevels(acme, 'acme-company')).resolves.toEqual([{ id: 'l1', name: 'L1' }]);
    await expect(nativeLane24MarketLevels(acme, 'acme-company')).resolves.toEqual([{ id: 'l1' }]);
    await expect(nativeLane24AccessNodes(acme, 'acme-company')).resolves.toEqual([
      { id: 'n1', parent_id: null, kind: 'branch', display_name: 'Main', status: 'active' },
    ]);
    expect(seen).toHaveLength(6);
    for (const req of seen) {
      expect(new URL(req.url!).pathname).toBe('/api/v1/holdings/acme/organization/lane24/evaluation-facts');
      expect(req.headers.host).toBe('acme.jesadakorn.com');
      expect(req.headers.origin).toBe('https://acme.jesadakorn.com');
      expect(req.headers['x-erpx-context-company']).toBe('acme-company');
      expect(Object.keys(JSON.parse(req.body) as object).sort()).toEqual(['kind', 'values']);
    }
    expect(JSON.parse(seen[0]!.body)).toEqual({ kind: LANE24_EVAL_POSITION_BY_LEVEL, values: ['p1', 'l1'] });
    expect(JSON.parse(seen[5]!.body)).toEqual({ kind: LANE24_ORG_ACCESS_NODES, values: [] });
  });

  it('routes a different WorkContext to its own registered holding', async () => {
    let url = '';
    vi.spyOn(globalThis, 'fetch').mockImplementation(async (input) => {
      url = String(input);
      return new Response(JSON.stringify(data({ kind: LANE24_EVAL_MARKET_LEVELS, rows: [] })), {
        status: 200, headers: { 'content-type': 'application/json' },
      });
    });
    process.env[CONFIG] = config('http://127.0.0.1:3198');
    await nativeLane24MarketLevels({ holdingId: 'h003', holdingSlug: 'erpx' }, 'erpx-company');
    expect(url).toContain('/api/v1/holdings/erpx/organization/lane24/evaluation-facts');
  });

  it('returns null for an absent row and false for an absent level', async () => {
    const base = await listen(req => {
      const kind = (JSON.parse(req.body) as { kind: string }).kind;
      const value = kind === LANE24_EVAL_POSITION_BY_LEVEL ? { kind, row: null } : { kind, exists: false };
      return { status: 200, value: data(value) };
    });
    process.env[CONFIG] = config(base);
    await expect(nativeLane24PositionByLevel(acme, 'acme-company', 'p1', 'l1')).resolves.toBeNull();
    await expect(nativeLane24LevelActive(acme, 'acme-company', 'l1')).resolves.toBeNull();
  });

  it('rejects a mismatched or malformed payload', async () => {
    const badReplies: unknown[] = [
      { data: { kind: LANE24_EVAL_MARKET_LEVELS, rows: [{ id: 1 }] } },
      { data: { kind: LANE24_EVAL_MARKET_LEVELS, rows: [{ id: 'l1' }], exists: true } },
      { data: { kind: 'drop_table', rows: [] } },
      { data: { kind: LANE24_ORG_ACCESS_NODES, rows: [{ id: 'n1', parent_id: 7, kind: 'branch', display_name: null, status: 'active' }] } },
      { data: null },
    ];
    for (const value of badReplies) {
      const base = await listen(() => ({ status: 200, value }));
      process.env[CONFIG] = config(base);
      expect(codeOf(() => validateFacts(value, LANE24_EVAL_MARKET_LEVELS))).toBe('ORGANIZATION_LANE24_INVALID');
      await expect(nativeLane24MarketLevels(acme, 'acme-company')).rejects.toMatchObject({ status: 503 });
      server?.close();
      server = undefined;
    }
  });

  it('fails closed on a non-200 reply and on a transport error', async () => {
    const base = await listen(() => ({ status: 503, value: { error: { code: 'X' } } }));
    process.env[CONFIG] = config(base);
    await expect(nativeLane24MarketLevels(acme, 'acme-company')).rejects.toMatchObject({ status: 503 });
    server?.close();
    server = undefined;
    process.env[CONFIG] = config('http://127.0.0.1:1');
    await expect(nativeLane24MarketLevels(acme, 'acme-company')).rejects.toMatchObject({ status: 503 });
  });

  it('keeps owner-local adapters thin and selects the holding from an explicit actor', async () => {
    const actor = { holdingId: 'h001', holdingSlug: 'acme', companyId: 'acme-company' };
    const base = await listen(() => ({ status: 200, value: data({ kind: LANE24_EVAL_MARKET_LEVELS, rows: [{ id: 'l1' }] }) }));
    process.env[CONFIG] = config(base);
    const facts = peopleEvaluationOrganizationFacts({} as SqlDatabase, actor);
    await expect(facts.readImportEvaluationMarketLevels('acme-company')).resolves.toEqual({
      results: [{ id: 'l1' }], success: true, meta: { changes: 0, duration: 0 },
    });
    server?.close();

    const base2 = await listen(() => ({ status: 200, value: data({ kind: LANE24_ORG_ACCESS_NODES, rows: [{ id: 'n1', parent_id: null, kind: 'branch', display_name: 'Main', status: 'active' }] }) }));
    process.env[CONFIG] = config(base2);
    await expect(peopleAccessNodes({} as SqlDatabase, 'acme-company', actor)).resolves.toEqual([
      { id: 'n1', parent_id: null, kind: 'branch', display_name: 'Main', status: 'active' },
    ]);
    server?.close();

    // No ambient target: the adapters fail closed instead of reading request state.
    process.env[CONFIG] = config(base2);
    await expect(peopleAccessNodes({} as SqlDatabase, 'acme-company')).rejects.toMatchObject({
      status: 503, code: 'ORGANIZATION_LANE24_TARGET_INVALID',
    });
  });
});
