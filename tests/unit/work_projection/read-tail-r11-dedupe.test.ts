import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const src = (name: string) =>
  new URL(`../../../modules/work-projection/server/src/adapters/${name}`, import.meta.url);

const noopDefinition = 'export const noopSql = "SELECT 1";';
const formerNoopSites = [
  'calendar-commands-facts.ts',
  'calendar-reminders-facts.ts',
  'calendar/reminder-authority.ts',
  'daily-plan.ts',
  'page-layouts.ts',
  'quick-access-preferences.ts',
  'workflow-projector.ts',
];

describe('work_projection read-tail r11 dedupe', () => {
  it('keeps one canonical inert noop SQL definition and no inline copies', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    expect(shared.split(noopDefinition)).toHaveLength(2);
    for (const name of formerNoopSites) {
      const source = await readFile(src(name), 'utf8');
      expect(source).not.toContain('"SELECT 1"');
      expect(source).toContain('noopSql');
    }
  });

  it('emits the original byte-identical inert SQL for every native owner write', async () => {
    const { noopSql } = await import(
      '../../../modules/work-projection/server/src/adapters/read-tail-sql.ts'
    );
    expect(noopSql).toBe('SELECT 1');
  });

  it('leaves the daily-plan and page-layout lock statements untouched', async () => {
    expect(await readFile(src('daily-plan.ts'), 'utf8')).toContain(
      'SELECT version FROM staff_daily_plans WHERE company_id=? AND user_id=? AND plan_date=? FOR UPDATE',
    );
    expect(await readFile(src('page-layouts.ts'), 'utf8')).toContain(
      'SELECT pg_advisory_xact_lock(hashtextextended(?,0))',
    );
  });
});
