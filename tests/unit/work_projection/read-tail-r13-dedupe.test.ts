import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const src = (name: string) =>
  new URL(`../../../modules/work-projection/server/src/adapters/${name}`, import.meta.url);

const original = {
  snapshot: 'SELECT table_name,revision FROM work_cache_revisions ORDER BY table_name',
  watermarks: (tables: string[]) =>
    `SELECT table_name,revision::text revision FROM work_cache_revisions WHERE table_name IN (${tables.map(() => '?').join(',')}) ORDER BY table_name`,
} as const;

describe('work_projection read-tail r13 dedupe', () => {
  it('keeps one canonical work_cache_revisions read and no inline copies', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    expect(shared.split('export const workCacheRevisionsSql =')).toHaveLength(2);
    const app = await readFile(src('app-executive-cache-source.ts'), 'utf8');
    const metric = await readFile(src('executive-metric-watermarks.ts'), 'utf8');
    expect(app).not.toContain('FROM work_cache_revisions');
    expect(metric).not.toContain('FROM work_cache_revisions');
    expect(app).toContain('workCacheRevisionsSql()');
    expect(metric).toContain('workCacheRevisionsSql(sourceTables)');
  });

  it('emits the original byte-identical SQL for both callers', async () => {
    const { workCacheRevisionsSql } = await import(
      '../../../modules/work-projection/server/src/adapters/read-tail-sql.ts'
    );
    expect(workCacheRevisionsSql()).toBe(original.snapshot);
    expect(workCacheRevisionsSql(['legal_items'])).toBe(original.watermarks(['legal_items']));
    expect(workCacheRevisionsSql(['a', 'b', 'c'])).toBe(original.watermarks(['a', 'b', 'c']));
  });

  it('leaves the work-cache fence and lock statements untouched', async () => {
    const fence = await readFile(src('work-cache.ts'), 'utf8');
    expect(fence).toContain("FROM work_cache_revisions WHERE table_name <> 'legal_runtime_health'");
    expect(fence).toContain('NOT EXISTS (SELECT 1 FROM work_cache_revisions r WHERE r.table_name=t.tablename)');
    expect(await readFile(src('daily-plan.ts'), 'utf8')).toContain(
      'SELECT version FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=? FOR UPDATE',
    );
  });
});
