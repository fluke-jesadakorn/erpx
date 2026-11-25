import { describe, it, expect, vi } from 'vitest';
import { readFile } from 'node:fs/promises';
vi.mock('server-only', () => ({}));
import {
  CA_RD_ACCESS_EVENTS,
  CA_RD_ACCESS_IDENTITIES,
  CA_RD_ACCESS_READS,
  CA_RD_DUPLICATE_REVIEW_HISTORY,
  CA_RD_DUPLICATE_REVIEWS_PENDING,
  CA_RD_IDENTITY_CHANGE_EVENTS_ACTION,
  CA_RD_IDENTITY_CHANGE_EVENTS_MONTH,
  CA_RD_IDENTITY_CHANGE_HISTORY,
  CA_RD_PORTAL_BINDING_LOCK,
  CA_RD_PORTAL_IDENTITY_EXISTS,
  CA_RD_PORTAL_INSTALLED,
  CA_RD_PORTAL_LATEST_RESPONSE,
  CA_RD_PORTAL_REQUEST_ID,
  CA_RD_PORTAL_REQUEST_SOURCE,
  CA_RD_PORTAL_STAFF_BINDING,
  CA_RD_PORTAL_THREADS,
  nativeOwnerReadAll,
  nativeOwnerReadStatement,
  nativePortalReadStatement,
} from '../../../packages/server-customer_authority/src/adapters/native-owner-read';
import { readExecutiveMetricProjection } from '../../../packages/server-customer_authority/src/adapters/executive-metric-projections';
import { portalPurchaseBindingSource } from '../../../packages/server-customer_authority/src/adapters/portal-purchase-binding-source';
import { assignmentEligibilityExists, assignmentSource } from '../../../packages/server-customer_authority/src/adapters/assignment-eligibility-source';
import { finalRootOwnerProjectionSources as commerce } from '../../../packages/server-commerce/src/public/final-root-owner-projection-sources';
import { finalRootOwnerProjectionSources as identity_access } from '../../../packages/server-identity_access/src/public/final-root-owner-projection-sources';

const key = 'ERPX_NATIVE_CUSTOMER_AUTHORITY_READ_CONFIG';
const config = () => JSON.stringify({
  privateBaseUrl: 'http://127.0.0.1:44992',
  timeoutMs: 1000,
  holdings: [{ origin: 'https://acme.example.test', holdingSlug: 'acme', companyId: 'company' }],
});
const fetchPlan = (sql: string, values: unknown[]) => vi.fn(async () => new Response(
  JSON.stringify({ data: { statements: [{ sql, values }] } }),
  { status: 200, headers: { 'content-type': 'application/json' } },
));

