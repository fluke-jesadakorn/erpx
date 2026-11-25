import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const src = (name: string) =>
  new URL(`../../../packages/server-work_projection/src/adapters/${name}`, import.meta.url);

const inListDefinition = '`${expression} IN (SELECT jsonb_array_elements_text(?::jsonb))`';
const formerInListSites = [
  'calendar-appointments-facts.ts',
  'calendar-announcements-facts.ts',
  'calendar-index-facts.ts',
  'calendar/access.ts',
];
const originalBatchSql = {
  series: 'SELECT * FROM calendar_series WHERE company_id=? AND appointment_id IN (SELECT jsonb_array_elements_text(?::jsonb))',
  exceptions: 'SELECT * FROM calendar_occurrence_exceptions WHERE company_id=? AND appointment_id IN (SELECT jsonb_array_elements_text(?::jsonb))',
} as const;

describe('work_projection read-tail r9 dedupe', () => {
  it('keeps one canonical inList fragment and no inline copies', async () => {
    const shared = await readFile(src('read-tail-sql.ts'), 'utf8');
    expect(shared.split(inListDefinition)).toHaveLength(2);
    for (const name of formerInListSites) {
      expect(await readFile(src(name), 'utf8')).not.toContain(inListDefinition);
    }
  });

  it('keeps one canonical batch SQL builder and no inline copies', async () => {
    const source = await readFile(src('calendar-appointments-facts.ts'), 'utf8');
    expect(source).not.toContain('SELECT * FROM calendar_series WHERE company_id=?');
    expect(source).not.toContain('SELECT * FROM calendar_occurrence_exceptions WHERE company_id=?');
    // Lane 29 cutover: the series batch builder is gone; the SQL is owned by
    // the native lane 29 reader and reached through a strict TS client.
    expect(source).not.toContain('byAppointmentIds');
    expect(source).toContain('nativeSeriesBatch(');
  });

  it('keeps the original series SQL in Rust and leaves occurrence batches native', async () => {
    const { inList } = await import(
      '../../../packages/server-work_projection/src/adapters/read-tail-sql.ts'
    );
    const rust = await readFile(src('native_lane_29/postgres.rs'), 'utf8');
    expect(inList('appointment_id')).toBe('appointment_id IN (SELECT jsonb_array_elements_text(?::jsonb))');
    expect(rust).toContain('SELECT * FROM calendar_series WHERE company_id=$1');
    expect(rust).toContain('SELECT jsonb_array_elements_text($2::jsonb)');
    expect(originalBatchSql.series).toBe(
      'SELECT * FROM calendar_series WHERE company_id=? AND appointment_id IN (SELECT jsonb_array_elements_text(?::jsonb))',
    );
    expect(Object.keys(originalBatchSql)).toEqual(['series', 'exceptions']);
    expect(await readFile(src('calendar-appointments-facts.ts'), 'utf8')).not.toContain('byAppointmentIds');
  });
});
