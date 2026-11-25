import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const SHARED = 'adapters/pms-read-tail-r24-sql.ts';

const PREFIX = 'SELECT id,kind,data_json,version,updated_at FROM pms_records';
const PMS = 'adapters/pms.ts';
const REPO = 'adapters/pms/read-repository.ts';

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r24 byte-identical pms_records projection prefix dedupe', () => {
  it('declares the shared SQL text exactly once', async () => {
    expect((await src(SHARED)).split(PREFIX).length - 1).toBe(1);
  });

  it('routes every call site through the shared fragment with no inline literal', async () => {
    const pms = await src(PMS);
    const repo = await src(REPO);
    expect(pms.split('PMS_RECORDS_PROJECTION_PREFIX_SQL').length - 1).toBe(5); // import + 4 uses
    expect(repo.split('PMS_RECORDS_PROJECTION_PREFIX_SQL').length - 1).toBe(11); // import + 10 uses after r48
    expect(pms).not.toContain(PREFIX);
    expect(repo).not.toContain(PREFIX);
  });

  it('keeps the fragment read-only, leaves locks inline and does not re-dedupe r17-r22', async () => {
    expect(PREFIX).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    expect(await src(PMS)).toContain('FOR UPDATE OF p');
    expect(await src('adapters/pms-stripe.ts')).toContain('FOR UPDATE');
    expect(await src('adapters/feature-availability.ts')).toContain('FOR SHARE');
    const prior = [
      ['adapters/pms-read-tail-r17-sql.ts', 'PMS_WRITE_CONTEXT_SQL'],
      ['adapters/pms-read-tail-r18-sql.ts', 'PMS_BOOKING_PAYMENT_BY_ID_SQL'],
      ['adapters/pms-read-tail-r19-sql.ts', 'PMS_IMPORT_BATCHES_FROM_SQL'],
      ['adapters/pms-read-tail-r20-sql.ts', 'PMS_PROPERTIES_PROJECTION_SQL'],
      ['adapters/pms-read-tail-r21-sql.ts', 'PMS_RECIPIENT_CROSS_JOIN_SQL'],
      ['adapters/pms-read-tail-r22-sql.ts', 'PMS_STRIPE_REFUND_COLUMNS_SQL'],
    ];
    for (const [file, constant] of prior) {
      expect((await src(file)).split(constant).length - 1).toBe(1);
    }
  });

  it('lowers the pms owner SELECT KPI strictly below the r23 after=435 baseline', async () => {
    let count = 0;
    for (const file of await tsFiles(dir)) count += (await readFile(file, 'utf8')).split('SELECT ').length - 1;
    expect(count).toBe(236);
    expect(count).toBeLessThan(435);
  });
});
