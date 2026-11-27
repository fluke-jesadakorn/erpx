import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
vi.mock('server-only', () => ({}));

/** Mutable request facts so each case can pin the incoming host/cookie. */
const io = vi.hoisted(() => ({ host: 'acme.jesadakorn.com' as string | null, cookie: '__Host-erpx-product=session' as string | null }));
vi.mock('next/headers', () => ({
  headers: async () => new Headers(io.host === null ? {} : {
    host: io.host,
    ...(io.cookie ? { cookie: io.cookie } : {}),
  }),
}));
vi.mock('@erpx/server-runtime/public/http', () => ({
  ApiError: class ApiError extends Error {
    constructor(readonly status: number, readonly code: string, message: string) {
      super(message);
      this.name = 'ApiError';
    }
  },
}));

import { PP_RD18_HR_PEOPLE_PROJ_SQL } from '../../../packages/server-people_payroll/src/adapters/pp-read-tail-r18-sources';
import { assignmentsProjectionCompact } from '../../../packages/server-identity_access/src/adapters/assignment-projection-source';

const slice = new URL(
  '../../../packages/server-maintenance/src/adapters/native_owner_options/',
  import.meta.url,
);
const read = (name: string) => readFile(new URL(name, slice), 'utf8');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

/** The exact statement the TypeScript `people` repository branch composes from
 * the two closed owner projections. The native slice must execute this literal
 * with a single `$1` Company bind, not a plan or a TypeScript-prepared query. */
const oracle =
  'SELECT DISTINCT p.user_id id,p.legal_name name ' +
  `FROM (${PP_RD18_HR_PEOPLE_PROJ_SQL}) p ` +
  `JOIN (${assignmentsProjectionCompact}) x ` +
  'ON x.user_id=p.user_id AND x.company_id=p.company_id ' +
  "WHERE p.company_id=$1 AND p.status='active' AND p.merged_into_id IS NULL " +
  'AND iam_assignment_active(x.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) ' +
  "AND iam_role_allows(x.role,x.company_id,'records.write') ORDER BY p.legal_name";

