import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { PMS_POSTINGS_FROM_SQL } from '../../../packages/server-pms/src/adapters/pms-read-tail-r19-sql';
import { PMS_POSTINGS_PROJECTION_SQL } from '../../../packages/server-pms/src/adapters/pms-read-tail-r29-sql';
import { pmsOwnerProjectionHeadSql } from '../../../packages/server-pms/src/adapters/pms-read-tail-r53-sql';
import { financeOwnedSnapshotSources } from '../../../packages/server-pms/src/adapters/finance-owned-snapshot-sources';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');
const count = (text: string, needle: string) => text.split(needle).length - 1;

// The byte-identical bare pms_postings projection head retired at HEAD (KPI 246).
// The r19 `FROM (...) pms_postings` fragment and the r29 bare projection opened
// with these exact bytes; the renderer must reproduce them verbatim.
const HEAD = 'SELECT "id","property_id","source_key","record_id","journal_id","business_date" FROM pms_postings';
const HEAD_SHA = '1423db312b6135c3f1e8baee11d357947f428b9fc6e23eb904592af7f3145674';
const WRAPPED = `FROM (${HEAD}) pms_postings`;
const WRAPPED_SHA = 'd747414d4b8fec6b2f77c8ff2c515d001dc86d4e9684aa2cf278ad585fb72e36';
const PP = `FROM (${HEAD}) pp`;
const PP_SHA = '641047325a25435896d6339be296c97541f2caf34a5551a1c8cab38f6c9d5fc7';
// File-text form: inner column quotes are backslash-escaped in TS source.
const SOURCE_HEAD = String.raw`SELECT \"id\",\"property_id\",\"source_key\",\"record_id\",\"journal_id\",\"business_date\" FROM pms_postings`;