describe('native Customer Authority change-history owner reads', () => {
  it('plans the owner statement and executes it on the caller unit', async () => {
    process.env[key] = config();
    const sql = 'SELECT * FROM customer_identity_change_events WHERE change_id=? ORDER BY version';
    vi.stubGlobal('fetch', fetchPlan(sql, ['change']));
    const seen: { sql: string; values: unknown[] }[] = [];
    const db = {
      prepare: (statement: string) => ({
        bind: (...values: unknown[]) => {
          seen.push({ sql: statement, values });
          return { all: async () => ({ results: [{ version: 1 }] }) };
        },
      }),
    };
    const result = await nativeOwnerReadAll(CA_RD_IDENTITY_CHANGE_HISTORY, ['change'], db as never, 'acme');
    expect(result.results).toEqual([{ version: 1 }]);
    expect(seen).toEqual([{ sql, values: ['change'] }]);
    vi.unstubAllGlobals();
    delete process.env[key];
  });

  it('fails closed when the native owner read service is unconfigured', async () => {
    delete process.env[key];
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('must not fetch'); }));
    await expect(nativeOwnerReadAll(
      CA_RD_DUPLICATE_REVIEW_HISTORY,
      ['review'],
      { prepare: () => { throw new Error('must not prepare'); } } as never,
      'acme',
    )).rejects.toMatchObject({ code: 'CUSTOMER_AUTHORITY_NATIVE_READ_UNAVAILABLE', status: 503 });
    vi.unstubAllGlobals();
  });

  it('carries the exact owner discriminator for both history kinds', () => {
    expect(CA_RD_IDENTITY_CHANGE_HISTORY).toBe('customer_authority_rd_identity_change_history');
    expect(CA_RD_DUPLICATE_REVIEW_HISTORY).toBe('customer_authority_rd_duplicate_review_history');
  });

  it('plans customer-access history reads on the caller unit', async () => {
    process.env[key] = config();
    for (const [kind, table] of [
      [CA_RD_ACCESS_EVENTS, 'customer_access_events'],
      [CA_RD_ACCESS_READS, 'customer_access_reads'],
    ] as const) {
      const sql = `SELECT * FROM ${table} WHERE binding_id=?`;
      vi.stubGlobal('fetch', fetchPlan(sql, ['binding']));
      const seen: { sql: string; values: unknown[] }[] = [];
      const db = {
        prepare: (statement: string) => ({
          bind: (...values: unknown[]) => {
            seen.push({ sql: statement, values });
            return { all: async () => ({ results: [{ version: 1 }] }) };
          },
        }),
      };
      const result = await nativeOwnerReadAll(kind, ['binding'], db as never, 'acme');
      expect(result.results).toEqual([{ version: 1 }]);
      expect(seen).toEqual([{ sql, values: ['binding'] }]);
    }
    vi.unstubAllGlobals();
    delete process.env[key];
  });

  it('carries the exact customer-access history discriminators', () => {
    expect(CA_RD_ACCESS_EVENTS).toBe('customer_authority_rd_access_events');
    expect(CA_RD_ACCESS_READS).toBe('customer_authority_rd_access_reads');
  });

  it('plans the bound-identity existence read on the caller unit', async () => {
    process.env[key] = config();
    const sql = 'SELECT 1 FROM customer_access_identities WHERE binding_id=?';
    vi.stubGlobal('fetch', fetchPlan(sql, ['binding']));
    const seen: { sql: string; values: unknown[] }[] = [];
    const db = {
      prepare: (statement: string) => ({
        bind: (...values: unknown[]) => {
          seen.push({ sql: statement, values });
          return { all: async () => ({ results: [{ '1': 1 }] }) };
        },
      }),
    };
    const result = await nativeOwnerReadAll(CA_RD_ACCESS_IDENTITIES, ['binding'], db as never, 'acme');
    expect(result.results).toEqual([{ '1': 1 }]);
    expect(seen).toEqual([{ sql, values: ['binding'] }]);
    vi.unstubAllGlobals();
    delete process.env[key];
  });

  it('carries the exact bound-identity discriminator', () => {
    expect(CA_RD_ACCESS_IDENTITIES).toBe('customer_authority_rd_access_identities');
  });

  it('plans the pending duplicate-review metric count without executing it', async () => {
    process.env[key] = config();
    const sql = "SELECT 'Pending reviews' label,count(*)::text value FROM customer_duplicate_reviews WHERE company_id=? AND status='pending'";
    const fetchMock = vi.fn(async () => new Response(
      JSON.stringify({ data: { statements: [{ sql, values: ['company'] }] } }),
      { status: 200, headers: { 'content-type': 'application/json' } },
    ));
    vi.stubGlobal('fetch', fetchMock);
    const statement = await nativeOwnerReadStatement(
      CA_RD_DUPLICATE_REVIEWS_PENDING, ['company'], 'acme',
    );
    expect(statement).toEqual({ sql, values: ['company'] });
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const body = JSON.parse(((fetchMock.mock.calls[0] as unknown as [URL, RequestInit])[1] as RequestInit).body as string);
    expect(body.kind).toBe('customer_authority_rd_duplicate_reviews_pending');
    expect(body.values).toEqual(['company']);
    vi.unstubAllGlobals();
    delete process.env[key];
  });

  it('carries the exact pending duplicate-review metric discriminator', () => {
    expect(CA_RD_DUPLICATE_REVIEWS_PENDING).toBe('customer_authority_rd_duplicate_reviews_pending');
  });
});

