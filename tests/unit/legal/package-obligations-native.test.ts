import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { PostgresDatabase } from '@erpx/server-runtime/public/runtime/postgres';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';

vi.mock('server-only', () => ({}));

const ENV = 'ERPX_NATIVE_LEGAL_RECORDS_CONFIG';
const CONFIG = {
  privateBaseUrl: 'http://127.0.0.1:44999',
  timeoutMs: 2000,
  holdings: [{ origin: 'https://acme.example.test', holdingSlug: 'acme', holdingId: 'h001' }],
};
const PLAN =
  "WITH scope AS(SELECT ?::text company_id,?::bigint now_ms) SELECT 'retention-policy' kind,p.id,to_jsonb(p)::text fact FROM legal_retention_policies p,scope WHERE p.company_id=scope.company_id AND p.state='draft' UNION ALL SELECT 'item' kind,i.id,to_jsonb(i)::text fact FROM legal_items i,scope ORDER BY kind,id,fact";
type Call = { sql: string; binds: unknown[] };
type FetchCall = { url: string; init?: RequestInit };

let savedConfig: string | undefined;
let savedFetch: typeof globalThis.fetch;
beforeEach(() => {
  savedConfig = process.env[ENV];
  savedFetch = globalThis.fetch;
});
afterEach(() => {
  globalThis.fetch = savedFetch;
  if (savedConfig === undefined) delete process.env[ENV];
  else process.env[ENV] = savedConfig;
});

function capture(reply: unknown, status = 200): FetchCall[] {
  const calls: FetchCall[] = [];
  globalThis.fetch = (async (url: RequestInfo | URL, init?: RequestInit) => {
    calls.push({ url: String(url), init });
    return Response.json(reply, { status });
  }) as typeof fetch;
  return calls;
}

function fake(facts: unknown[]) {
  const calls: Call[] = [];
  const result = () => ({ results: facts, success: true, meta: { changes: 0, duration: 0 } });
  const statement = (sql: string) => ({
    run: async () => {
      calls.push({ sql, binds: [] });
      return { success: true };
    },
    all: async () => {
      calls.push({ sql, binds: [] });
      return result();
    },
    first: async () => {
      calls.push({ sql, binds: [] });
      return facts[0] ?? null;
    },
    bind: (...binds: unknown[]) => ({
      run: async () => {
        calls.push({ sql, binds });
        return { success: true };
      },
      all: async () => {
        calls.push({ sql, binds });
        return result();
      },
      first: async () => {
        calls.push({ sql, binds });
        return facts[0] ?? null;
      },
    }),
  });
  return { database: { prepare: statement } as unknown as SqlDatabase, calls };
}

function owner(database: SqlDatabase): PostgresDatabase {
  return {
    assertActiveTransaction() {},
    transaction: async <T>(run: (transaction: SqlDatabase) => Promise<T>) => run(database),
  } as unknown as PostgresDatabase;
}

const input = (now: number) => ({
  companyId: 'company-a',
  packageId: 'legal',
  packageVersion: 1,
  moduleId: 'legal',
  now,
});