const RENDERER = 'adapters/pms-read-tail-r53-sql.ts';
const R19 = 'adapters/pms-read-tail-r19-sql.ts';
const R29 = 'adapters/pms-read-tail-r29-sql.ts';
const HELPER = "pmsOwnerProjectionHeadSql('postings')";

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r53 owner projection head dedupe', () => {
  it('pins the exact HEAD bytes of the shared postings head', () => {
    expect(sha256(HEAD), 'head sha').toBe(HEAD_SHA);
    expect(HEAD.length, 'head length').toBe(97);
    expect(count(HEAD, 'SELECT '), 'head selects').toBe(1);
    expect(count(HEAD, '?'), 'head binds').toBe(0);
    expect(sha256(WRAPPED), 'wrapped sha').toBe(WRAPPED_SHA);
    expect(WRAPPED.length, 'wrapped length').toBe(117);
    expect(sha256(PP), 'pp sha').toBe(PP_SHA);
    expect(PP.length, 'pp length').toBe(107);
  });

  it('declares one closed owner-local renderer with no caller SQL or write seam', async () => {
    const shared = await src(RENDERER);
    expect(count(shared, 'pmsOwnerProjectionHeadSql')).toBe(1);
    expect(shared).toContain("type Head = 'postings'");
    expect(shared).toContain('const HEAD: Record<Head, string>');
    expect(shared).toContain(HEAD);
    expect(count(shared, 'SELECT '), 'renderer adds one SELECT token').toBe(1);
    expect(shared).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('recomposes the bare head byte-identically with zero binds', () => {
    const rendered = pmsOwnerProjectionHeadSql('postings');
    expect(rendered).toBe(HEAD);
    expect(sha256(rendered)).toBe(HEAD_SHA);
    expect(count(rendered, 'SELECT ')).toBe(1);
    expect(count(rendered, '?')).toBe(0);
  });

  it('routes both sites through the helper and retires the inline heads', async () => {
    const r19 = await src(R19);
    const r29 = await src(R29);
    expect(count(r19, HELPER)).toBe(1);
    expect(count(r29, HELPER)).toBe(1);
    expect(r19).toContain(`export const PMS_POSTINGS_FROM_SQL = \`FROM (\${${HELPER}}) pms_postings\`;`);
    expect(r29).toContain(`export const PMS_POSTINGS_PROJECTION_SQL = ${HELPER};`);
    expect(count(r19, SOURCE_HEAD), 'r19 inline head retired').toBe(0);
    expect(count(r29, SOURCE_HEAD), 'r29 inline head retired').toBe(0);
    // Distinct fragments and bind order stay byte-identical at both sites.
    expect(r19).toContain('export const PMS_IMPORT_BATCHES_FROM_SQL = "FROM (SELECT \\"id\\",\\"company_id\\",');
    expect(r29).toContain('export const PMS_PROPERTIES_TEN_COL_JOIN_SQL = "JOIN (SELECT \\"id\\",\\"company_id\\",');
    expect(r29).toContain('status: \'payment_pending\' | \'confirmed\',');
  });

  it('renders byte-identical owner and consumer SQL', () => {
    expect(PMS_POSTINGS_FROM_SQL).toBe(WRAPPED);
    expect(sha256(PMS_POSTINGS_FROM_SQL)).toBe(WRAPPED_SHA);
    expect(PMS_POSTINGS_PROJECTION_SQL).toBe(HEAD);
    expect(sha256(PMS_POSTINGS_PROJECTION_SQL)).toBe(HEAD_SHA);
    const sources = financeOwnedSnapshotSources() as unknown as Record<string, string>;
    expect(count(sources['pms_command_facts_pms_postings_from']!, 'SELECT ')).toBe(1);
    expect(sources['pms_command_facts_pms_postings_from']).toBe(WRAPPED);
    expect(sources['pms_finance_facts_pms_postings_from']).toBe(WRAPPED);
    expect(sources['pms_record_details_facts_pms_postings_from']).toBe(WRAPPED);
    expect(sources['pms_reporting_facts_pms_postings_from']).toBe(WRAPPED);
    expect(sources['pms_finance_facts_pms_postings_pp_from']).toBe(PP);
    expect(sha256(sources['pms_finance_facts_pms_postings_pp_from']!)).toBe(PP_SHA);
  });

  it('leaves the properties family, native registry, locks and prior helpers untouched', async () => {
    const r20 = await src('adapters/pms-read-tail-r20-sql.ts');
    const r29 = await src(R29);
    expect(r20).toContain('export const PMS_PROPERTIES_PROJECTION_SQL = "SELECT \\"id\\",\\"company_id\\",');
    expect(r29).toContain('export const PMS_PROPERTIES_TEN_COL_JOIN_SQL = "JOIN (SELECT \\"id\\",\\"company_id\\",');
    const rust = await src('adapters/native_read_sql.rs');
    expect((rust.match(/^pub const PMS_RD_/gm) ?? []).length).toBe(33);
    expect(rust).toContain('const SQL_STAY_COMMAND: &str = "WITH scope AS (');
    expect(rust).toContain('pub const PMS_RD_STAY_COMMAND: &str = "pms_rd_stay_command";');
    expect(rust).toContain('pub const PMS_RD_SECTION_RELATED_RECORDS: &str = "pms_rd_section_related_records";');
    const bridge = await src('adapters/native-pms-read.ts');
    expect((bridge.match(/^export const PMS_RD_/gm) ?? []).length).toBe(33);
    expect(await src('adapters/pms.ts')).toContain('FOR UPDATE OF p');
    expect(await src('adapters/pms-stripe.ts')).toContain('FOR UPDATE');
    expect(await src('adapters/feature-availability.ts')).toContain('FOR SHARE');
    expect(await src('adapters/attachment-read-sources.ts')).toContain('LOCK TABLE');
    for (const n of [17, 18, 19, 20, 21, 22, 24, 25, 26, 27, 29, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 49, 50, 51, 53]) {
      expect((await src(`adapters/pms-read-tail-r${n}-sql.ts`)).length, `r${n}`).toBeGreaterThan(0);
    }
  });

  it('lowers the pms owner SELECT KPI from 246 to 245', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(dir)) {
      const text = await readFile(file, 'utf8');
      spaced += count(text, 'SELECT ');
      bare += count(text, 'SELECT');
    }
    expect(spaced).toBe(241);
    expect(bare).toBe(243);
    expect(spaced).toBeLessThan(246);
    expect(bare).toBeLessThan(248);
  });
});