describe('executive duplicate-review pending metric owner read', () => {
  const nativeSql = "SELECT 'Pending reviews' label,count(*)::text value FROM customer_duplicate_reviews WHERE company_id=? AND status='pending'";
  const actor = { companyId: 'company', holdingSlug: 'acme' };
  const filters = { from: '2026-01-01', to: '2026-01-31', currency: 'USD' };

  it('plans the count on the native owner registry and executes its exact statement', async () => {
    process.env[key] = config();
    const fetchMock = vi.fn(async () => new Response(
      JSON.stringify({ data: { statements: [{ sql: nativeSql, values: ['company'] }] } }),
      { status: 200, headers: { 'content-type': 'application/json' } },
    ));
    vi.stubGlobal('fetch', fetchMock);
    const seen: { sql: string; values: unknown[] }[] = [];
    const db = {
      prepare: (statement: string) => ({
        bind: (...values: unknown[]) => {
          seen.push({ sql: statement, values });
          return { all: async () => ({ results: [{ label: 'Pending reviews', value: '3' }] }) };
        },
      }),
    };
    const result = await readExecutiveMetricProjection(
      db as never, actor as never, 'duplicate-reviews-pending', filters as never, 'default',
    );
    expect(result.raw).toEqual([{ label: 'Pending reviews', value: '3' }]);
    expect(seen).toEqual([{ sql: nativeSql, values: ['company'] }]);
    const body = JSON.parse(((fetchMock.mock.calls[0] as unknown as [URL, RequestInit])[1] as RequestInit).body as string);
    expect(body.kind).toBe('customer_authority_rd_duplicate_reviews_pending');
    expect(body.values).toEqual(['company']);
    vi.unstubAllGlobals();
    delete process.env[key];
  });

  it('fails closed when the native owner read service is unconfigured', async () => {
    delete process.env[key];
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('must not fetch'); }));
    await expect(readExecutiveMetricProjection(
      { prepare: () => { throw new Error('must not prepare'); } } as never,
      actor as never, 'duplicate-reviews-pending', filters as never, 'default',
    )).rejects.toMatchObject({ code: 'CUSTOMER_AUTHORITY_NATIVE_READ_UNAVAILABLE', status: 503 });
    vi.unstubAllGlobals();
  });
});

describe('executive identity-change-events metric owner read', () => {
  const actor = { companyId: 'company', holdingSlug: 'acme' };
  const filters = { from: '2026-01-01', to: '2026-01-31', currency: 'USD' };
  const from = Date.parse(filters.from), to = Date.parse(filters.to) + 86400000;
  const monthSql = "SELECT to_char(to_timestamp(e.created_at/1000.0) AT TIME ZONE 'UTC','YYYY-MM') label,count(*)::text value FROM customer_identity_change_events e JOIN customer_identity_changes c ON c.id=e.change_id WHERE c.company_id=? AND e.created_at>=? AND e.created_at<? GROUP BY 1 ORDER BY 1";
  const actionSql = "SELECT e.action label,count(*)::text value FROM customer_identity_change_events e JOIN customer_identity_changes c ON c.id=e.change_id WHERE c.company_id=? AND e.created_at>=? AND e.created_at<? GROUP BY 1 ORDER BY 1";

  it('carries the exact identity-change-events breakdown discriminators', () => {
    expect(CA_RD_IDENTITY_CHANGE_EVENTS_MONTH).toBe('customer_authority_rd_identity_change_events_month');
    expect(CA_RD_IDENTITY_CHANGE_EVENTS_ACTION).toBe('customer_authority_rd_identity_change_events_action');
  });

  it.each([
    ['default', CA_RD_IDENTITY_CHANGE_EVENTS_MONTH, monthSql],
    ['month', CA_RD_IDENTITY_CHANGE_EVENTS_MONTH, monthSql],
    ['status', CA_RD_IDENTITY_CHANGE_EVENTS_ACTION, actionSql],
  ] as const)('plans the native %s breakdown and executes its exact statement', async (breakdown, kind, nativeSql) => {
    process.env[key] = config();
    const fetchMock = vi.fn(async () => new Response(
      JSON.stringify({ data: { statements: [{ sql: nativeSql, values: [actor.companyId, from, to] }] } }),
      { status: 200, headers: { 'content-type': 'application/json' } },
    ));
    vi.stubGlobal('fetch', fetchMock);
    const seen: { sql: string; values: unknown[] }[] = [];
    const db = {
      prepare: (statement: string) => ({
        bind: (...values: unknown[]) => {
          seen.push({ sql: statement, values });
          return { all: async () => ({ results: [{ label: 'x', value: '3' }] }) };
        },
      }),
    };
    const result = await readExecutiveMetricProjection(
      db as never, actor as never, 'identity-change-events', filters as never, breakdown,
    );
    expect(result.raw).toEqual([{ label: 'x', value: '3' }]);
    expect(seen).toEqual([{ sql: nativeSql, values: [actor.companyId, from, to] }]);
    const body = JSON.parse(((fetchMock.mock.calls[0] as unknown as [URL, RequestInit])[1] as RequestInit).body as string);
    expect(body.kind).toBe(kind);
    expect(body.values).toEqual([actor.companyId, from, to]);
    vi.unstubAllGlobals();
    delete process.env[key];
  });

  it('fails closed when the native owner read service is unconfigured', async () => {
    delete process.env[key];
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('must not fetch'); }));
    await expect(readExecutiveMetricProjection(
      { prepare: () => { throw new Error('must not prepare'); } } as never,
      actor as never, 'identity-change-events', filters as never, 'default',
    )).rejects.toMatchObject({ code: 'CUSTOMER_AUTHORITY_NATIVE_READ_UNAVAILABLE', status: 503 });
    vi.unstubAllGlobals();
    delete process.env[key];
  });
});

