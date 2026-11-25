import { readFile, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { describe, expect, it } from 'vitest';
import { billingTargetMonthSql } from '../../../packages/server-billing/src/adapters/billing-read-tail-r31-sql';

const srcRoot = new URL('../../../packages/server-billing/src/', import.meta.url);
const src = (name: string) => readFile(new URL(`adapters/${name}`, srcRoot), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

/** Verbatim pre-r31 target-month reads captured at BEFORE=54 (HEAD fa51930df).
 * SHA-256 values are byte-identity oracles only, never live query sources. */
const ORIGINALS = [
  {
    file: 'billing-target.ts',
    full: "SELECT * FROM billing_target_rated_periods WHERE holding_id=$1 AND month=$2 AND state='approved'",
    columns: '*',
    table: 'billing_target_rated_periods',
    tail: " AND state='approved'",
    sha256: '191f02eebdacd77a160f5f20bc46c3851a0be863ae2be970b68a2fdd422be13b',
    routed: "billingTargetMonthSql('*','billing_target_rated_periods',\" AND state='approved'\")",
  },
  {
    file: 'billing-target.ts',
    full: 'SELECT id,kind,description,amount_satang,evidence_id FROM billing_target_extras WHERE holding_id=$1 AND month=$2 ORDER BY approved_at,id',
    columns: 'id,kind,description,amount_satang,evidence_id',
    table: 'billing_target_extras',
    tail: ' ORDER BY approved_at,id',
    sha256: '1b11d3f44151e90e6f036f22f81bf76da766bcd2159bfe260d1ec020f6c06a0d',
    routed: "billingTargetMonthSql('id,kind,description,amount_satang,evidence_id','billing_target_extras',' ORDER BY approved_at,id')",
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

describe('billing read-tail r31 target-month read dedupe', () => {
  it('authors the parameterized target-month text exactly once as read-only text', async () => {
    const helper = await src('billing-read-tail-r31-sql.ts');
    expect(helper.split('SELECT').length - 1).toBe(0); // r35 routes target-month reads through r29
    expect(helper).toContain("from './billing-read-tail-r29-sql'");
    expect(helper.split('billingTargetMonthSql').length - 1).toBe(1);
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
  });

  it('recomposes both target-month reads byte-for-byte with two unchanged binds', () => {
    for (const oracle of ORIGINALS) {
      expect(sha256(oracle.full)).toBe(oracle.sha256);
      expect(billingTargetMonthSql(oracle.columns, oracle.table, oracle.tail)).toBe(oracle.full);
      expect((oracle.full.match(/\$\d/g) ?? []).join(',')).toBe('$1,$2');
    }
  });

  it('routes every call site through the helper and removes the inline literals', async () => {
    const text = await src('billing-target.ts');
    for (const oracle of ORIGINALS) {
      expect(text.split(oracle.full).length - 1).toBe(0);
      expect(text.split(oracle.routed).length - 1).toBe(1);
    }
    expect(text).toContain('./billing-read-tail-r31-sql');
  });

  it('leaves FOR UPDATE, advisory and native owner reads untouched', async () => {
    const helper = await src('billing-read-tail-r31-sql.ts');
    expect(helper).not.toMatch(/FOR\s+UPDATE|FOR\s+SHARE|pg_advisory|nativeBillingOwnerQuery/i);
    const text = await src('billing-target.ts');
    expect(text.split('nativeBillingOwnerQuery').length - 1).toBeGreaterThan(0);
    expect(text.split('pg_advisory').length - 1).toBe(0);
  });

  it('lowers the billing owner SELECT KPI from BEFORE=54 to 42', async () => {
    let spaced = 0, bare = 0;
    for (const file of await tsFiles(srcRoot)) {
      const text = await readFile(file, 'utf8');
      spaced += text.split('SELECT ').length - 1;
      bare += text.split('SELECT').length - 1;
    }
    expect(spaced).toBe(6);
    expect(bare).toBe(6);
    expect(spaced).toBeLessThan(48);
    expect(bare).toBeLessThan(48);
  });
});
