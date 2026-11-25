import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { describe, expect, it, vi } from 'vitest';
import type { PostgresDatabase } from '@erpx/server-runtime/public/runtime/postgres';
import type { SqlDatabase } from '@erpx/server-runtime/public/runtime/types';
import contract from '../../../packages/server-legal/src/adapters/package-obligation-plan-sources.json' with {
  type: 'json',
};

vi.mock('server-only', () => ({}));

type Call = { sql: string; binds: unknown[] };

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
  it('returns the owner contract and binds only the Company and clock', async () => {
    const { nativeLegalPackageObligationPlan } = await import(
      '../../../packages/server-legal/src/adapters/native-legal-package-obligations'
    );
    const plan = await nativeLegalPackageObligationPlan('company-a', 123);
    expect(plan).toEqual({ sql: contract.sql, values: ['company-a', 123] });
    expect(plan.sql.startsWith('WITH scope AS(')).toBe(true);
    expect(plan.sql.endsWith('ORDER BY kind,id,fact')).toBe(true);
    expect(plan.sql.match(/\?/g) ?? []).toHaveLength(2);
    expect(plan.sql).toContain('legal_intake_questions');
    expect(plan.sql).toContain("'retention-policy'");
    expect(plan.sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE \w+ SET|LOCK TABLE|FOR UPDATE|FOR SHARE/i);
  });

  it('executes the owner contract on the caller unit and keeps the retained evidence shape', async () => {
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
    expect(calls).toContainEqual({ sql: contract.sql, binds: ['company-a', 123] });
  });

  it('fails closed on an invalid scope', async () => {
    const { nativeLegalPackageObligationPlan } = await import(
      '../../../packages/server-legal/src/adapters/native-legal-package-obligations'
    );
    for (const [company, now] of [
      ['', 1],
      ['x'.repeat(201), 1],
      ['company-a', -1],
      ['company-a', Number.MAX_SAFE_INTEGER + 1],
    ] as const)
      await expect(nativeLegalPackageObligationPlan(company, now)).rejects.toMatchObject({
        status: 422,
        code: 'LEGAL_PACKAGE_OBLIGATION_SCOPE_REQUIRED',
      });
  });

  it('refuses the read without the settlement source lock', async () => {
    const { database } = fake([]);
    const db = owner(database);
    const { legalPackageObligations } = await import(
      '../../../packages/server-legal/src/public/package-obligations'
    );
    await expect(legalPackageObligations(db, database, input(123))).rejects.toThrow(
      'LEGAL_PACKAGE_OBLIGATION_SOURCE_REQUIRED',
    );
  });

  it('keeps the statement in the native owner contract and out of TypeScript', async () => {
    const [plan, sources] = await Promise.all([
      readFile(
        new URL(
          '../../../packages/server-legal/src/adapters/native_read_plan.rs',
          import.meta.url,
        ),
        'utf8',
      ),
      readFile(
        new URL(
          '../../../packages/server-legal/src/adapters/package-obligation-plan-sources.json',
          import.meta.url,
        ),
        'utf8',
      ),
    ]);
    expect(plan).toContain('pub const RD_PACKAGE_OBLIGATIONS');
    expect(plan).toContain('include_str!("package-obligation-plan-sources.json")');
    expect(plan).toContain('package_obligation_plan_sql');
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
      expect(sources).toContain(table);
    expect(sources).toContain("'retention-policy'");
    expect(sources).toContain(' ORDER BY kind,id,fact');
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