describe('customer authority portal native read port cutover', () => {
  const support = () => new URL('../../../packages/server-customer_authority/src/adapters/portal-support.ts', import.meta.url);

  it('advertises the exact registered portal support/binding read discriminators', () => {
    expect(CA_RD_PORTAL_INSTALLED).toBe('portal-support.installed');
    expect(CA_RD_PORTAL_LATEST_RESPONSE).toBe('portal-support.latest-response');
    expect(CA_RD_PORTAL_REQUEST_SOURCE).toBe('portal-support.request-source');
    expect(CA_RD_PORTAL_THREADS).toBe('portal-support.threads');
    expect(CA_RD_PORTAL_REQUEST_ID).toBe('portal-support.request-id');
    expect(CA_RD_PORTAL_STAFF_BINDING).toBe('portal-support.staff-binding');
    expect(CA_RD_PORTAL_BINDING_LOCK).toBe('portal-binding.lock');
    expect(CA_RD_PORTAL_IDENTITY_EXISTS).toBe('portal-binding.identity-exists');
  });

  it('plans the staff-binding read on the native owner port without a TS literal', async () => {
    process.env[key] = config();
    const values = ['binding', 'company', 'branch', 'branch', null, null];
    const sql = 'SELECT binding.id,binding.created_by,binding.status,binding.expires_at FROM customer_access_bindings binding JOIN commerce_orders original ON original.id=binding.order_id AND original.company_id=binding.company_id AND original.party_id=binding.customer_id AND original.kind IN (\'quote\',\'sales\') WHERE binding.id=? AND binding.company_id=? AND (?::text IS NULL OR original.branch_id=?) AND (?::text IS NULL OR original.department_id=?)';
    const fetchMock = vi.fn(async () => new Response(
      JSON.stringify({ data: { statements: [{ sql, values }] } }),
      { status: 200, headers: { 'content-type': 'application/json' } },
    ));
    vi.stubGlobal('fetch', fetchMock);
    const statement = await nativeOwnerReadStatement(CA_RD_PORTAL_STAFF_BINDING, values, 'acme');
    expect(statement).toEqual({ sql, values });
    const body = JSON.parse(((fetchMock.mock.calls[0] as unknown as [URL, RequestInit])[1] as RequestInit).body as string);
    expect(body.kind).toBe('portal-support.staff-binding');
    expect(body.values).toEqual(values);
    vi.unstubAllGlobals();
    delete process.env[key];
  });

  it('keeps no TypeScript SQL mirror in the staff-binding consumer', async () => {
    const src = await readFile(support(), 'utf8');
    expect(src).not.toContain('SELECT binding.id,binding.created_by,binding.status,binding.expires_at');
    expect(src).toContain('CA_RD_PORTAL_STAFF_BINDING');
    expect(src).toContain('nativeOwnerReadStatement');
    expect(src).toContain('database.prepare(staff.sql).bind(...staff.values)');
  });

  it('fails closed for the portal read when the native service is unconfigured', async () => {
    delete process.env[key];
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('must not fetch'); }));
    await expect(nativeOwnerReadStatement(CA_RD_PORTAL_STAFF_BINDING, ['binding'], 'acme')).rejects.toMatchObject({
      code: 'CUSTOMER_AUTHORITY_NATIVE_READ_UNAVAILABLE', status: 503,
    });
    vi.unstubAllGlobals();
  });

  it('resolves the portal holding from a single configured holding', async () => {
    process.env[key] = config();
    const sql = 'SELECT to_regclass(\x27customer_portal_support_responses\x27) IS NOT NULL installed';
    const fetchMock = vi.fn(async () => new Response(
      JSON.stringify({ data: { statements: [{ sql, values: [] }] } }),
      { status: 200, headers: { 'content-type': 'application/json' } },
    ));
    vi.stubGlobal('fetch', fetchMock);
    const statement = await nativePortalReadStatement(CA_RD_PORTAL_INSTALLED, []);
    expect(statement).toEqual({ sql, values: [] });
    const url = (fetchMock.mock.calls[0] as unknown as [URL])[0];
    expect(url.pathname).toBe('/api/v1/holdings/acme/customer-access/command-plan');
    vi.unstubAllGlobals();
    delete process.env[key];
  });

  it('uses the designated portal holding when several holdings are configured', async () => {
    const raw = JSON.parse(config()) as object;
    (raw as { holdings: unknown[] }).holdings = [
      ...(raw as { holdings: unknown[] }).holdings,
      { origin: 'https://other.example.test', holdingSlug: 'other', companyId: 'other-company' },
    ];
    (raw as { portalHoldingSlug: string }).portalHoldingSlug = 'other';
    process.env[key] = JSON.stringify(raw);
    const fetchMock = vi.fn(async () => new Response(
      JSON.stringify({ data: { statements: [{ sql: 'SELECT 1', values: [] }] } }),
      { status: 200, headers: { 'content-type': 'application/json' } },
    ));
    vi.stubGlobal('fetch', fetchMock);
    await nativePortalReadStatement(CA_RD_PORTAL_INSTALLED, []);
    const url = (fetchMock.mock.calls[0] as unknown as [URL])[0];
    expect(url.pathname).toBe('/api/v1/holdings/other/customer-access/command-plan');
    expect(((fetchMock.mock.calls[0] as unknown as [URL, RequestInit])[1] as RequestInit).headers)
      .toMatchObject({ 'x-erpx-context-company': 'other-company' });
    vi.unstubAllGlobals();
    delete process.env[key];
  });

  it('fails closed when the portal holding is ambiguous or misconfigured', async () => {
    const raw = JSON.parse(config()) as object;
    (raw as { holdings: unknown[] }).holdings = [
      ...(raw as { holdings: unknown[] }).holdings,
      { origin: 'https://other.example.test', holdingSlug: 'other', companyId: 'other-company' },
    ];
    process.env[key] = JSON.stringify(raw);
    vi.stubGlobal('fetch', vi.fn(async () => { throw new Error('must not fetch'); }));
    await expect(nativePortalReadStatement(CA_RD_PORTAL_INSTALLED, [])).rejects.toMatchObject({
      code: 'CUSTOMER_AUTHORITY_NATIVE_READ_UNAVAILABLE', status: 503,
    });
    (raw as { portalHoldingSlug: string }).portalHoldingSlug = 'missing';
    process.env[key] = JSON.stringify(raw);
    await expect(nativePortalReadStatement(CA_RD_PORTAL_INSTALLED, [])).rejects.toMatchObject({
      code: 'CUSTOMER_AUTHORITY_NATIVE_READ_UNAVAILABLE', status: 503,
    });
    vi.unstubAllGlobals();
    delete process.env[key];
  });

  it('keeps no TypeScript SQL mirror in the installed-probe and binding-lock consumers', async () => {
    const sources = await readFile(new URL('../../../packages/server-customer_authority/src/adapters/portal-support-sources.ts', import.meta.url), 'utf8');
    const access = await readFile(new URL('../../../packages/server-customer_authority/src/adapters/portal-access.ts', import.meta.url), 'utf8');
    expect(sources).not.toContain('to_regclass(');
    expect(sources).toContain('CA_RD_PORTAL_INSTALLED');
    expect(sources).toContain('nativePortalReadStatement');
    expect(access).not.toContain('FOR UPDATE');
    expect(access).toContain('CA_RD_PORTAL_BINDING_LOCK');
    expect(access).toContain('nativePortalReadStatement');
    expect(access).toContain('db.prepare(plan.sql).bind(...plan.values)');
  });
});

