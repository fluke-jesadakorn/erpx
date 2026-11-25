import { createHash } from 'node:crypto';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { requestAiUsageMonthlyCountSql } from '../../../packages/server-records/src/adapters/ai-usage-projection';

const root = resolve(__dirname, '../../..');
const read = (p: string) => readFileSync(resolve(root, p), 'utf8');
const sha = (s: string) => createHash('sha256').update(s, 'utf8').digest('hex');
const ai = read('packages/server-records/src/adapters/ai-usage-projection.ts');

describe('Records request_ai_usage monthly count head cutover', () => {
  test('the parameterized count renders byte-identically after the r32 delegation', () => {
    const sql = requestAiUsageMonthlyCountSql;
    expect(sql).toBe('SELECT count(*)::int count FROM request_ai_usage WHERE company_id=? AND month=?');
    expect(sha(sql)).toBe('3558f43a6369e72cc812b08c6c7691cbef7b8e916b1ba49de08bdb45f61122b4');
    expect(sql).toContain('count(*)::int count');
    expect(sql).toContain('company_id=? AND month=?');
    expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('both retained consumers keep the same fixed SQL and no inlined literal remains', () => {
    expect(ai).not.toMatch(/SELECT /);
    expect(ai).toContain("projectionColumnHeadSql(\n  'count(*)::int count',");
    expect(ai).toContain("' WHERE company_id=? AND month=?',");
    const beta = read('packages/server-records/src/adapters/beta-management-sources.ts');
    const agent = read('packages/server-records/src/adapters/agent-ai-usage-projection.ts');
    expect(beta).toContain('requestAiUsageMonthlyCountSql');
    expect(agent).toContain('requestAiUsageMonthlyCountSql');
    expect(beta).not.toMatch(/SELECT /);
    expect(agent).not.toMatch(/SELECT /);
  });

  test('owner retirement:ts-statements drops 10 -> 9', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(resolve(root, 'packages/server-records/src'));
    expect(total).toBe(6);
    expect(total).toBeLessThan(10);
  });
});
