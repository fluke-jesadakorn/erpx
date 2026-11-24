import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const src = (name: string) =>
  new URL(`../../../packages/server-work_projection/src/adapters/${name}`, import.meta.url);

const original = {
  pageLayout: 'SELECT request_hash,result_json FROM page_layout_receipts WHERE id=?',
  quickAccess: 'SELECT user_id,surface,context_key,request_hash,result_json FROM quick_access_preference_receipts WHERE id=?',
} as const;

describe('work_projection read-tail r17 dedupe', () => {
  it('keeps one canonical receipt point-read builder and no inline copies', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    expect(shared.split('export const receiptByIdSql =')).toHaveLength(2);
    for (const name of ['page-layouts.ts', 'quick-access-preferences.ts']) {
      const source = await readFile(src(name), 'utf8');
      expect(source).not.toContain('FROM page_layout_receipts WHERE id=?');
      expect(source).not.toContain('FROM quick_access_preference_receipts WHERE id=?');
      expect(source).toContain('receiptByIdSql(');
    }
  });

  it('emits the original byte-identical SQL for both receipt callers', async () => {
    const { receiptByIdSql } = await import(
      '../../../packages/server-work_projection/src/adapters/read-tail-sql.ts'
    );
    expect(receiptByIdSql('request_hash,result_json', 'page_layout_receipts')).toBe(original.pageLayout);
    expect(receiptByIdSql('user_id,surface,context_key,request_hash,result_json', 'quick_access_preference_receipts')).toBe(original.quickAccess);
  });

  it('leaves the r9-r16 helpers, locks and retained consumers intact', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    for (const helper of [
      'inList', 'versionById', 'noopSql', 'appointmentProbe', 'invitedAttendeeSql',
      'workCacheRevisionsSql', 'overlapProbe', 'occurrenceExceptionPointSql',
      'reminderAttendeePointSql', 'attendeeScopeExistsSql',
    ]) {
      expect(shared).toContain(`export const ${helper} =`);
    }
    expect(await readFile(src('daily-plan.ts'), 'utf8')).toContain(
      'SELECT version FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=? FOR UPDATE',
    );
    expect(await readFile(src('page-layouts.ts'), 'utf8')).toContain(
      'SELECT pg_advisory_xact_lock(hashtextextended(?,0))',
    );
    expect(await readFile(src('page-layouts.ts'), 'utf8')).toContain('pageLayoutRequest');
    expect(await readFile(src('quick-access-preferences.ts'), 'utf8')).toContain('readQuickAccessPreferences');
  });
});