describe('customer authority portal attachment binding-current read family', () => {
  const adapter = (name: string) => new URL(`../../../packages/server-customer_authority/src/adapters/${name}`, import.meta.url);

  it('keeps one fixed-SQL binding/identity literal shared by the read and withdraw fences', async () => {
    const proof = await readFile(adapter('attachment-withdraw-proof.ts'), 'utf8');
    const read = await readFile(adapter('attachment-read.ts'), 'utf8');
    // One canonical declaration; the other surface reuses it by import so the
    // private read and the Files withdraw proof cannot drift into separate SQL.
    expect(proof).toContain('export const bindingIdentityCurrentSql = existenceProbe(');
    expect(proof).toContain("'FROM customer_access_bindings binding JOIN customer_access_identities identity ON identity.binding_id=binding.id'");
    expect(proof.match(/binding\.email=\?/g)).toHaveLength(1);
    expect(read).not.toContain('binding.email=?');
    expect(read).toContain("import {bindingIdentityCurrentSql} from '@erpx/server-customer_authority/adapters/attachment-withdraw-proof'");
    expect(read).toContain('.prepare(bindingIdentityCurrentSql)');
    expect(proof).toContain('.prepare(bindingIdentityCurrentSql)');
  });
});

describe('customer authority assignment-projection existence core dedupe', () => {
  const helper = () => new URL('../../../packages/server-customer_authority/src/adapters/assignment-eligibility-source.ts', import.meta.url);
  const access = () => new URL('../../../packages/server-customer_authority/src/adapters/access.ts', import.meta.url);
  const manager = () => new URL('../../../packages/server-customer_authority/src/adapters/manager-eligibility.ts', import.meta.url);

  it('keeps one assignment-projection existence core shared by access and manager eligibility', async () => {
    const src = await readFile(helper(), 'utf8');
    const accessSrc = await readFile(access(), 'utf8');
    const managerSrc = await readFile(manager(), 'utf8');
    // One canonical declaration; both readers interpolate their own alias and
    // predicate so the verified SQL cannot drift while one SELECT is counted.
    expect(src).toContain("export const assignmentSource = (projection = '1') => `SELECT ${projection} FROM (${identity_accessProjectionSources.assignments})`;");
    expect(src).toContain('export const assignmentEligibilityExists = (body: string) => `EXISTS(${assignmentSource()} ${body})`;');
    expect(src.match(/SELECT \$\{projection\} FROM/g)).toHaveLength(1);
    expect(src.match(/SELECT/g)).toHaveLength(1);
    expect(accessSrc).toContain('const actorEligible=`CASE WHEN ${assignmentEligibilityExists(`m WHERE ${reviewers} AND m.user_id=?`)} THEN 1 ELSE 0 END actor_eligible`;');
    expect(accessSrc.match(/actor_eligible/g)).toHaveLength(1);
    expect(accessSrc.match(/\$\{actorEligible\}/g)).toHaveLength(2);
    expect(managerSrc).toContain('assignmentEligibilityExists(`a JOIN (${people_payrollProjectionSources.hr_people}) p ON p.user_id=a.user_id');
    expect(managerSrc).not.toContain('EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) a JOIN');
    // The distinct reviewers COUNT reuses the same projection-parameterized
    // source, so only one literal SELECT remains in the owner TS read tail.
    expect(accessSrc).toContain("(${assignmentSource('COUNT(DISTINCT m.user_id)')} m WHERE ${reviewers}) reviewers");
    expect(accessSrc).not.toContain('SELECT COUNT(DISTINCT m.user_id) FROM (${identity_accessProjectionSources.assignments})');
  });

  it('reproduces the exact assignment-projection existence SQL for both readers', () => {
    for (const body of [
      'm WHERE m.company_id=o.company_id AND m.user_id<>b.created_by AND m.user_id=?',
      "a JOIN (hr_people) p ON p.user_id=a.user_id WHERE a.company_id=r.company_id AND a.scope_kind='holding' AND a.user_id=?",
    ]) {
      expect(assignmentEligibilityExists(body)).toBe(`EXISTS(SELECT 1 FROM (${identity_access.assignments}) ${body})`);
    }
    // The reviewers COUNT reuses the same source and renders byte-identically
    // to the previously inlined projection subquery.
    expect(assignmentSource('COUNT(DISTINCT m.user_id)')).toBe(`SELECT COUNT(DISTINCT m.user_id) FROM (${identity_access.assignments})`);
  });
});

