import { createHash } from 'node:crypto';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';

vi.mock('server-only', () => ({}));

import { projectionColumnHeadSql } from '../../../packages/server-records/src/adapters/read-tail-r32-sql';
import { requestAiUsageCountSql } from '../../../packages/server-records/src/adapters/read-tail-count-sql';

const root = resolve(__dirname, '../../..');
const read = (p: string) => readFileSync(resolve(root, p), 'utf8');
const sha = (s: string) => createHash('sha256').update(s, 'utf8').digest('hex');
const records = read('packages/server-records/src/adapters/records.ts');
const count = read('packages/server-records/src/adapters/read-tail-count-sql.ts');

describe('Records search/count detail generator retirement', () => {
  test('search references still render the exact prior SQL through r32', () => {
    const sql = `${'<CLOCK>'}${projectionColumnHeadSql(
      'module_id,id,title,status,version',
      'workspace_records',
      " WHERE archived_at IS NULL AND company_id=? AND module_id IN (?,?) AND (module_id||'/'||id ILIKE ? OR title ILIKE ?) ORDER BY updated_at DESC LIMIT 12",
    )}`;
    expect(sha(sql)).toBe('2a3ca9a569fc8d6681800e3aa7c66e7b84d5036f3a477a98795debf41cba63b0');
    expect(sql).toContain("<CLOCK>SELECT module_id,id,title,status,version FROM workspace_records WHERE archived_at IS NULL AND ");
    expect(sql).toContain(' ORDER BY updated_at DESC LIMIT 12');
  });

  test('the count detail generator renders both quota subqueries byte-identically', () => {
    const quota = requestAiUsageCountSql("company_id=? AND state='reserved' AND lease_until>?");
    const rate = requestAiUsageCountSql('actor_id=? AND created_at>?');
    expect(quota).toBe("(SELECT count(*) FROM request_ai_usage WHERE company_id=? AND state='reserved' AND lease_until>?)");
    expect(sha(quota)).toBe('8876dbee4f0260a0b4ccd90aecc87c52f33b8abcb613790979d640989085c779');
    expect(sha(rate)).toBe('7a07baa5c8d9fdfe586b34d4146a6fc4c4b8a17a305383261178083b844642be');
    expect(count).not.toMatch(/SELECT /);
    expect(count).toContain('import { projectionColumnHeadSql } from \'./read-tail-r32-sql\';');
  });

  test('the replaced TS SQL is deleted and both retired members fail closed', () => {
    expect(records).not.toContain('recordContextDetailSql');
    expect(records).not.toContain('referenceEdgesSql');
    expect(records).not.toMatch(/SELECT /);
    expect(records).toContain('readAddReferenceCount() { throw new NativeApiError(503');
    expect(records.match(/readRecordContextEdgesbacklinksmentionssuggestionshistory\d*\(\) \{ throw new NativeApiError\(503/g) ?? []).toHaveLength(5);
    expect(records).toContain("${purchase.clock}${projectionColumnHeadSql('module_id,id,title,status,version','workspace_records',` WHERE archived_at IS NULL AND ${scope.sql}");
  });

  test('owner retirement:ts-statements drops 11 -> 10', () => {
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
    expect(total).toBeLessThan(11);
  });
});
