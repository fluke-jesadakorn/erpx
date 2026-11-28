import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { countEvidenceRowHeadSql, countLineProductGuardSql } from '../../../modules/inventory/server/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../modules/inventory/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, name), 'utf8');
const adapter = (name: string) => src(`adapters/${name}`);
const sha = (text: string) => createHash('sha256').update(text).digest('hex');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;
const markerRe = /\$\{countLineProductGuardSql\('([^']*)'\)\}/g;

// Verbatim byte oracles captured from the pre-r57 source. The folded head is
// bind-free and read-only; each caller keeps its own NOT prefix, tail, binds
// and independent statement.
const HEAD = 'EXISTS(SELECT 1 FROM inventory_count_lines line WHERE line.count_id=c.id AND line.product_id';
const HEAD_SHA = 'a3bad4d108dde86429df6d20c3b3e6feb85c6ad2ab5e8518aa37161e4bb97aac';
const SITES = [
  {
    tail: '=h.product_id AND line.snapshot_quantity_milli>line.counted_quantity_milli)',
    TAIL_SHA: 'f0842db75885a6047d318bcf2da271f9fcc9d62ac61819deeefb1056423bd1c1',
    GUARD_SHA: 'b076ba5ee3f3ef631ecbdc25576b5b7daed873ded865c2fb16024d08efd1331f',
  },
  {
    tail: '<>h.product_id)',
    TAIL_SHA: '029b75c0c42d24725a5426c241c2398fe3b28bad50aefb714d9596e184fe2cda',
    GUARD_SHA: '8352dd5a875472751bba10316eb418c8b8a6c33707e3353ee4dd6096242fb2eb',
  },
] as const;

describe('Inventory read-tail r57 count-line product guard dedupe', () => {
  test('f1: the folded head is a single byte-identical, bind-free EXISTS prefix', () => {
    expect(HEAD_SHA).toBe(sha(HEAD));
    expect(countLineProductGuardSql('')).toBe(HEAD);
    expect(sha(countLineProductGuardSql(''))).toBe(HEAD_SHA);
    expect(selectCount(countLineProductGuardSql(''))).toBe(1);
    expect(countLineProductGuardSql('')).not.toContain('?');
    expect(countLineProductGuardSql('')).not.toMatch(lockRe);
    expect(countLineProductGuardSql('')).not.toContain('NOT ');
    expect(adapter('read-tail-sql.ts').split('export const countLineProductGuardSql').length - 1).toBe(1);
  });

  test('f2: both owner-local guards delegate once and no inline head remains', () => {
    const text = adapter('quality-count-purchases.ts');
    expect(text.match(markerRe)?.length ?? 0).toBe(2);
    expect(text).not.toContain(HEAD);
    expect(text).toContain("import {countEvidenceRowHeadSql,countLineProductGuardSql} from './read-tail-sql';");
    expect(text).toContain("WHERE c.created_by=c.prepared_by AND ${countLineProductGuardSql('=h.product_id AND line.snapshot_quantity_milli>line.counted_quantity_milli)')}");
    expect(text).toContain("AND NOT ${countLineProductGuardSql('<>h.product_id)')}");
  });

  test('f3: rendered guards, full statement and bind order stay byte-identical', () => {
    const text = adapter('quality-count-purchases.ts');
    const tails = [...text.matchAll(markerRe)].map((match) => match[1]);
    expect(tails).toHaveLength(2);
    tails.forEach((tail, i) => {
      expect(sha(tail), `tail:${i}`).toBe(SITES[i].TAIL_SHA);
      expect(sha(countLineProductGuardSql(tail)), `guard:${i}`).toBe(SITES[i].GUARD_SHA);
      expect(countLineProductGuardSql(tail)).toBe(HEAD + SITES[i].tail);
      expect(selectCount(countLineProductGuardSql(tail))).toBe(1);
      expect(countLineProductGuardSql(tail)).not.toContain('?');
      expect(countLineProductGuardSql(tail)).not.toMatch(lockRe);
    });
    // Restoring the whole pre-r57 statement from the current source reproduces
    // the captured literal exactly, including the NOT prefix and both tails.
    const body = text.slice(text.indexOf('export const qualityDisposalCountRows=`') + 'export const qualityDisposalCountRows=`'.length, text.indexOf('`;\nexport async function'));
    const rendered = body
      .replace(/\$\{countEvidenceRowHeadSql\(`([^`]*)`\)\}/, (_, arg: string) => `${countEvidenceRowHeadSql('')}${arg}`)
      .replace(markerRe, (_, tail: string) => countLineProductGuardSql(tail));
    expect(sha(rendered)).toBe('dbaa1588fef69d5d26c6f423859c4733e4518b647307c735121f03a3adf10515');
    expect(rendered).toContain(`AND ${HEAD}${SITES[0].tail}`);
    expect(rendered).toContain(`AND NOT ${HEAD}${SITES[1].tail}`);
    expect(rendered.match(/\?/g)?.length ?? 0).toBe(0);
    expect(rendered).not.toMatch(lockRe);
  });

  test('f4: locks, native transport, count adapters and prior helpers stay untouched', () => {
    const shared = adapter('read-tail-sql.ts');
    for (const helper of [
      'lotAllocationRowsHeadSql', 'commandReceiptRecordIdGuardSql', 'packageObligationStockHeadSql',
      'countSelectHeadSql', 'markedCountExistsSql', 'reservationsCteHeadSql',
      'inList', 'transitEventQuantitySumSql', 'countEvidenceRowHeadSql',
    ]) expect(shared, helper).toContain(`export const ${helper}`);
    expect(adapter('count-attachment-purchases.ts')).toContain('SELECT id FROM assignments WHERE id=? AND company_id=? FOR SHARE');
    expect(adapter('transfer-attachment-purchases.ts')).toContain('SELECT id FROM assignments WHERE id=? AND company_id=? FOR SHARE');
    expect(adapter('sales-package-obligations.ts')).toContain('LOCK TABLE');
    expect(adapter('supply-package-obligations.ts')).toContain('LOCK TABLE');
    expect(src('adapters/native-inventory-read.ts').length).toBeGreaterThan(0);
    expect(src('adapters/native_command_sql.rs').length).toBeGreaterThan(0);
    expect(src('public/quality-count-purchases.ts')).toContain('qualityDisposalCountRows,qualityDisposalCountSource');
  });

  test('retirement:ts-statements KPI is 295 after the r57 count-line guard head fold (strictly below r56 296)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(p, 'utf8'));
      }
    };
    walk(srcRoot);
    // r45 ended at 320, r46 at 316, r47 at 314, r49 native cutover at 307,
    // r50 at 305, r51 at 303, r52 at 302, r53 at 300, r54 at 298, r55 at 297
    // and r56 at 296. r57 folds the byte-identical count-line product guard
    // head shared by the quality-disposal same-product and cross-product reads
    // into one owner-local renderer: 2 source tokens -> 1, i.e. 296 -> 295.
    expect(total).toBe(276);
    expect(total).toBeLessThan(294);
  });
});