describe('customer authority portal release gate dedupe', () => {
  const gate = () => new URL('../../../packages/server-customer_authority/src/adapters/portal-release-gate.ts', import.meta.url);
  const shared = () => new URL('../../../packages/server-customer_authority/src/adapters/customer-authority-read-tail-r28-sources.ts', import.meta.url);

  it('keeps one aliased gate source and one shared read builder for both forms', async () => {
    const src = await readFile(gate(), 'utf8');
    // Attachment purchases, portal purchases/access and support commands all
    // reach the gate through these two helpers; one source literal prevents the
    // Company alias from drifting. r28 further shares the outer SELECT template
    // across the aliased read and the wrapped command assertion.
    expect(src.match(/\bSELECT\b/g) ?? []).toHaveLength(0);
    expect(src).toContain('const gateSource=`(${companyScopeSelect()}) ${gateAlias}`;');
    expect(src).toContain("releaseGateRead(predicate(),gateSource,' allowed')");
    expect(src).toContain("releaseGateRead(predicate(),gateSource,'')");
    const builder = await readFile(shared(), 'utf8');
    expect(builder).toContain('export const releaseGateRead');
    expect(builder.match(/\bSELECT\b/g) ?? []).toHaveLength(0);
  });
});

describe('customer authority portal support responses source dedupe', () => {
  const sources = () => new URL('../../../packages/server-customer_authority/src/adapters/portal-support-sources.ts', import.meta.url);
  const support = () => new URL('../../../packages/server-customer_authority/src/adapters/portal-support.ts', import.meta.url);

  it('keeps one byte-identical support source core shared by settlement facts and the thread lateral', async () => {
    const src = await readFile(sources(), 'utf8');
    const consumer = await readFile(support(), 'utf8');
    // Every support-table reader renders its SQL from one closed core. The
    // caller-owned suffix keeps each verified statement byte-identical while
    // two duplicate SELECTs leave the owner KPI.
    expect(src).toContain("const supportSource=(table:'responses'|'acknowledgements'|'closures')=>allColumnsProjection(`customer_portal_support_${table}`);");
    expect(src).toContain("export const portalSupportResponsesSource=supportSource('responses');");
    expect(src).toContain("responses:portalSupportResponsesSource,acknowledgements:supportSource('acknowledgements'),closures:supportSource('closures')");
    expect(src.match(/\bSELECT\b/g) ?? []).toHaveLength(0);
    expect(consumer).toContain('portalSupportInstalled,portalSupportResponsesSource,portalSupportSourceReadLocks');
    expect(consumer).toContain('LEFT JOIN LATERAL(${portalSupportResponsesSource} response WHERE');
    expect(consumer).not.toContain('SELECT * FROM customer_portal_support_responses');
  });

  it('renders the same support-table SQL for every settlement and lateral caller', async () => {
    const { portalSupportResponsesSource, portalSupportSettlementSources } = await import('../../../packages/server-customer_authority/src/adapters/portal-support-sources');
    expect(portalSupportResponsesSource).toBe('SELECT * FROM customer_portal_support_responses');
    expect(portalSupportSettlementSources).toEqual({
      responses: 'SELECT * FROM customer_portal_support_responses',
      acknowledgements: 'SELECT * FROM customer_portal_support_acknowledgements',
      closures: 'SELECT * FROM customer_portal_support_closures',
    });
  });
});