async function rustSql(): Promise<string> {
  const text = await read('domain.rs');
  const match = text.match(/OWNER_OPTIONS_SQL: &str = "([\s\S]*?)";/);
  if (!match) throw new Error('OWNER_OPTIONS_SQL constant not found');
  return match[1]
    .split('\\\n')
    .map((line) => line.trim())
    .join(' ')
    .replace(/\\"/g, '"');
}

describe('native Maintenance owner-option read parity', () => {
  it('executes the same composed statement as the TypeScript authority', async () => {
    const sql = await rustSql();
    expect(sql).toBe(oracle);
    expect(sha(sql)).toBe(sha(oracle));
    expect(sql.match(/\$1/g)).toHaveLength(1);
    expect(sql).not.toContain('?');
    for (const forbidden of ['INSERT', 'UPDATE ', 'DELETE ', 'LOCK ']) {
      expect(sql.toUpperCase()).not.toContain(forbidden);
    }
  });

  it('pins the complete {id,name} row image used by the preventive console', async () => {
    const domain = await read('domain.rs');
    expect(domain).toMatch(/pub struct OwnerOption \{[\s\S]*?pub id: String,[\s\S]*?pub name: String,[\s\S]*?\}/);
    expect(domain).not.toMatch(/pub struct OwnerOption \{[\s\S]*?pub [a-z_]+: (?!String)/);
    const postgres = await read('postgres.rs');
    expect(postgres).toContain('sqlx::query(OWNER_OPTIONS_SQL)');
    expect(postgres).toContain('.fetch_all(&mut *connection)');
    expect(postgres).toContain('row.try_get("id")');
    expect(postgres).toContain('row.try_get("name")');
  });

  it('owns a dedicated axum route with no SQL fallback', async () => {
    const http = await read('http.rs');
    expect(http).toContain('/api/v1/holdings/{holding_slug}/maintenance/owner-options');
    expect(http).toContain('get(current)');
    expect(http).toContain('x-erpx-context-company');
    expect(http).not.toContain('SELECT ');
    const mod = await read('postgres.rs');
    expect(mod.match(/sqlx::query\(/g)).toHaveLength(1);
  });
});

/** The TypeScript transport is a strict projection of the mounted native GET:
 * it prepares no SQL, carries no repository fallback, forwards only the current
 * Holding/session/CAS and closes every failure. No `people` SELECT remains to
 * fall back to, so `prepare` must never be reachable on this path. */
describe('native Maintenance owner-option transport', () => {
  const key = 'ERPX_NATIVE_MAINTENANCE_CONFIG';
  const config = (overrides: Record<string, unknown> = {}) => JSON.stringify({
    privateBaseUrl: 'http://127.0.0.1:44992',
    timeoutMs: 1000,
    holdings: [{ origin: 'https://acme.jesadakorn.com', holdingSlug: 'acme', holdingId: 'h001' }],
    ...overrides,
  });
  const actor = (overrides: Record<string, unknown> = {}) => ({
    userId: 'user', holdingId: 'h001', holdingSlug: 'acme', companyId: 'company',
    branchId: null, departmentId: null, assignmentId: 'assignment',
    authority: { revision: 7, assignmentVersion: 3 },
    iam: { permissions: ['records.read', 'records.write'] },
    ...overrides,
  }) as unknown as import('@erpx/server-identity_access/public/authorization').WorkContext;
  const ok = (data: unknown) => new Response(JSON.stringify({ data }), {
    status: 200, headers: { 'content-type': 'application/json' },
  });
  const load = () => import('../../../packages/server-maintenance/src/adapters/native-owner-options-read');
  let fetchSpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    process.env[key] = config();
    io.host = 'acme.jesadakorn.com';
    io.cookie = '__Host-erpx-product=session';
  });
  afterEach(() => { fetchSpy?.mockRestore(); delete process.env[key]; });

  it('reads owner options from the native endpoint only, forwarding the current context', async () => {
    fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(
      ok([{ id: 'u1', name: 'Owner One' }, { id: 'u2', name: 'Owner Two' }]),
    );
    const { maintenanceOwnerOptions } = await load();
    expect(await maintenanceOwnerOptions(actor())).toEqual([
      { id: 'u1', name: 'Owner One' }, { id: 'u2', name: 'Owner Two' },
    ]);
    const [url, init] = fetchSpy.mock.calls[0];
    expect(String(url)).toBe('http://127.0.0.1:44992/api/v1/holdings/acme/maintenance/owner-options');
    expect(init).toMatchObject({ method: 'GET', redirect: 'error', cache: 'no-store' });
    const h = new Headers(init?.headers as HeadersInit);
    expect(h.get('host')).toBe('acme.jesadakorn.com');
    expect(h.get('cookie')).toBe('__Host-erpx-product=session');
    expect(h.get('x-erpx-context-company')).toBe('company');
    expect(h.get('x-erpx-context-assignment')).toBe('assignment');
    expect(h.get('x-erpx-context-revision')).toBe('7');
    expect(h.get('x-erpx-context-assignment-version')).toBe('3');
  });

  const failures: [string, () => void, Record<string, unknown>, string][] = [
    ['missing configuration', () => { delete process.env[key]; }, {}, 'NATIVE_MAINTENANCE_OWNER_OPTIONS_UNAVAILABLE'],
    ['insecure native base URL', () => { process.env[key] = config({ privateBaseUrl: 'http://10.0.0.1:44992' }); }, {}, 'NATIVE_MAINTENANCE_CONFIG_INVALID'],
    ['changed host', () => { io.host = 'evil.example.com'; }, {}, 'WORK_CONTEXT_CHANGED'],
    ['changed holding', () => {}, { holdingId: 'other' }, 'WORK_CONTEXT_CHANGED'],
    ['missing authority', () => {}, { authority: null }, 'WORK_CONTEXT_CHANGED'],
    ['missing company scope', () => {}, { companyId: '' }, 'NATIVE_MAINTENANCE_OWNER_OPTIONS_RESPONSE_INVALID'],
    ['oversized cookie', () => { io.cookie = 'x'.repeat(9000); }, {}, 'NATIVE_MAINTENANCE_READ_COOKIE_LIMIT'],
  ];
  for (const [label, arrange, overrides, code] of failures) {
    it(`fails closed for ${label} before any fetch`, async () => {
      arrange();
      fetchSpy = vi.spyOn(globalThis, 'fetch');
      const { maintenanceOwnerOptions } = await load();
      await expect(maintenanceOwnerOptions(actor(overrides))).rejects.toMatchObject({ status: expect.any(Number), code });
      expect(fetchSpy).not.toHaveBeenCalled();
    });
  }

  it('closes a non-ok, wrong-content-type or failed fetch without a fallback', async () => {
    const { maintenanceOwnerOptions } = await load();
    fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response('{}', { status: 503, headers: { 'content-type': 'application/json' } }));
    await expect(maintenanceOwnerOptions(actor())).rejects.toMatchObject({ code: 'NATIVE_MAINTENANCE_OWNER_OPTIONS_UNAVAILABLE' });
    fetchSpy.mockResolvedValue(new Response(JSON.stringify({ data: [] }), { status: 200, headers: { 'content-type': 'text/plain' } }));
    await expect(maintenanceOwnerOptions(actor())).rejects.toMatchObject({ code: 'NATIVE_MAINTENANCE_OWNER_OPTIONS_UNAVAILABLE' });
    fetchSpy.mockRejectedValue(new Error('network'));
    await expect(maintenanceOwnerOptions(actor())).rejects.toMatchObject({ code: 'NATIVE_MAINTENANCE_OWNER_OPTIONS_UNAVAILABLE' });
  });

  it('rejects malformed or unbounded payloads instead of projecting them', async () => {
    const { maintenanceOwnerOptions } = await load();
    for (const body of [
      { rows: [] },
      { data: [{ id: 'u1', name: 'Owner', extra: 1 }] },
      { data: [{ id: '', name: 'Owner' }] },
      { data: Array.from({ length: 1001 }, () => ({ id: 'u1', name: 'Owner' })) },
    ]) {
      fetchSpy = vi.spyOn(globalThis, 'fetch').mockResolvedValue(ok(body));
      await expect(maintenanceOwnerOptions(actor())).rejects.toMatchObject({ code: 'NATIVE_MAINTENANCE_OWNER_OPTIONS_RESPONSE_INVALID' });
      fetchSpy.mockRestore();
    }
  });

  it('keeps the TS transport SQL-free and on the native route', async () => {
    const source = await readFile(new URL(
      '../../../packages/server-maintenance/src/adapters/native-owner-options-read.ts', import.meta.url,
    ), 'utf8');
    expect(source).not.toMatch(/\b(?:SELECT|INSERT|UPDATE|DELETE|prepare)\b/i);
    expect(source).toContain('/maintenance/owner-options');
    const pub = await readFile(new URL(
      '../../../packages/server-maintenance/src/public/asset-maintenance.ts', import.meta.url,
    ), 'utf8');
    expect(pub).toContain('maintenanceOwnerOptions');
  });
});
