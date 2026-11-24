import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { executiveGenerationJobsSql } from '../../../packages/server-runtime/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-runtime/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// Pre-dedupe literals copied from HEAD, used only as byte-identity oracles.
const pre = [
  'SELECT company_id,created_at FROM executive_generation_jobs',
  'SELECT count(*) FROM executive_generation_jobs WHERE company_id=? AND created_at>=?',
  'SELECT "company_id","created_at" FROM executive_generation_jobs',
  'SELECT "id","report_id","company_id","user_id","assignment_id","scope_key","question","input_json","actor_json","provider_json","state","proposal_json","error","lease_token","lease_until","attempts","created_at","updated_at" FROM executive_generation_jobs',
];

const callers = [
  ['agent-ai-usage-projection.ts', 2],
  ['platform-owner-projection-sources.ts', 1],
  ['final-root-owner-projection-sources.ts', 1],
] as const;

describe('Runtime read-tail r6 executive_generation_jobs parameterized family', () => {
  test('canonical helper renders byte-identical SQL for every family member and stays read-only', () => {
    expect(executiveGenerationJobsSql('company_id,created_at')).toBe(pre[0]);
    expect(executiveGenerationJobsSql('count(*)', ' WHERE company_id=? AND created_at>=?')).toBe(pre[1]);
    expect(executiveGenerationJobsSql('"company_id","created_at"')).toBe(pre[2]);
    expect(executiveGenerationJobsSql('"id","report_id","company_id","user_id","assignment_id","scope_key","question","input_json","actor_json","provider_json","state","proposal_json","error","lease_token","lease_until","attempts","created_at","updated_at"')).toBe(pre[3]);
    for (const sql of pre) readOnly(sql);
  });

  test('the family is one owner literal, composed by every named adapter, never inlined', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('`SELECT ${columns} FROM executive_generation_jobs${suffix}`')).toHaveLength(2);
    const family = shared.slice(shared.indexOf('export const executiveGenerationJobsSql'));
    expect(family).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    let uses = 0;
    for (const [name, count] of callers) {
      const text = src(name);
      for (const literal of pre) expect(text, `${name} :: ${literal}`).not.toContain(literal);
      expect(text.match(/executiveGenerationJobsSql\(/g) ?? []).toHaveLength(count);
      uses += count;
    }
    expect(uses).toBe(4);
  });

  test('control-plane lock and migration SQL is untouched', () => {
    const migrations = src('runtime/migrations.ts');
    const postgres = src('runtime/postgres.ts');
    expect(migrations).toContain("'SELECT pg_advisory_lock(714928001)'");
    expect(migrations).toContain("'SELECT pg_advisory_unlock(714928001)'");
    expect(migrations).not.toContain('executiveGenerationJobsSql');
    expect(postgres).not.toContain('executiveGenerationJobsSql');
  });

  test('retirement:ts-statements stays strictly below the r6 BEFORE=53 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(53);
  });
});