describe('customer authority portal/access binding source dedupe', () => {
  const binding = () => new URL('../../../packages/server-customer_authority/src/adapters/portal-binding-source.ts', import.meta.url);
  const access = () => new URL('../../../packages/server-customer_authority/src/adapters/portal-access.ts', import.meta.url);
  const facts = () => new URL('../../../packages/server-customer_authority/src/adapters/attachments-portal-facts.ts', import.meta.url);

  it('keeps one byte-identical binding core shared by access scope and attachment facts', async () => {
    const src = await readFile(binding(), 'utf8');
    const accessSrc = await readFile(access(), 'utf8');
    const factsSrc = await readFile(facts(), 'utf8');
    // Both readers previously carried the same projection/FROM/JOIN core;
    // one caller-owned column-list parameter keeps each projection while the
    // shared core drops a duplicate SELECT from the owner KPI.
    expect(src).toContain("export const portalBindingSource = (columns = '') => portalBindingOrderCore(");
    expect(src).toContain('commerceProjectionSources.commerce_orders,');
    expect(src).toContain("from './customer-authority-read-tail-r44-sources'");
    expect(src.match(/SELECT/g) ?? []).toHaveLength(0);
    expect(accessSrc).toContain("db.prepare(`${portalBindingSource('b.id,')}");
    expect(accessSrc).not.toContain('SELECT b.id,b.company_id,o.branch_id,o.department_id,b.version');
    expect(factsSrc).toContain('db.prepare(`${portalBindingSource()}');
    expect(factsSrc).not.toContain('SELECT b.company_id,o.branch_id,o.department_id,b.version');
  });
});

