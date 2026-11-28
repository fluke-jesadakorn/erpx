import { describe, expect, it } from 'vitest';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';

const src = (relative: string) =>
  new URL(`../../../packages/${relative}`, import.meta.url);
const hash = (value: string) => createHash('sha256').update(value).digest('hex');

/** The exact retired TypeScript literal; only `?` -> `$n` is native. */
const TS_SQL =
  'SELECT (extract(epoch FROM (?::timestamp AT TIME ZONE ?))*1000)::bigint from_ms,' +
  '(extract(epoch FROM (?::timestamp AT TIME ZONE ?))*1000)::bigint to_ms';
const NATIVE_SQL =
  'SELECT (extract(epoch FROM ($1::timestamp AT TIME ZONE $2))*1000)::bigint from_ms,' +
  '(extract(epoch FROM ($3::timestamp AT TIME ZONE $4))*1000)::bigint to_ms';

describe('native Work index interval bounds (read-tail r20 sibling)', () => {
  it('keeps the owner SQL byte-identical to the retired TypeScript literal', async () => {
    const rust = await readFile(src('server-work_projection/src/adapters/calendar_read.rs'), 'utf8');
    expect(rust).toContain(`pub const INTERVAL_BOUNDS_SQL: &str = "${NATIVE_SQL}"`);
    expect(rust).toContain('READ_68');
    expect(hash(NATIVE_SQL)).toBe('84cb1cbf466f34450b94fa78287bdf16b0463efeb77cc61667e2329207e67cb7');
    expect(hash(TS_SQL)).not.toBe(hash(NATIVE_SQL));
  });

  it('removes the TypeScript SQL and binds from the index facts adapter', async () => {
    const facts = await readFile(src('server-work_projection/src/adapters/calendar-index-facts.ts'), 'utf8');
    expect(facts).not.toContain(TS_SQL);
    expect(facts).not.toContain('extract(epoch FROM');
    expect(facts).not.toMatch(/read2[\s\S]{0,400}\.bind\(facts\.value1, facts\.value2, facts\.value3, facts\.value2\)/);
    expect(facts).toContain('nativeCalendarRead.read68(facts)');
  });

  it('pins the client wire kind, positional values and strict response shape', async () => {
    const client = await readFile(src('feature-work_projection/src/lib/native-calendar-read.ts'), 'utf8');
    expect(client).toContain("read('wp_cal_read_68', [facts.value1, facts.value2, facts.value3, facts.value2], '')");
    expect(client).toContain('NATIVE_WORK_CALENDAR_READ_RESPONSE_INVALID');
    expect(client).toContain('from_ms: number; to_ms: number');
  });
});
