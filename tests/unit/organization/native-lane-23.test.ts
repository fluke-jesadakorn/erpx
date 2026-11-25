import { afterEach, describe, expect, it, vi } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { ApiError } from '@erpx/server-runtime/public/http';

vi.mock('server-only', () => ({}));

import {
  configuration,
  holdingFor,
  nativeDirectoryHierarchy,
  validateDirectoryHierarchy,
  type DirectoryHierarchy,
} from '../../../packages/server-organization/src/adapters/native-lane-23/client';

const ROOT = process.cwd();
const RETIRED_FILE = resolve(ROOT, 'packages/server-organization/src/adapters/directory-projection-facts.ts');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');

const NODES_SQL = `SELECT n.id,n.company_id,n.kind,n.slug,n.display_name,n.parent_id,n.branch_id
      FROM organization_nodes n JOIN organization_nodes c
        ON c.id=n.company_id AND c.kind='company' AND c.status='active'
      WHERE n.status='active' ORDER BY n.company_id,n.kind,n.display_name,n.id`;
const LEVELS_SQL = `SELECT l.id,l.company_id,l.code,l.name,l.level_number
      FROM staff_levels l JOIN organization_nodes c
        ON c.id=l.company_id AND c.kind='company' AND c.status='active'
      WHERE l.active=1 ORDER BY l.company_id,l.level_number NULLS LAST,l.name,l.id`;
const POSITIONS_SQL = `SELECT p.id,p.company_id,p.branch_id,p.department_id,p.title,p.level_id,
        p.reports_to_position_id,p.is_ceo
      FROM organization_positions p JOIN organization_nodes c
        ON c.id=p.company_id AND c.kind='company' AND c.status='active'
      WHERE p.active=1 ORDER BY p.company_id,p.title,p.id`;

const PAYLOAD = {
  data: {
    nodes: [
      {
        id: 'node-1',
        company_id: 'company-1',
        kind: 'company',
        slug: 'acme',
        display_name: 'Acme',
        parent_id: null,
        branch_id: null,
      },
      {
        id: 'node-2',
        company_id: 'company-1',
        kind: 'branch',
        slug: 'nyc',
        display_name: 'New York',
        parent_id: 'node-1',
        branch_id: 'node-2',
      },
    ],
    levels: [
      {
        id: 'level-1',
        company_id: 'company-1',
        code: 'L1',
        name: 'Level 1',
        level_number: 1,
      },
    ],
    positions: [
      {
        id: 'pos-1',
        company_id: 'company-1',
        branch_id: 'node-2',
        department_id: null,
        title: 'CEO',
        level_id: 'level-1',
        reports_to_position_id: null,
        is_ceo: 1,
      },
    ],
  },
};

const DEV_HOLDINGS = [
  {
    origin: 'https://acme.jesadakorn.com',
    host: 'acme.jesadakorn.com',
    holdingId: 'h001',
    holdingSlug: 'acme',
  },
  {
    origin: 'https://northstar.jesadakorn.com',
    host: 'northstar.jesadakorn.com',
    holdingId: 'h002',
    holdingSlug: 'northstar',
  },
  {
    origin: 'https://erpx-internal.jesadakorn.com',
    host: 'erpx-internal.jesadakorn.com',
    holdingId: 'h003',
    holdingSlug: 'erpx',
  },
] as const;

const DEV_CONFIG = {
  privateBaseUrl: 'http://127.0.0.1:3198',
  timeoutMs: 10000,
  holdings: DEV_HOLDINGS,
};

const NATIVE_CONFIG = JSON.stringify(DEV_CONFIG);

const previousConfig = process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
afterEach(() => {
  vi.unstubAllGlobals();
  if (previousConfig === undefined) delete process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
  else process.env.ERPX_NATIVE_ORGANIZATION_CONFIG = previousConfig;
});

