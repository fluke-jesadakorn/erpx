import { createServer, type Server } from 'node:http';
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { afterEach, describe, expect, it, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import {
  nativeLane28StaffLevels,
  nativeLane28LevelHistory,
  nativeLane28Configured,
  type NativeStaffLevelFacts,
} from '../../../packages/server-operations/src/adapters/native-lane-28/client';

const host = 'acme.example.test';
const origin = `https://${host}`;
const company = 'company';
const ENV = 'ERPX_NATIVE_OPERATIONS_LANE28_CONFIG';
const path = '/api/v1/holdings/acme/operations/lane28/staff-levels';

const config = (base: string, companyId = company) =>
  JSON.stringify({
    privateBaseUrl: base,
    timeoutMs: 1000,
    holdings: [
      { origin, host, holdingId: 'h001', holdingSlug: 'acme', companyId },
      {
        origin: 'https://northstar.example.test',
        host: 'northstar.example.test',
        holdingId: 'h002',
        holdingSlug: 'northstar',
        companyId: 'northstar-company',
      },
      {
        origin: 'https://erpx-internal.example.test',
        host: 'erpx-internal.example.test',
        holdingId: 'h003',
        holdingSlug: 'erpx',
        companyId: 'erpx-company',
      },
    ],
  });

let server: Server | undefined;
afterEach(() => {
  server?.close();
  server = undefined;
  delete process.env[ENV];
});

async function listen(
  handler: (req: { headers: Record<string, unknown>; url?: string }) => {
    status: number;
    value: unknown;
  },
): Promise<string> {
  server = createServer((req, res) => {
    const result = handler({ headers: req.headers, url: req.url });
    res.writeHead(result.status, { 'content-type': 'application/json' });
    res.end(JSON.stringify(result.value));
  });
  await new Promise<void>((resolve) => server!.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  if (!address || typeof address === 'string') throw new Error('no listener');
  return `http://127.0.0.1:${address.port}`;
}

const facts: NativeStaffLevelFacts = {
  levels: [
    { id: 'company-l1', rank: 1, levelNumber: 1 },
    { id: 'company-l2', rank: 2, levelNumber: 6 },
  ],
  activeCount: 2,
};

describe('native Operations lane-28 staff-level facts transport', () => {
  it('fails closed when the native owner transport is not configured', async () => {
    delete process.env[ENV];
    expect(nativeLane28Configured()).toBe(false);
    await expect(nativeLane28StaffLevels(company)).rejects.toMatchObject({
      status: 503,
      code: 'OPERATIONS_LANE28_CONFIGURATION_REQUIRED',
    });
  });

  it('fails closed on a malformed native owner configuration', async () => {
    process.env[ENV] = config('http://127.0.0.1:3100');
    await expect(nativeLane28StaffLevels(company)).rejects.toMatchObject({
      status: 503,
      code: 'OPERATIONS_LANE28_CONFIGURATION_INVALID',
    });
  });

  it('pins the response shape and the scoped request, with no SQL on the wire', async () => {
    let request: { headers: Record<string, unknown>; url?: string } | undefined;
    const base = await listen((req) => {
      request = req;
      return { status: 200, value: { data: facts } };
    });
    process.env[ENV] = config(base);
    await expect(nativeLane28StaffLevels(company)).resolves.toEqual(facts);
    expect(request?.url).toBe(path);
    expect(request?.headers).toMatchObject({
      origin,
      accept: 'application/json',
      'x-erpx-context-company': company,
    });
    expect(JSON.stringify(request)).not.toContain('SELECT');
  });

  it('pins the level-history response shape and scoped engagement header', async () => {
    const rows = [
      { created_at: 100, reason: 'promotion', level_name: 'Level 2', previous_level_name: 'Level 1' },
    ];
    let request: { headers: Record<string, unknown>; url?: string } | undefined;
    const base = await listen((req) => {
      request = req;
      return { status: 200, value: { data: { history: rows } } };
    });
    process.env[ENV] = config(base);
    await expect(nativeLane28LevelHistory('eng:1', undefined, company)).resolves.toEqual(rows);
    expect(request?.url).toBe('/api/v1/holdings/acme/operations/lane28/staff-level-history');
    expect(request?.headers).toMatchObject({ 'x-erpx-context-engagement': 'eng:1' });
  });

  it('selects the Holding from the caller WorkContext, not the first holding', async () => {
    let request: { headers: Record<string, unknown>; url?: string } | undefined;
    const base = await listen((req) => {
      request = req;
      return { status: 200, value: { data: facts } };
    });
    process.env[ENV] = config(base);
    await expect(
      nativeLane28StaffLevels('northstar-company', {
        holdingId: 'h002',
        holdingSlug: 'northstar',
        companyId: 'northstar-company',
      }),
    ).resolves.toEqual(facts);
    expect(request?.url).toBe('/api/v1/holdings/northstar/operations/lane28/staff-levels');
    expect(request?.headers).toMatchObject({ 'x-erpx-context-company': 'northstar-company' });
  });

  it('fails closed when the WorkContext Holding is not registered', async () => {
    const base = await listen(() => ({ status: 200, value: { data: facts } }));
    process.env[ENV] = config(base);
    await expect(
      nativeLane28StaffLevels(company, { holdingId: 'h9xx', holdingSlug: 'foreign', companyId: company }),
    ).rejects.toMatchObject({ status: 409, code: 'WORK_CONTEXT_CHANGED' });
  });

  it('fails closed on a conflicting WorkContext company', async () => {
    const base = await listen(() => ({ status: 200, value: { data: facts } }));
    process.env[ENV] = config(base);
    await expect(
      nativeLane28StaffLevels(company, {
        holdingId: 'h001',
        holdingSlug: 'acme',
        companyId: 'northstar-company',
      }),
    ).rejects.toMatchObject({ status: 409, code: 'WORK_CONTEXT_CHANGED' });
  });

  it('has no first-holding default when neither context nor company is given', async () => {
    const base = await listen(() => ({ status: 200, value: { data: { history: [] } } }));
    process.env[ENV] = config(base);
    await expect(nativeLane28LevelHistory('eng:1')).rejects.toMatchObject({
      status: 409,
      code: 'WORK_CONTEXT_REQUIRED',
    });
  });

  it('fails closed on a malformed level-history payload', async () => {
    const base = await listen(() => ({ status: 200, value: { data: { history: [{ created_at: '100' }] } } }));
    process.env[ENV] = config(base);
    await expect(nativeLane28LevelHistory('eng:1', undefined, company)).rejects.toMatchObject({
      status: 503,
      code: 'OPERATIONS_LANE28_RESPONSE_INVALID',
    });
  });

  it('fails closed on a malformed native owner payload', async () => {
    const base = await listen(() => ({ status: 200, value: { data: { levels: [], activeCount: '2' } } }));
    process.env[ENV] = config(base);
    await expect(nativeLane28StaffLevels(company)).rejects.toMatchObject({
      status: 503,
      code: 'OPERATIONS_LANE28_RESPONSE_INVALID',
    });
  });

  it('fails closed when the native owner is unavailable', async () => {
    const base = await listen(() => ({ status: 503, value: { error: { code: 'X', message: 'down' } } }));
    process.env[ENV] = config(base);
    await expect(nativeLane28StaffLevels(company)).rejects.toMatchObject({ status: 503 });
  });

  it('forwards an owner rejection with its exact status and code', async () => {
    const base = await listen(() => ({ status: 422, value: { error: { code: 'OPERATIONS_LANE28_REQUEST_INVALID', message: 'bad' } } }));
    process.env[ENV] = config(base);
    await expect(nativeLane28StaffLevels(company)).rejects.toMatchObject({
      status: 422,
      code: 'OPERATIONS_LANE28_REQUEST_INVALID',
    });
  });
});

/** Independent byte oracles captured from the retired TypeScript SQL before
 *  the migration. The Rust owner executes the same literals (only `?` becomes
 *  `$1` and an ORDER BY is added for deterministic response). */
const projection = '"id","company_id","code","name","rank","active","version","created_at","updated_at","level_number"';
const activeRanksSql = `SELECT id,rank FROM (SELECT ${projection} FROM staff_levels) staff_levels WHERE company_id=$1 AND active=1 ORDER BY rank,id`;
const acceptanceCountSql = `SELECT count(*)::bigint active_count FROM (SELECT ${projection} FROM staff_levels) staff_levels WHERE company_id=$1 AND active=1 AND level_number BETWEEN 1 AND 6`;
const historyProjection = '"id","engagement_id","previous_level_id","level_id","actor_id","reason","command_id","created_at","insertion_order"';
const levelHistorySql = `SELECT h.created_at,h.reason,l.name AS level_name,old.name AS previous_level_name FROM (${historyProjection.replace(/^/, 'SELECT ')} FROM staff_level_history) h JOIN (SELECT ${projection} FROM staff_levels) l ON l.id=h.level_id LEFT JOIN (SELECT ${projection} FROM staff_levels) old ON old.id=h.previous_level_id WHERE h.engagement_id=$1 ORDER BY h.created_at DESC`;
const sha = (sql: string) => createHash('sha256').update(sql).digest('hex');
const fnv = (sql: string) => {
  let hash = 0xcbf29ce484222325n;
  for (const byte of Buffer.from(sql)) {
    hash ^= BigInt(byte);
    hash = (hash * 0x100000001b3n) & 0xffffffffffffffffn;
  }
  return `0x${hash.toString(16)}`;
};

describe('native lane-28 SQL/result oracle', () => {
  it('pins rendered SQL hashes shared by Rust and TypeScript', () => {
    expect(sha(activeRanksSql)).toBe('0f76f4b04f07e67557e64c05f74e6d6abdb0280feb7a36f699111aaa5a02a540');
    expect(sha(acceptanceCountSql)).toBe('2acd64c84a200b674c8df3f6e6c44a091a34c67a81e0c047cc5afa710f411636');
    expect(sha(levelHistorySql)).toBe('aa0c8174c2c50c8dfdf5822391ee8d12376112805d236372348114eb49fdddac');
    expect(fnv(activeRanksSql)).toBe('0x195a854ad7fad16f');
    expect(fnv(acceptanceCountSql)).toBe('0xfd6ce958369b433e');
    expect(fnv(levelHistorySql)).toBe('0xf523fc8eb6805a52');
  });

  it('pins the Rust owner constants against the byte oracles', async () => {
    const domain = await readFile(
      new URL('../../../packages/server-operations/src/adapters/native_lane_28/domain.rs', import.meta.url),
      'utf8',
    );
    expect(domain).toContain('0f76f4b04f07e67557e64c05f74e6d6abdb0280feb7a36f699111aaa5a02a540');
    expect(domain).toContain('2acd64c84a200b674c8df3f6e6c44a091a34c67a81e0c047cc5afa710f411636');
    expect(domain).toContain('aa0c8174c2c50c8dfdf5822391ee8d12376112805d236372348114eb49fdddac');
    expect(domain).toContain('0x195a854ad7fad16f');
    expect(domain).toContain('0xfd6ce958369b433e');
    expect(domain).toContain('0xf523fc8eb6805a52');
  });

  it('leaves no SQL in the replaced TypeScript adapters', async () => {
    for (const file of ['staff-facts.ts', 'company-acceptance-facts.ts', 'people-level-history.ts']) {
      const text = await readFile(
        new URL(`../../../packages/server-operations/src/adapters/${file}`, import.meta.url),
        'utf8',
      );
      expect(text, file).not.toMatch(/SELECT|prepare\(|FROM staff_levels/);
      expect(text, file).toContain('nativeLane28');
    }
  });
});
