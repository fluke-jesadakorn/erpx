import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const SHARED = 'adapters/pms-read-tail-r19-sql.ts';
const CALL_SITE = 'adapters/finance-owned-snapshot-sources.ts';
// File-text forms: inner column quotes are backslash-escaped in the TS source.
const IMPORT_BATCHES = String.raw`FROM (SELECT \"id\",\"company_id\",\"property_id\",\"cutover_date\",\"property_version\",\"definition_json\",\"report_json\",\"state\",\"created_by\",\"approved_by\",\"opening_journal_id\",\"version\",\"created_at\",\"updated_at\" FROM pms_import_batches) pms_import_batches`;
const POSTINGS = String.raw`FROM (SELECT \"id\",\"property_id\",\"source_key\",\"record_id\",\"journal_id\",\"business_date\" FROM pms_postings) pms_postings`;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r19 byte-identical projection-fragment dedupe', () => {
  it('declares each shared SQL text exactly once', async () => {
    const shared = await src(SHARED);
    expect(shared.split(IMPORT_BATCHES).length - 1).toBe(1);
    expect(shared.split(POSTINGS).length - 1).toBe(1);
  });

  it('routes every projection source through the shared fragments with no inline literal', async () => {
    const text = await src(CALL_SITE);
    // one import + four call sites for each fragment
    expect(text.split('PMS_IMPORT_BATCHES_FROM_SQL').length - 1).toBe(5);
    expect(text.split('PMS_POSTINGS_FROM_SQL').length - 1).toBe(5);
    expect(text).not.toContain(IMPORT_BATCHES);
    expect(text).not.toContain(POSTINGS);
  });

  it('keeps the fragments read-only and leaves distinct aliases inline', async () => {
    for (const sql of [IMPORT_BATCHES, POSTINGS]) {
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    }
    const text = await src(CALL_SITE);
    // distinct aliases are different fragments and stay inline
    expect(text).toContain('FROM pms_postings) pp');
    expect(text).toContain('FROM pms_import_batches) b');
  });

  it('lowers the pms owner SELECT KPI strictly below the r18 after=487 baseline', async () => {
    let count = 0;
    for (const file of await tsFiles(dir)) count += (await readFile(file, 'utf8')).split('SELECT ').length - 1;
    expect(count).toBe(481);
    expect(count).toBeLessThan(487);
  });
});