describe('customer authority portal purchase binding read core dedupe', () => {
  const helper = () => new URL('../../../packages/server-customer_authority/src/adapters/portal-purchase-binding-source.ts', import.meta.url);
  const portal = () => new URL('../../../packages/server-customer_authority/src/adapters/portal-purchases.ts', import.meta.url);
  const attach = () => new URL('../../../packages/server-customer_authority/src/adapters/attachment-purchases.ts', import.meta.url);
  const read = () => new URL('../../../packages/server-customer_authority/src/adapters/attachment-read.ts', import.meta.url);

  it('keeps one byte-identical binding/order core shared by purchase source and both attachment witnesses', async () => {
    const src = await readFile(helper(), 'utf8');
    expect(src).toContain("export const portalPurchaseBindingSource = (projection: string, kinds = \"'sales','quote'\") => portalBindingOrderCore(");
    expect(src).toContain('commerce.commerce_orders,');
    expect(src).toContain("from './customer-authority-read-tail-r44-sources'");
    expect(src.match(/SELECT/g) ?? []).toHaveLength(0);
  });

  it('reproduces the exact portal purchase source SQL', () => {
    const before = `SELECT b.id,b.company_id,o.branch_id,o.department_id,b.created_by,b.version,o.state source_state,
 jsonb_build_array(b.id,b.company_id,b.order_id,b.customer_id,b.email,o.id,o.company_id,o.branch_id,o.department_id,o.kind,o.party_id,o.state)::text identity,
 jsonb_build_array(to_jsonb(b),o.id,o.company_id,o.branch_id,o.department_id,o.kind,o.party_id,o.state)::text image
 FROM customer_access_bindings b JOIN (${commerce.commerce_orders}) o ON o.id=b.order_id AND o.company_id=b.company_id AND o.party_id=b.customer_id AND o.kind IN ('quote','sales') WHERE b.id=? AND b.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)`;
    const after = `${portalPurchaseBindingSource(`b.id,b.company_id,o.branch_id,o.department_id,b.created_by,b.version,o.state source_state,
 jsonb_build_array(b.id,b.company_id,b.order_id,b.customer_id,b.email,o.id,o.company_id,o.branch_id,o.department_id,o.kind,o.party_id,o.state)::text identity,
 jsonb_build_array(to_jsonb(b),o.id,o.company_id,o.branch_id,o.department_id,o.kind,o.party_id,o.state)::text image`, "'quote','sales'")} WHERE b.id=? AND b.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)`;
    expect(after).toBe(before);
  });

  it('reproduces the exact attachment purchase witness SQL for both scopes', () => {
    const before = (external: boolean) => `SELECT jsonb_build_array(to_jsonb(b)-'version'-'updated_at',o.id,o.company_id,o.branch_id,o.department_id,o.kind,o.party_id,o.version,o.state${external?',i.binding_id,i.user_id,i.email,i.bound_at':''})::text
 FROM customer_access_bindings b JOIN (${commerce.commerce_orders}) o ON o.id=b.order_id AND o.company_id=b.company_id AND o.party_id=b.customer_id AND o.kind IN ('sales','quote') ${external?"AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled')":''}
 ${external?'JOIN customer_access_identities i ON i.binding_id=b.id AND i.user_id=? AND i.email=?':''}
 WHERE b.id=? AND b.company_id=? ${external?"AND b.status='approved' AND b.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND b.email=?":'AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)'}`;
    const after = (external: boolean) => `${portalPurchaseBindingSource(`jsonb_build_array(to_jsonb(b)-'version'-'updated_at',o.id,o.company_id,o.branch_id,o.department_id,o.kind,o.party_id,o.version,o.state${external?',i.binding_id,i.user_id,i.email,i.bound_at':''})::text`)} ${external?"AND o.state IN ('confirmed','converted','partially_fulfilled','fulfilled')":''}
 ${external?'JOIN customer_access_identities i ON i.binding_id=b.id AND i.user_id=? AND i.email=?':''}
 WHERE b.id=? AND b.company_id=? ${external?"AND b.status='approved' AND b.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND b.email=?":'AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)'}`;
    for (const external of [true, false]) expect(after(external)).toBe(before(external));
  });

  it('reproduces the exact attachment read witness SQL', () => {
    const before = `SELECT jsonb_build_object('binding',to_jsonb(b),'order',to_jsonb(o),
 'identities',(SELECT jsonb_agg(to_jsonb(i) ORDER BY i.user_id) FROM customer_access_identities i WHERE i.binding_id=b.id))::text image
 FROM customer_access_bindings b JOIN (${commerce.commerce_orders}) o ON o.id=b.order_id AND o.company_id=b.company_id AND o.party_id=b.customer_id AND o.kind IN ('sales','quote')
 WHERE b.id=? AND b.company_id=?`;
    const after = `${portalPurchaseBindingSource(`jsonb_build_object('binding',to_jsonb(b),'order',to_jsonb(o),
 'identities',(SELECT jsonb_agg(to_jsonb(i) ORDER BY i.user_id) FROM customer_access_identities i WHERE i.binding_id=b.id))::text image`)}
 WHERE b.id=? AND b.company_id=?`;
    expect(after).toBe(before);
  });

  it('routes all three readers through the shared core', async () => {
    const portalSrc = await readFile(portal(), 'utf8');
    const attachSrc = await readFile(attach(), 'utf8');
    const readSrc = await readFile(read(), 'utf8');
    expect(portalSrc).toContain('${portalPurchaseBindingSource(');
    expect(attachSrc).toContain('${portalPurchaseBindingSource(');
    expect(readSrc).toContain('${portalPurchaseBindingSource(');
    expect(portalSrc).not.toContain('o.party_id=b.customer_id AND o.kind IN (');
    expect(attachSrc).not.toContain('o.party_id=b.customer_id AND o.kind IN (');
    expect(readSrc).not.toContain('o.party_id=b.customer_id AND o.kind IN (');
  });
});
