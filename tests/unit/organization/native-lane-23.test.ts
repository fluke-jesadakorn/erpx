import { afterEach, describe, expect, it, vi } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { resolve } from 'node:path';
import { ApiError } from '@erpx/server-runtime/public/http';
import { canonicalDestinationLinks } from '@erpx/feature-work_projection/lib/destination-links';

vi.mock('server-only', () => ({}));
const { nativeRequest } = vi.hoisted(() => ({ nativeRequest: vi.fn() }));
vi.mock('@erpx/server-runtime/public/native-http', () => ({ nativeRequest }));

import {
  configuration,
  holdingFor,
  nativeDirectoryHierarchy,
  nativeExecutiveEvidence,
  nativeNodeReferences,
  validateDirectoryHierarchy,
  validateExecutiveEvidence,
  validateNodeReferences,
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
const previousDirectoryConfig = process.env.ERPX_NATIVE_ORGANIZATION_DIRECTORY_CONFIG;
afterEach(() => {
  vi.unstubAllGlobals();
  nativeRequest.mockReset();
  if (previousConfig === undefined) delete process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
  else process.env.ERPX_NATIVE_ORGANIZATION_CONFIG = previousConfig;
  if (previousDirectoryConfig === undefined) delete process.env.ERPX_NATIVE_ORGANIZATION_DIRECTORY_CONFIG;
  else process.env.ERPX_NATIVE_ORGANIZATION_DIRECTORY_CONFIG = previousDirectoryConfig;
});

const useSharedConfig = (raw: string = NATIVE_CONFIG): void => {
  process.env.ERPX_NATIVE_ORGANIZATION_CONFIG = raw;
  delete process.env.ERPX_NATIVE_ORGANIZATION_DIRECTORY_CONFIG;
};
const jsonResponse = (payload: unknown, status = 200): Response =>
  new Response(JSON.stringify(payload), { status, headers: { 'content-type': 'application/json' } });
type TransportCall = [URL, { method?: string; headers: Record<string, string>; body?: string }, number];
const transportCall = (index = 0): TransportCall => nativeRequest.mock.calls[index] as unknown as TransportCall;

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
    useSharedConfig();
    nativeRequest.mockRejectedValueOnce(new Error('native service down'));
    const source = new Headers({ host: 'acme.jesadakorn.com' });
    await expect(nativeDirectoryHierarchy(source)).rejects.toMatchObject({
      status: 503,
      code: 'ORGANIZATION_DIRECTORY_HIERARCHY_UNAVAILABLE',
    });
    expect(nativeRequest).toHaveBeenCalledTimes(1);
    expect(transportCall()[0].pathname).toBe('/api/v1/holdings/acme/organization/directory-hierarchy');
  });

  it('accepts a valid native payload over the exact holding route', async () => {
    useSharedConfig();
    nativeRequest.mockResolvedValueOnce(jsonResponse(PAYLOAD));
    const facts = await nativeDirectoryHierarchy(new Headers({ host: 'acme.jesadakorn.com' }));
    expect(facts).toEqual(validateDirectoryHierarchy(PAYLOAD));
    const [, init] = transportCall();
    expect(init.method).toBe('GET');
    expect(init.headers['host']).toBe('acme.jesadakorn.com');
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
    useSharedConfig(JSON.stringify({
      ...DEV_CONFIG,
      holdings: DEV_HOLDINGS.map((holding, i) => ({
        ...holding,
        ...(i === 2 ? { companyId: 'erpx-company' } : {}),
      })),
    }));
    nativeRequest.mockResolvedValueOnce(jsonResponse(PAYLOAD));
    await nativeDirectoryHierarchy(
      { holdingId: 'h003', holdingSlug: 'erpx' },
      new Headers({ cookie: 'session=x' }),
    );
    const [url, init] = transportCall();
    expect(url.pathname).toBe('/api/v1/holdings/erpx/organization/directory-hierarchy');
    expect(url.href).not.toContain('/holdings/acme/');
    expect(init.headers['host']).toBe('erpx-internal.jesadakorn.com');
    expect(init.headers['x-erpx-context-company']).toBe('erpx-company');
    expect(init.headers['cookie']).toBe('session=x');
  });

  it('prefers the directory config over the inherited shared config for lane 23', async () => {
    process.env.ERPX_NATIVE_ORGANIZATION_CONFIG = NATIVE_CONFIG;
    process.env.ERPX_NATIVE_ORGANIZATION_DIRECTORY_CONFIG = JSON.stringify({
      ...DEV_CONFIG,
      privateBaseUrl: 'http://127.0.0.1:3199',
    });
    nativeRequest.mockResolvedValueOnce(jsonResponse(PAYLOAD));
    await nativeDirectoryHierarchy(new Headers({ host: 'northstar.jesadakorn.com' }));
    const [url, init] = transportCall();
    expect(url.origin).toBe('http://127.0.0.1:3199');
    expect(init.headers['host']).toBe('northstar.jesadakorn.com');
  });

  it('fails closed on an invalid directory config without falling back to the shared config', async () => {
    useSharedConfig();
    process.env.ERPX_NATIVE_ORGANIZATION_DIRECTORY_CONFIG = '{not-json';
    await expect(nativeDirectoryHierarchy(new Headers({ host: 'acme.jesadakorn.com' }))).rejects.toMatchObject({
      status: 503,
      code: 'ORGANIZATION_DIRECTORY_HIERARCHY_CONFIG_INVALID',
    });
    expect(nativeRequest).not.toHaveBeenCalled();
  });
});