describe('native Legal package-obligation cutover', () => {
  it('plans the owner statement and binds only the Company and clock', async () => {
    process.env[ENV] = JSON.stringify(CONFIG);
    const calls = capture({ data: { statements: [{ sql: PLAN, values: ['company-a', 123] }] } });
    const { nativeLegalPackageObligationPlan } = await import(
      '../../../packages/server-legal/src/adapters/native-legal-package-obligations'
    );
    const plan = await nativeLegalPackageObligationPlan('company-a', 123);
    expect(plan).toEqual({ sql: PLAN, values: ['company-a', 123] });
    expect(calls).toHaveLength(1);
    expect(new URL(calls[0]!.url).pathname).toBe(
      '/api/v1/holdings/acme/legal/command-plan',
    );
    const headers = new Headers(calls[0]!.init?.headers);
    expect(headers.get('host')).toBe('acme.example.test');
    expect(headers.get('origin')).toBe('https://acme.example.test');
    expect(headers.get('content-type')).toBe('application/json');
    expect(JSON.parse(String(calls[0]!.init?.body))).toEqual({
      kind: 'legal_package_obligations',
      values: ['company-a', 123],
    });
  });

  it('executes the plan on the caller unit and keeps the retained evidence shape', async () => {
    process.env[ENV] = JSON.stringify(CONFIG);
    capture({ data: { statements: [{ sql: PLAN, values: ['company-a', 123] }] } });
    const facts = [{ kind: 'retention-policy', id: 'policy-one', fact: '{"id":"policy-one"}' }];
    const { database, calls } = fake(facts);
    const db = owner(database);
    const { withLegalPackageObligations, legalPackageObligations } = await import(
      '../../../packages/server-legal/src/public/package-obligations'
    );
    const result = await withLegalPackageObligations(db, 'company-a', transaction =>
      legalPackageObligations(db, transaction, input(123)),
    );
    expect(Object.keys(result).sort()).toEqual(
      ['companyId', 'moduleId', 'observedAt', 'outstanding', 'packageId', 'revision'].sort(),
    );
    expect(result).toMatchObject({
      companyId: 'company-a',
      moduleId: 'legal',
      packageId: 'legal',
      observedAt: 123,
      outstanding: 1,
    });
    expect(result.revision).toBe(
      createHash('sha256')
        .update(
          JSON.stringify({
            companyId: 'company-a',
            packageId: 'legal',
            packageVersion: 1,
            facts,
          }),
        )
        .digest('hex'),
    );
    expect(calls).toContainEqual({ sql: PLAN, binds: ['company-a', 123] });
  });

  it('fails closed on configuration, transport, reply shape and scope', async () => {
    delete process.env[ENV];
    const { nativeLegalPackageObligationPlan } = await import(
      '../../../packages/server-legal/src/adapters/native-legal-package-obligations'
    );
    await expect(nativeLegalPackageObligationPlan('company-a', 123)).rejects.toMatchObject({
      status: 503,
      code: 'NATIVE_LEGAL_UNAVAILABLE',
    });

    process.env[ENV] = JSON.stringify(CONFIG);
    capture({ error: { code: 'NATIVE_DENIED', message: 'Denied' } }, 403);
    await expect(nativeLegalPackageObligationPlan('company-a', 123)).rejects.toMatchObject({
      status: 403,
      code: 'NATIVE_DENIED',
      message: 'Denied',
    });

    capture({ data: { statements: [] } });
    await expect(nativeLegalPackageObligationPlan('company-a', 123)).rejects.toMatchObject({
      status: 502,
      code: 'NATIVE_LEGAL_READ_RESPONSE_INVALID',
    });

    capture({ data: { statements: [{ sql: PLAN, values: ['company-b', 123] }] } });
    await expect(nativeLegalPackageObligationPlan('company-a', 123)).rejects.toMatchObject({
      status: 502,
      code: 'NATIVE_LEGAL_PACKAGE_READ_RESPONSE_INVALID',
    });

    capture({ data: { statements: [{ sql: 'SELECT 1', values: ['company-a', 123] }] } });
    await expect(nativeLegalPackageObligationPlan('company-a', 123)).rejects.toMatchObject({
      status: 502,
      code: 'NATIVE_LEGAL_PACKAGE_READ_RESPONSE_INVALID',
    });

    await expect(nativeLegalPackageObligationPlan('', 123)).rejects.toMatchObject({
      status: 422,
      code: 'LEGAL_PACKAGE_OBLIGATION_SCOPE_REQUIRED',
    });
  });

  it('refuses the read without the settlement source lock', async () => {
    process.env[ENV] = JSON.stringify(CONFIG);
    capture({ data: { statements: [{ sql: PLAN, values: ['company-a', 123] }] } });
    const { database } = fake([]);
    const db = owner(database);
    const { legalPackageObligations } = await import(
      '../../../packages/server-legal/src/public/package-obligations'
    );
    await expect(legalPackageObligations(db, database, input(123))).rejects.toThrow(
      'LEGAL_PACKAGE_OBLIGATION_SOURCE_REQUIRED',
    );
  });

  it('keeps the native owner SQL and the mounted plan route', async () => {
    const [plan, http] = await Promise.all([
      readFile(
        new URL(
          '../../../packages/server-legal/src/adapters/native_read_plan.rs',
          import.meta.url,
        ),
        'utf8',
      ),
      readFile(
        new URL('../../../packages/server-legal/src/adapters/legal_http.rs', import.meta.url),
        'utf8',
      ),
    ]);
    expect(plan).toContain('pub const RD_PACKAGE_OBLIGATIONS');
    expect(plan).toContain('const FEATURE_TABLES');
    for (const table of [
      'legal_case_events',
      'legal_compliance_actions',
      'legal_privacy_cases',
      'legal_corporate_actions',
      'legal_ip_actions',
      'legal_counsel_entries',
      'legal_obligation_schedule',
      'legal_custody_events',
      'legal_policy_acknowledgements',
      'legal_intake_questions',
    ])
      expect(plan).toContain(table);
    expect(plan).toContain("ORDER BY kind,id,fact");
    expect(plan).toContain("SELECT 'retention-policy'");
    expect(http).toContain('/legal/command-plan');
    expect(http).toContain('plan_legal_read');
  });

  it('retirement:ts-statements is BEFORE=125 -> AFTER=112', async () => {
    const [adapter, client] = await Promise.all([
      readFile(
        new URL('../../../packages/server-legal/src/adapters/package-obligations.ts', import.meta.url),
        'utf8',
      ),
      readFile(
        new URL(
          '../../../packages/server-legal/src/adapters/native-legal-package-obligations.ts',
          import.meta.url,
        ),
        'utf8',
      ),
    ]);
    expect(adapter.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(client.match(/SELECT /g) ?? []).toHaveLength(0);
    let total = 0;
    const walk = async (dir: string) => {
      for (const entry of await readdir(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) await walk(path);
        else if (entry.name.endsWith('.ts'))
          total += ((await readFile(path, 'utf8')).match(/SELECT /g) ?? []).length;
      }
    };
    await walk(resolve(__dirname, '../../../packages/server-legal/src'));
    expect(total).toBeLessThan(125);
    expect(total).toBe(112);
  });
});
