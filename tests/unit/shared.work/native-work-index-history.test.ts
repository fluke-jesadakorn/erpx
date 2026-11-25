import { describe, expect, it } from 'vitest';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';

const src = (relative: string) =>
  new URL(`../../../packages/${relative}`, import.meta.url);
const hash = (value: string) => createHash('sha256').update(value).digest('hex');

/** The retired TypeScript literal after inlining its two fixed fragments:
 * `inList('record_id')` and the r45 people projection. Only ? -> $n is native. */
const NATIVE_SQL =
  "SELECT h.record_id,h.action,coalesce(p.legal_name,'Authorized colleague') actor,h.created_at at " +
  'FROM (SELECT *,row_number() OVER(PARTITION BY record_id ORDER BY created_at DESC,id DESC) ordinal ' +
  'FROM calendar_history WHERE company_id=$1 AND record_id IN (SELECT jsonb_array_elements_text($2::jsonb))) h ' +
  'LEFT JOIN (SELECT id,user_id,company_id,legal_name,merged_into_id FROM hr_people) p ' +
  'ON p.user_id=h.actor_id AND p.company_id=h.company_id AND p.merged_into_id IS NULL ' +
  'WHERE ordinal<=20 ORDER BY h.created_at DESC';

describe('native Work index history (read-tail r20 sibling)', () => {
  it('keeps the owner SQL byte-identical to the retired TypeScript literal', async () => {
    const rust = await readFile(src('server-work_projection/src/adapters/calendar_read.rs'), 'utf8');
    expect(rust).toContain(`pub const CALENDAR_HISTORY_SQL: &str = "${NATIVE_SQL}"`);
    expect(rust).toContain('READ_69');
    expect(hash(NATIVE_SQL)).toBe('541bb31b09d37dd5208fbee258d95432c615d588051cb398505c245f1c17b104');
  });

  it('removes every fragment of the TypeScript SQL from the index facts adapter', async () => {
    const facts = await readFile(src('server-work_projection/src/adapters/calendar-index-facts.ts'), 'utf8');
    expect(facts).not.toContain('SELECT');
    expect(facts).not.toContain('calendarPeopleProjectionSources');
    expect(facts).not.toContain('inList(');
    expect(facts).not.toContain('.bind(facts.value1, facts.value2)');
    expect(facts).toContain('nativeCalendarRead.read69(facts)');
  });

  it('pins the client wire kind, positional values and strict row shape', async () => {
    const client = await readFile(src('feature-work_projection/src/lib/native-calendar-read.ts'), 'utf8');
    expect(client).toContain("read('wp_cal_read_69', [facts.value1, facts.value2], facts.value1)");
    expect(client).toContain('record_id: string; action: string; actor: string; at: number');
    expect(client).toContain('Object.keys(row).length !== 4');
  });
});