describe('native lane 23 — organization directory hierarchy', () => {
  it('retired all three literal TS SELECTs and kept only the thin client', () => {
    const source = readFileSync(RETIRED_FILE, 'utf8');
    expect(source).not.toContain('SELECT ');
    expect(source).toContain('nativeDirectoryNodes(await headers())');
    expect(source).toContain('nativeDirectoryLevels(await headers())');
    expect(source).toContain('nativeDirectoryPositions(await headers())');
  });

  it('pins the rendered SQL oracle that the Rust owner now executes', () => {
    expect(sha(NODES_SQL)).toBe('3209884176699884550af2180cbd61c52e95bc0551d804b8ddd218e9596201ff');
    expect(sha(LEVELS_SQL)).toBe('3ec6b658baae8dfb915c182e229783d147d3ee30133b899a96e5e444e5641f53');
    expect(sha(POSITIONS_SQL)).toBe('00e25fe99020172a9ba538ced87e7c32eea4220f09ebe07c3ec061bf67a1fee9');
  });

  it('pins the native result shape and its oracle hash', () => {
    const facts: DirectoryHierarchy = validateDirectoryHierarchy(PAYLOAD);
    expect(facts.nodes[0]!.slug).toBe('acme');
    expect(facts.positions[0]!.is_ceo).toBe(1);
    expect(sha(JSON.stringify(facts))).toBe('f48ac71e156c9a4172977bb8e60d785ef2a5bc17b081137cb6cb36efdab12d08');
  });

  it.each([
    ['missing data wrapper', { nodes: [], levels: [], positions: [] }],
    ['unknown top-level field', { data: { ...PAYLOAD.data, extra: 1 } }],
    ['unknown row field', { data: { ...PAYLOAD.data, nodes: [{ ...PAYLOAD.data.nodes[0], extra: 1 }] } }],
    ['unknown node kind', { data: { ...PAYLOAD.data, nodes: [{ ...PAYLOAD.data.nodes[0], kind: 'other' }] } }],
    ['duplicate node ids', { data: { ...PAYLOAD.data, nodes: [PAYLOAD.data.nodes[0], PAYLOAD.data.nodes[0]] } }],
    ['out-of-range level number', { data: { ...PAYLOAD.data, levels: [{ ...PAYLOAD.data.levels[0], level_number: 9 }] } }],
    ['invalid is_ceo', { data: { ...PAYLOAD.data, positions: [{ ...PAYLOAD.data.positions[0], is_ceo: 2 }] } }],
    ['wrong scalar type', { data: { ...PAYLOAD.data, levels: [{ ...PAYLOAD.data.levels[0], code: 12 }] } }],
  ])('fails closed on %s', (_label, payload) => {
    expect(() => validateDirectoryHierarchy(payload)).toThrowError(ApiError);
    try {
      validateDirectoryHierarchy(payload);
    } catch (error) {
      expect(error).toMatchObject({ status: 503, code: 'ORGANIZATION_DIRECTORY_HIERARCHY_INVALID' });
    }
  });

  it('fails closed without a native configuration', () => {
    delete process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
    expect(() => configuration(process.env.ERPX_NATIVE_ORGANIZATION_CONFIG)).toThrowError(ApiError);
    try {
      configuration(undefined);
    } catch (error) {
      expect(error).toMatchObject({ status: 503, code: 'ORGANIZATION_DIRECTORY_HIERARCHY_UNAVAILABLE' });
    }
  });

  it('fails closed on a rejected transport with no SQL fallback', async () => {
    process.env.ERPX_NATIVE_ORGANIZATION_CONFIG = NATIVE_CONFIG;
    const fetchMock = vi.fn().mockRejectedValue(new Error('native service down'));
    vi.stubGlobal('fetch', fetchMock);
    const source = new Headers({ host: 'acme.jesadakorn.com' });
    await expect(nativeDirectoryHierarchy(source)).rejects.toMatchObject({
      status: 503,
      code: 'ORGANIZATION_DIRECTORY_HIERARCHY_UNAVAILABLE',
    });
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(String(fetchMock.mock.calls[0]![0])).toContain(
      '/api/v1/holdings/acme/organization/directory-hierarchy',
    );
  });

  it('accepts a valid native payload over the exact holding route', async () => {
    process.env.ERPX_NATIVE_ORGANIZATION_CONFIG = NATIVE_CONFIG;
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => PAYLOAD,
    });
    vi.stubGlobal('fetch', fetchMock);
    const facts = await nativeDirectoryHierarchy(new Headers({ host: 'acme.jesadakorn.com' }));
    expect(facts).toEqual(validateDirectoryHierarchy(PAYLOAD));
    const request = fetchMock.mock.calls[0]![1] as RequestInit;
    expect((request.headers as Headers).get('host')).toBe('acme.jesadakorn.com');
  });

  it('accepts 1..64 holdings and rejects empty, oversized, duplicate or mismatched entries', () => {
    const make = (count: number, override: (item: Record<string, unknown>, i: number) => Record<string, unknown> = item => item) =>
      JSON.stringify({
        ...DEV_CONFIG,
        holdings: Array.from({ length: count }, (_, i) =>
          override(
            {
              origin: `https://h${i}.example.test`,
              host: `h${i}.example.test`,
              holdingId: `id-${i}`,
              holdingSlug: `h-${i}`,
            },
            i,
          ),
        ),
      });
    expect(configuration(make(1)).holdings).toHaveLength(1);
    expect(configuration(make(64)).holdings).toHaveLength(64);
    expect(() => configuration(make(0))).toThrowError(ApiError);
    expect(() => configuration(make(65))).toThrowError(ApiError);
    expect(() => configuration(make(2, item => ({ ...item, host: 'h0.example.test' })))).toThrowError(ApiError);
    expect(() => configuration(make(2, item => ({ ...item, holdingId: 'same' })))).toThrowError(ApiError);
    expect(() => configuration(make(1, item => ({ ...item, host: 'other.example.test' })))).toThrowError(ApiError);
    expect(() => configuration(make(1, item => ({ ...item, companyId: '' })))).toThrowError(ApiError);
  });

  it('selects the exact Holding from actor identity, origin or host', () => {
    const config = configuration(NATIVE_CONFIG);
    expect(holdingFor({ holdingId: 'h003', holdingSlug: 'erpx' }, config).host).toBe('erpx-internal.jesadakorn.com');
    expect(holdingFor({ holdingId: 'h002', holdingSlug: 'northstar' }, config).host).toBe('northstar.jesadakorn.com');
    expect(holdingFor({ origin: 'https://northstar.jesadakorn.com' }, config).holdingId).toBe('h002');
    expect(holdingFor(new Headers({ host: 'acme.jesadakorn.com' }), config).holdingId).toBe('h001');
  });

  it.each([
    ['unknown actor', { holdingId: 'h999', holdingSlug: 'ghost' }],
    ['slug mismatch', { holdingId: 'h001', holdingSlug: 'northstar' }],
    ['unknown origin', { origin: 'https://evil.example.test' }],
    ['unknown host', new Headers({ host: 'evil.example.test' })],
    ['actor/origin disagreement', { holdingId: 'h001', holdingSlug: 'acme', origin: 'https://northstar.jesadakorn.com' }],
    ['empty selector', {}],
  ])('fails closed on %s without falling back to the first Holding', (_label, target) => {
    const config = configuration(NATIVE_CONFIG);
    expect(() => holdingFor(target, config)).toThrowError(ApiError);
    try {
      holdingFor(target, config);
    } catch (error) {
      expect(error).toMatchObject({ status: 503, code: 'ORGANIZATION_DIRECTORY_HIERARCHY_TARGET_INVALID' });
    }
  });

  it('routes an actor-selected Holding to its own pool path and optional company scope', async () => {
    process.env.ERPX_NATIVE_ORGANIZATION_CONFIG = JSON.stringify({
      ...DEV_CONFIG,
      holdings: DEV_HOLDINGS.map((holding, i) => ({
        ...holding,
        ...(i === 2 ? { companyId: 'erpx-company' } : {}),
      })),
    });
    const fetchMock = vi.fn().mockResolvedValue({ ok: true, status: 200, json: async () => PAYLOAD });
    vi.stubGlobal('fetch', fetchMock);
    await nativeDirectoryHierarchy(
      { holdingId: 'h003', holdingSlug: 'erpx' },
      new Headers({ cookie: 'session=x' }),
    );
    const url = String(fetchMock.mock.calls[0]![0]);
    const request = fetchMock.mock.calls[0]![1] as RequestInit;
    expect(url).toContain('/api/v1/holdings/erpx/organization/directory-hierarchy');
    expect(url).not.toContain('/holdings/acme/');
    expect((request.headers as Headers).get('host')).toBe('erpx-internal.jesadakorn.com');
    expect((request.headers as Headers).get('x-erpx-context-company')).toBe('erpx-company');
    expect((request.headers as Headers).get('cookie')).toBe('session=x');
  });
});