describe('native lane 23 — organization node references', () => {
  const REF_SQL = {
    audience: "SELECT id,kind,branch_id FROM organization_nodes WHERE id=? AND company_id=? AND status='active'",
    units: 'SELECT id,display_name AS name,kind,branch_id AS "branchId" FROM organization_nodes\n    WHERE company_id=? AND status=\'active\' AND kind IN (\'branch\',\'department\') ORDER BY display_name,id',
    attachment: "SELECT company_id,CASE WHEN kind='branch' THEN id ELSE branch_id END branch_id,NULL::text department_id,hierarchy_revision AS version,kind\n      FROM organization_nodes WHERE id=? AND company_id=? AND kind IN ('company','headquarters','branch','department')",
  };

  const REF_PAYLOAD = {
    data: {
      node: { id: 'node-1', kind: 'branch', branch_id: 'node-1' },
      units: [{ id: 'node-1', name: 'Branch', kind: 'branch', branchId: null }],
      attachment: {
        company_id: 'company-1',
        branch_id: 'node-1',
        department_id: null,
        version: 3,
        kind: 'branch',
      },
    },
  };

  it('retired all three node-reference literals and kept only the client', () => {
    for (const [name, file] of [
      ['audience', 'collaboration-inbox-sources.ts'],
      ['units', 'iam-unit-facts.ts'],
      ['attachment', 'attachments-finance-facts.ts'],
    ] as const) {
      const source = readFileSync(resolve(ROOT, 'packages/server-organization/src/adapters', file), 'utf8');
      expect(source).not.toContain(REF_SQL[name]);
      expect(source).toContain('nativeNodeReferences');
    }
  });

  it('pins the retired node-reference SQL oracles', () => {
    expect(sha(REF_SQL.audience)).toBe('f735e1687f8991d098fb440a9b42ac848db5e518ba2d55897bffc2b07c814f09');
    expect(sha(REF_SQL.units)).toBe('4487d2a30b78b49145077dc6003ffbbd729d64b0f72dbc66238724cfb903f336');
    expect(sha(REF_SQL.attachment)).toBe('1a550056dc4a644104b5b7fadcdfd2581362ad20ae24a30bfad8a7914021e7c3');
  });

  it('strictly projects the native node-reference payload', () => {
    const facts = validateNodeReferences(REF_PAYLOAD);
    expect(facts.node?.id).toBe('node-1');
    expect(facts.units[0]!.branchId).toBeNull();
    expect(facts.attachment?.version).toBe(3);
  });

  it.each([
    ['bad unit camelCase', { data: { ...REF_PAYLOAD.data, units: [{ ...REF_PAYLOAD.data.units[0], branch_id: null }] } }],
    ['duplicate unit ids', { data: { ...REF_PAYLOAD.data, units: [REF_PAYLOAD.data.units[0], REF_PAYLOAD.data.units[0]] } }],
    ['bad attachment version', { data: { ...REF_PAYLOAD.data, attachment: { ...REF_PAYLOAD.data.attachment, version: 0 } } }],
    ['missing attachment field', { data: { ...REF_PAYLOAD.data, attachment: { company_id: 'c1' } } }],
    ['unknown field', { data: { ...REF_PAYLOAD.data, extra: true } }],
  ])('fails closed on %s', (_label, payload) => {
    expect(() => validateNodeReferences(payload)).toThrowError(ApiError);
    try {
      validateNodeReferences(payload);
    } catch (error) {
      expect(error).toMatchObject({ status: 503, code: 'ORGANIZATION_DIRECTORY_HIERARCHY_INVALID' });
    }
  });

  it('fails closed on an invalid reference scope with no TS fallback', async () => {
    useSharedConfig();
    await expect(
      nativeNodeReferences(
        { holdingId: 'h001', holdingSlug: 'acme' },
        { companyId: 'bad id', nodeId: 'node-1' },
      ),
    ).rejects.toMatchObject({ status: 503, code: 'ORGANIZATION_DIRECTORY_HIERARCHY_INVALID' });
    expect(nativeRequest).not.toHaveBeenCalled();
  });

  it('routes the reference read to the selected Holding with bounded query', async () => {
    useSharedConfig();
    nativeRequest.mockResolvedValueOnce(jsonResponse(REF_PAYLOAD));
    const facts = await nativeNodeReferences(
      { holdingId: 'h002', holdingSlug: 'northstar' },
      { companyId: 'company-1', nodeId: 'node-1' },
    );
    expect(facts).toEqual(validateNodeReferences(REF_PAYLOAD));
    const [url, init] = transportCall();
    expect(`${url.pathname}${url.search}`).toBe(
      '/api/v1/holdings/northstar/organization/node-references?companyId=company-1&nodeId=node-1',
    );
    expect(init.headers['host']).toBe('northstar.jesadakorn.com');
  });
});

