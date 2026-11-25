import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const GUARD = "SELECT set_config('erpx.pms_write','on',true)";
const SHARED = 'adapters/pms-read-tail-r17-sql.ts';
const CALL_SITES = [
  'adapters/pms-stripe.ts',
  'adapters/pms-booking.ts',
  'adapters/pms/import.ts',
  'adapters/pms-booking-changes.ts',
  'adapters/pms.ts',
  'adapters/pms-booking-refunds.ts',
];

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r17 byte-identical guard dedupe', () => {
  it('declares the shared SQL text exactly once', async () => {
    expect((await src(SHARED)).split(GUARD).length - 1).toBe(1);
  });

  it('routes every call site through the shared fragment with byte-identical SQL', async () => {
    let usages = 0;
    for (const name of CALL_SITES) {
      const text = await src(name);
      usages += text.split('PMS_WRITE_CONTEXT_SQL').length - 1;
      expect(text).not.toContain(`"${GUARD}"`);
    }
    // one import + 12 call sites per the owners; the constant declaration lives in SHARED
    expect(usages).toBe(18);
  });

  it('keeps the fragment read-only and leaves advisory/FOR UPDATE reads untouched', async () => {
    expect(GUARD).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    const pms = await src('adapters/pms.ts');
    expect(pms).toContain("WITH command_context AS MATERIALIZED (SELECT set_config('erpx.pms_write','on',true),set_config('erpx.pms_property',?::text,true))");
    expect(pms).toContain('FOR UPDATE OF p');
  });

  it('lowers the pms owner SELECT KPI strictly below the r16 after=501 baseline', async () => {
    let count = 0;
    for (const file of await tsFiles(dir)) count += (await readFile(file, 'utf8')).split('SELECT').length - 1;
    // r18 deduped the 4x booking-payment point read below the r17 after=492 baseline;
    // r19 deduped the two 4x import-batches/postings projection fragments below r18 after=489;
    // r20 deduped the 3x pms_properties and 3x pms_records projections below r19 after=481.
    // r21 deduped the 4x parameterized recipient cross-join fragment below r20 after=477.
    expect(count).toBe(203);
    expect(count).toBeLessThan(501);
  });
});
