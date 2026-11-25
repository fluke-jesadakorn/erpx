import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const SHARED = 'adapters/pms-read-tail-r20-sql.ts';
const CALL_SITES = [
  'adapters/platform-owner-projection-sources.ts',
  'adapters/hospitality-owner-projection-sources.ts',
  'adapters/commercial-owner-projection-sources.ts',
];
// File-text forms: inner column quotes are backslash-escaped in the TS source.
const PROPERTIES = String.raw`SELECT \"id\",\"company_id\",\"branch_id\",\"name\",\"business_date\",\"state\",\"config_json\",\"version\",\"created_by\",\"updated_at\" FROM pms_properties`;
const RECORDS = String.raw`SELECT \"id\",\"property_id\",\"kind\",\"data_json\",\"version\",\"updated_at\" FROM pms_records`;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r20 byte-identical owner-projection dedupe', () => {
  it('declares each shared SQL text exactly once', async () => {
    const shared = await src(SHARED);
    expect(shared.split(PROPERTIES).length - 1).toBe(1);
    expect(shared.split(RECORDS).length - 1).toBe(1);
  });

  it('routes every projection source through the shared fragments with no inline literal', async () => {
    for (const name of CALL_SITES) {
      const text = await src(name);
      // one import + one use for each constant in every call-site file
      expect(text.split('PMS_PROPERTIES_PROJECTION_SQL').length - 1).toBe(2);
      expect(text.split('PMS_RECORDS_PROJECTION_SQL').length - 1).toBe(2);
      expect(text).not.toContain(PROPERTIES);
      expect(text).not.toContain(RECORDS);
    }
  });

  it('keeps the fragments read-only and leaves distinct fragments/locks inline', async () => {
    for (const sql of [PROPERTIES, RECORDS]) {
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    const hospitality = await src(CALL_SITES[1]);
    expect(hospitality).toContain('pms_postings: PMS_POSTINGS_PROJECTION_SQL');
    expect(hospitality).toContain('pms_sources: "SELECT \\"property_id\\",\\"source_key\\",\\"record_id\\" FROM pms_sources"');
    expect(await src('adapters/pms-stripe.ts')).toContain('FOR UPDATE');
  });

  it('lowers the pms owner SELECT KPI strictly below the r19 after=481 baseline', async () => {
    let count = 0;
    for (const file of await tsFiles(dir)) count += (await readFile(file, 'utf8')).split('SELECT ').length - 1;
    // r21 deduped the 4x parameterized recipient cross-join fragment below r20 after=477.
    expect(count).toBe(216);
    expect(count).toBeLessThan(481);
  });
});
