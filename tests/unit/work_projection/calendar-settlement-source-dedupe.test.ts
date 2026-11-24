import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const adapter = () =>
  new URL('../../../packages/server-work_projection/src/adapters/work-calendar-package-obligation-sources.ts', import.meta.url);

const original = {
  announcements: 'SELECT * FROM calendar_announcements',
  appointments: 'SELECT * FROM calendar_appointments',
  attendees: 'SELECT * FROM calendar_attendees',
  series: 'SELECT * FROM calendar_series',
  exceptions: 'SELECT * FROM calendar_occurrence_exceptions',
  responses: 'SELECT * FROM calendar_occurrence_responses',
  reminders: 'SELECT * FROM calendar_reminder_deliveries',
  history: 'SELECT * FROM calendar_history',
} as const;

describe('work calendar settlement source dedupe', () => {
  it('keeps one canonical SELECT literal and no inline copies in the source', async () => {
    const source = await readFile(adapter(), 'utf8');
    for (const sql of Object.values(original)) expect(source).not.toContain(`'${sql}'`);
    expect(source).toContain('settlementSourceRows');
    expect(source.match(/return `SELECT \* FROM \$\{table\}`/g)).toHaveLength(1);
  });

  it('emits the original byte-identical settlement source statements', async () => {
    const { workCalendarSettlementSources } = await import(
      '../../../packages/server-work_projection/src/adapters/work-calendar-package-obligation-sources.ts'
    );
    for (const [variant, sql] of Object.entries(original)) {
      expect(workCalendarSettlementSources[variant as keyof typeof original]).toBe(sql);
    }
  });
});
