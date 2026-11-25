import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import { billingAccountProjectionSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r30-sql';
import { hospitalityOwnerProjectionSources } from '../../../packages/server-billing/src/adapters/hospitality-owner-projection-sources';

const srcRoot = new URL('../../../packages/server-billing/src/', import.meta.url);
const src = (name: string) => readFile(new URL(`adapters/${name}`, srcRoot), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

/** Verbatim pre-r30 account projections captured at BEFORE=55 (HEAD 6a8bc93ac).
 * SHA-256 values are byte-identity oracles only, never live query sources. */
const ORIGINALS = [
  {
    file: 'billing-jobs.ts',
    full: 'SELECT holding_id FROM billing_accounts',
    columns: 'holding_id',
    sha256: '24239c58bc1f04279cc3ab7659f2cf4912bf3ae71da566e66b46e63620057940',
    routed: "billingAccountProjectionSql('holding_id')",
  },
  {
    file: 'hospitality-owner-projection-sources.ts',
    full: 'SELECT "holding_id","version","payment_mode" FROM billing_accounts',
    columns: '"holding_id","version","payment_mode"',
    sha256: '54815a56720b5e543f57187aa13bdd7a34b2906b5091d2f0ecd715c3350185c3',
    routed: `billingAccountProjectionSql('"holding_id","version","payment_mode"')`,
  },
] as const;

async function tsFiles(directory: URL): Promise<string[]> {
  const found: string[] = [];
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    if (entry.isDirectory()) found.push(...await tsFiles(new URL(`${entry.name}/`, directory)));
    else if (entry.isFile() && entry.name.endsWith('.ts')) found.push(new URL(entry.name, directory).pathname);
  }
  return found;
}

describe('billing read-tail r30 whole-account-projection dedupe', () => {
  it('authors the projection head exactly once as read-only text', async () => {
    const helper = await src('billing-read-tail-r30-sql.ts');
    expect(helper.split('SELECT').length - 1).toBe(0); // r35 generic projection head owns the SELECT
    expect(helper).toContain("from './billing-read-tail-r35-sql'");
    expect(helper.split('billingAccountProjectionSql').length - 1).toBe(1);
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('recomposes both projections byte-for-byte with unchanged columns', () => {
    for (const oracle of ORIGINALS) {
      expect(sha256(oracle.full)).toBe(oracle.sha256);
      expect(billingAccountProjectionSql(oracle.columns)).toBe(oracle.full);
      expect(oracle.full).not.toContain('$');
    }
  });

  it('routes every call site through the helper and removes the inline literals', async () => {
    for (const oracle of ORIGINALS) {
      const text = await src(oracle.file);
      expect(text.split(oracle.full).length - 1).toBe(0);
      expect(text.split(oracle.routed).length - 1).toBe(1);
      expect(text).toContain("./billing-read-tail-r30-sql");
    }
  });

  it('keeps the runtime projection value byte-identical for the native join consumer', () => {
    expect(hospitalityOwnerProjectionSources.billing_accounts).toBe(ORIGINALS[1].full);
    expect(sha256(hospitalityOwnerProjectionSources.billing_accounts)).toBe(ORIGINALS[1].sha256);
  });

  it('lowers the billing owner SELECT KPI from BEFORE=55 to 42', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(srcRoot)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(7);
    expect(bare).toBe(7);
    expect(spaced).toBeLessThan(48);
    expect(bare).toBeLessThan(48);
  });
});