describe('native lane 23 — organization executive evidence', () => {
  const EVIDENCE_SQL = {
    connection:
      `SELECT n.id,n.kind,n.display_name,n.branch_id,b.display_name branch_name
    FROM organization_nodes n LEFT JOIN organization_nodes b ON b.id=n.branch_id AND b.company_id=n.company_id AND b.status='active'
    WHERE n.company_id=? AND n.status='active' AND n.kind IN ('company','branch','department')
      AND (n.branch_id IS NULL OR b.id IS NOT NULL)
      AND (?::text IS NULL OR n.id=? OR n.branch_id=?)
      AND (?::text IS NULL OR n.id=?) ORDER BY n.kind,n.display_name,n.id`,
    contextNodes:
      `SELECT id,kind,display_name,parent_id,branch_id FROM organization_nodes WHERE company_id=? AND status='active' AND (kind='company' OR ((?::text IS NULL OR id=? OR branch_id=?) AND (?::text IS NULL OR id=? OR (kind='branch' AND id=(SELECT branch_id FROM organization_nodes WHERE id=? AND company_id=?))))) ORDER BY kind,display_name,id LIMIT 501`,
    contextPositions:
      `SELECT p.id,p.title,p.branch_id,p.department_id,p.level_id,p.reports_to_position_id,l.level_number FROM organization_positions p JOIN staff_levels l ON l.id=p.level_id AND l.company_id=p.company_id WHERE p.company_id=? AND p.active=1 AND l.active=1 AND (?::text IS NULL OR p.branch_id=? OR p.branch_id IS NULL) AND (?::text IS NULL OR p.department_id=? OR p.department_id IS NULL) ORDER BY l.level_number,p.title,p.id LIMIT 501`,
    nodeEvidence:
      `SELECT n.id,n.kind,n.slug,n.display_name,n.status,n.hierarchy_revision,p.display_name parent_name,
  b.display_name branch_name FROM organization_nodes n
  LEFT JOIN organization_nodes p ON p.id=n.parent_id AND p.company_id=n.company_id
  LEFT JOIN organization_nodes b ON b.id=CASE WHEN n.kind='branch' THEN n.id ELSE n.branch_id END AND b.company_id=n.company_id
  WHERE n.company_id=? AND n.kind IN ('branch','department') AND n.status='active'
    AND (?::text IS NULL OR n.id=? OR n.branch_id=?) AND (?::text IS NULL OR n.id=?)`,
    pending:
      `SELECT c.id,c.action,c.node_kind,c.display_name,c.state,c.version,c.created_at,c.updated_at,
  COALESCE(b.display_name,'Company-wide') branch_name FROM organization_node_changes c
  LEFT JOIN organization_nodes b ON b.id=c.branch_id AND b.company_id=c.company_id
  WHERE c.company_id=? AND c.state='submitted' AND (?::text IS NULL OR c.branch_id=?)`,
  };
  const EVIDENCE_FILES = [
    ['connection', 'executive-connection-facts.ts'],
    ['contextNodes', 'executive-context-sources.ts'],
    ['contextPositions', 'executive-context-sources.ts'],
    ['nodeEvidence', 'executive-main-evidence-sources.ts'],
    ['pending', 'executive-main-evidence-sources.ts'],
  ] as const;

  const EVIDENCE_PAYLOAD = { data: { rows: [{ id: 'node-1', kind: 'branch', display_name: 'Branch' }] } };

  it('retired every executive evidence SQL literal and kept only the client', () => {
    const sources = new Map<string, string>();
    for (const file of new Set(EVIDENCE_FILES.map(([, name]) => name))) {
      sources.set(file, readFileSync(resolve(ROOT, 'packages/server-organization/src/adapters', file), 'utf8'));
    }
    for (const [key, file] of EVIDENCE_FILES) {
      expect(sources.get(file)).not.toContain(EVIDENCE_SQL[key]);
      expect(sources.get(file)).toContain('nativeExecutiveEvidence');
    }
  });

  it('pins the retired executive evidence SQL oracles', () => {
    expect(sha(EVIDENCE_SQL.connection)).toBe('270124c0154df0b1f42c8a76baa46168719ff009c88362a3ea46ed875af54f4a');
    expect(sha(EVIDENCE_SQL.contextNodes)).toBe('217fcab9daf0f888e232b67e858e8536754a0e5a839b0a21664e969494dd6170');
    expect(sha(EVIDENCE_SQL.contextPositions)).toBe('13d40c34f8423ea329375fac2892fc96d31f56b7687a4416100393ba5979ff49');
    expect(sha(EVIDENCE_SQL.nodeEvidence)).toBe('1f2cc5c5dff8a615f55fdfd0d0334c75006d0e69f856f56ec9e121f1c5ae4f0b');
    expect(sha(EVIDENCE_SQL.pending)).toBe('00ac378a2a99356eb5a98f4b6ff6ad020baa19dc2c9e04fab09f1b0d189664f5');
  });

  it('strictly projects the native evidence payload', () => {
    expect(validateExecutiveEvidence(EVIDENCE_PAYLOAD).rows).toHaveLength(1);
    expect(() => validateExecutiveEvidence({ data: { rows: [{}] } })).toThrowError(ApiError);
    expect(() => validateExecutiveEvidence({ data: { rows: [], extra: true } })).toThrowError(ApiError);
  });

  it('fails closed on an invalid evidence request with no TS fallback', async () => {
    useSharedConfig();
    await expect(
      nativeExecutiveEvidence({ holdingId: 'h001', holdingSlug: 'acme' }, {
        kind: 'node-evidence', companyId: 'company-1', limit: 501,
      }),
    ).rejects.toMatchObject({ status: 503, code: 'ORGANIZATION_DIRECTORY_HIERARCHY_INVALID' });
    await expect(
      nativeExecutiveEvidence({ holdingId: 'h001', holdingSlug: 'acme' }, {
        kind: 'unsupported' as 'node-evidence', companyId: 'company-1',
      }),
    ).rejects.toMatchObject({ status: 503, code: 'ORGANIZATION_DIRECTORY_HIERARCHY_INVALID' });
    expect(nativeRequest).not.toHaveBeenCalled();
  });

  it('posts a bounded evidence body to the selected Holding', async () => {
    useSharedConfig();
    nativeRequest.mockResolvedValueOnce(jsonResponse(EVIDENCE_PAYLOAD));
    const facts = await nativeExecutiveEvidence(
      { holdingId: 'h002', holdingSlug: 'northstar' },
      { kind: 'node-evidence', companyId: 'company-1', values: [null, 'node-1'], nodeId: 'node-2' },
    );
    expect(facts.rows[0]!.id).toBe('node-1');
    const [url, init] = transportCall();
    expect(url.pathname).toBe('/api/v1/holdings/northstar/organization/executive-evidence');
    expect(init.method).toBe('POST');
    expect(init.headers['content-type']).toBe('application/json');
    expect(JSON.parse(init.body!)).toEqual({
      kind: 'node-evidence', companyId: 'company-1', values: [null, 'node-1'], nodeId: 'node-2',
    });
    expect(init.headers['host']).toBe('northstar.jesadakorn.com');
  });

  it('preserves the exact Host, cookie, method and JSON body over a real Node loopback transport', async () => {
    const seen: { host?: string; cookie?: string; method?: string; url?: string; body: string }[] = [];
    const server = createServer((req, res) => {
      let body = '';
      req.setEncoding('utf8');
      req.on('data', chunk => { body += chunk; });
      req.on('end', () => {
        seen.push({ host: req.headers.host, cookie: req.headers.cookie, method: req.method, url: req.url, body });
        res.writeHead(200, { 'content-type': 'application/json' });
        res.end(JSON.stringify(EVIDENCE_PAYLOAD));
      });
    });
    await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
    try {
      const address = server.address();
      if (!address || typeof address === 'string') throw new Error('missing loopback port');
      process.env.ERPX_NATIVE_ORGANIZATION_DIRECTORY_CONFIG = JSON.stringify({
        privateBaseUrl: `http://127.0.0.1:${address.port}`,
        timeoutMs: 2000,
        holdings: DEV_HOLDINGS,
      });
      delete process.env.ERPX_NATIVE_ORGANIZATION_CONFIG;
      const actual = await vi.importActual<typeof import('@erpx/server-runtime/public/native-http')>(
        '@erpx/server-runtime/public/native-http',
      );
      nativeRequest.mockImplementationOnce((...args) =>
        actual.nativeRequest(...(args as Parameters<typeof actual.nativeRequest>)),
      );

      const facts = await nativeExecutiveEvidence(
        new Headers({ host: 'northstar.jesadakorn.com', cookie: 'session=loopback', origin: 'https://northstar.jesadakorn.com' }),
        { kind: 'node-evidence', companyId: 'company-1', values: [null, 'node-1'] },
      );
      expect(facts.rows[0]!.id).toBe('node-1');
      expect(seen).toHaveLength(1);
      expect(seen[0]).toMatchObject({
        host: 'northstar.jesadakorn.com',
        cookie: 'session=loopback',
        method: 'POST',
        url: '/api/v1/holdings/northstar/organization/executive-evidence',
      });
      expect(JSON.parse(seen[0]!.body)).toEqual({
        kind: 'node-evidence', companyId: 'company-1', values: [null, 'node-1'],
      });
    } finally {
      await new Promise<void>((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
    }
  });
});

describe('native holding people directory — payload parity and retirement', () => {
  /** Retired `HoldingPeopleDirectory` output from the TypeScript route. Field
   * order is preserved so serialization can be compared byte for byte with
   * `erpx-server-people_payroll::adapters::holding_directory`. */
  const fixture = {
    companies: [{ id: 'c1', slug: 'acme', name: 'Acme' }],
    nodes: [{ id: 'n1', companyId: 'c1', kind: 'company', slug: 'acme', name: 'Acme HQ', parentId: null, branchId: null }],
    levels: [{ id: 'l1', companyId: 'c1', code: 'L1', name: 'One', number: 1 }],
    positions: [{ id: 'p1', companyId: 'c1', branchId: null, departmentId: null, title: 'CEO', levelId: 'l1', reportsToPositionId: null, isCeo: true }],
    people: [{ id: 'e1', personId: 'person-1', userId: 'user-1', engagementId: 'e1', companyId: 'c1', branchId: null, departmentId: null, positionId: 'p1', legalName: 'Ada Lovelace', jobTitle: 'CEO', workEmail: 'ada@example.test', workPhone: null }],
  };

  it('returns the exact retired TypeScript payload and oracle hash', () => {
    const body = JSON.stringify(canonicalDestinationLinks({ data: fixture }));
    expect(body).toBe('{"data":{"companies":[{"id":"c1","slug":"acme","name":"Acme"}],"nodes":[{"id":"n1","companyId":"c1","kind":"company","slug":"acme","name":"Acme HQ","parentId":null,"branchId":null}],"levels":[{"id":"l1","companyId":"c1","code":"L1","name":"One","number":1}],"positions":[{"id":"p1","companyId":"c1","branchId":null,"departmentId":null,"title":"CEO","levelId":"l1","reportsToPositionId":null,"isCeo":true}],"people":[{"id":"e1","personId":"person-1","userId":"user-1","engagementId":"e1","companyId":"c1","branchId":null,"departmentId":null,"positionId":"p1","legalName":"Ada Lovelace","jobTitle":"CEO","workEmail":"ada@example.test","workPhone":null}]}}');
    expect(sha(body)).toBe('7fdd3261202ff75d9822102d8d7e7ecba59990a6793a542342ee564804a101dc');
  });

  it('retains no TypeScript SELECT for the directory execution path', () => {
    const client = readFileSync(resolve(ROOT, 'packages/server-organization/src/adapters/holding-people-directory.ts'), 'utf8');
    expect(client).not.toContain('SELECT');
    expect(client).not.toContain('.prepare(');
    expect(client).not.toContain('readHoldingPeopleDirectory');
    expect(client).toContain('people-public-directory');

    const repository = readFileSync(resolve(ROOT, 'packages/server-people_payroll/src/adapters/people/directory-repository.ts'), 'utf8');
    expect(repository).not.toContain('readHoldingPeopleDirectory');
    expect(repository).not.toContain('readDirectoryFacts6');

    const application = readFileSync(resolve(ROOT, 'packages/server-people_payroll/src/application/directory-projection.ts'), 'utf8');
    expect(application).not.toContain('readHoldingPeopleDirectory');

    const facts = readFileSync(RETIRED_FILE, 'utf8');
    expect(facts).not.toContain('activeCompanyNodes');
    expect(facts).not.toContain('SELECT ');

    const route = readFileSync(resolve(ROOT, 'packages/workplace/src/routes/api/v1/holdings/[holdingSlug]/people-public-directory/route.ts'), 'utf8');
    expect(route).not.toContain('SELECT');
  });

  it('pins the Rust owner SQL and its native route', () => {
    const postgres = readFileSync(resolve(ROOT, 'packages/server-people_payroll/src/adapters/holding_directory/postgres.rs'), 'utf8');
    expect(postgres).toContain('pub const PEOPLE_SQL');
    expect(postgres).toContain('p.merged_into_id IS NULL');
    expect(postgres).toContain('sqlx::query(PEOPLE_SQL)');
    expect(postgres).toContain('sqlx::query(ASSIGNMENT_ACTIVE_SQL)');
    expect(postgres).toContain('a.scope_kind=$5');

    const http = readFileSync(resolve(ROOT, 'packages/server-people_payroll/src/adapters/holding_directory/http.rs'), 'utf8');
    expect(http).toContain('/api/v1/holdings/{holding_slug}/people-public-directory');
    expect(http).toContain('work_scope');
  });
});
