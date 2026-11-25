import { readFile, readdir } from 'node:fs/promises';
import { describe, expect, it } from 'vitest';

const dir = new URL('../../../packages/server-pms/src/', import.meta.url);
const src = (name: string) => readFile(new URL(name, dir), 'utf8');
const SHARED = 'adapters/pms-read-tail-r23-sql.ts';

const SCOPE_SELECT = 'SELECT r.id,r.kind,r.data_json,r.version,r.updated_at FROM pms_records r,scope q';
const CALL_SITE = 'adapters/pms.ts';

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('pms read-tail r23 byte-identical scope-read fragment dedupe', () => {
  it('declares the shared SQL text exactly once', async () => {
    expect((await src(SHARED)).split(SCOPE_SELECT).length - 1).toBe(1);
  });

  it('routes every scoped records reader through the shared fragment with no inline literal', async () => {
    const text = await src(CALL_SITE);
    expect(text.split('PMS_SCOPED_RECORDS_SQL').length - 1).toBe(0); // r28 native port removed all 34 uses and the import
    expect(text).not.toContain(SCOPE_SELECT);
  });

  it('keeps the fragment read-only, leaves locks inline and does not re-dedupe r17-r22', async () => {
    expect(SCOPE_SELECT).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    expect(await src(CALL_SITE)).toContain('FOR UPDATE OF p');
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

  it('lowers the pms owner SELECT KPI strictly below the r22 after=468 baseline', async () => {
    let count = 0;
    for (const file of await tsFiles(dir)) count += (await readFile(file, 'utf8')).split('SELECT ').length - 1;
    expect(count).toBe(258);
    expect(count).toBeLessThan(468);
  });
});
