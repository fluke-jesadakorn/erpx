import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { stockObligationHeadSql } from '../../../packages/server-inventory/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const src = (name: string) => readFileSync(resolve(srcRoot, name), 'utf8');
const adapter = (name: string) => src(`adapters/${name}`);
const sha = (text: string) => createHash('sha256').update(text).digest('hex');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;
const MARKER = '${stockObligationHeadSql}';

// Verbatim byte oracles captured from the pre-r59 source. The folded head is
// bind-free and read-only; each caller keeps its own projection tail, source
// scope and independent statement.
const HEAD = "SELECT 'stock',concat(s.warehouse_id,':',s.product_id),";
const HEAD_SHA = '9732d926bca2d380cc3b024aa22a646b297ac931ec0ea7f700ebec64ca87b7b7';
const SITES = [
  {
    file: 'manufacturing-package-obligations.ts',
    BODY_SHA: '9fc31ff7d0dda35a5685df3313087e8af051ebde7395d4b0a77913582a70b485',
    TAIL_SHA: '23b4160c71b4dbc87c1bf8b27bf1af39203fa5e4c71967e9589a2e2a34299d5e',
    binds: 0,
    selects: 4,
  },
  {
    file: 'pms-package-obligations.ts',
    BODY_SHA: '74cf34afa1699e2cd7cb3cb94f3eb31d8d208c5bced37957f7e0c4600c1de88b',
    TAIL_SHA: '98c9a18573cf4d06ef1f77f012cb1ae4c41df2e389eaaa513a8d1c1616723775',
    binds: 0,
    selects: 2,
  },
] as const;

const tailAfterMarker = (text: string, marker: string) => {
  const out: string[] = [];
  let i = 0;
  while (true) {
    const at = text.indexOf(marker, i);
    if (at < 0) break;
    const start = at + marker.length;
    out.push(text.slice(start, text.indexOf('`', start)));
    i = start;
  }
  return out;
};

describe('Inventory read-tail r59 stock-obligation head dedupe', () => {
  test('f1: the folded head is a single byte-identical, bind-free projection prefix', () => {
    expect(stockObligationHeadSql).toBe(HEAD);
    expect(sha(stockObligationHeadSql)).toBe(HEAD_SHA);
    expect(selectCount(stockObligationHeadSql)).toBe(1);
    expect(stockObligationHeadSql).not.toContain('?');
    expect(stockObligationHeadSql).not.toMatch(lockRe);
    expect(adapter('read-tail-sql.ts').split('export const stockObligationHeadSql').length - 1).toBe(1);
  });

  test('f2: both owner-local call sites delegate once and no inline head remains', () => {
    for (const site of SITES) {
      const text = adapter(site.file);
      expect(text.split(MARKER).length - 1, site.file).toBe(1);
      expect(text, site.file).not.toContain(HEAD);
    }
    expect(adapter('manufacturing-package-obligations.ts')).toContain(
      'countScopeRowsSql,markedCountExistsSql,stockObligationHeadSql',
    );
    expect(adapter('pms-package-obligations.ts')).toContain(
      'packageObligationCountHeadSql, stockObligationHeadSql',
    );
  });

  test('f3: rendered SQL, projection tails and bind order stay byte-identical', () => {
    for (const site of SITES) {
      const tails = tailAfterMarker(adapter(site.file), MARKER);
      expect(tails, site.file).toHaveLength(1);
      expect(sha(tails[0]), `tail:${site.file}`).toBe(site.TAIL_SHA);
      expect(sha(HEAD + tails[0]), `body:${site.file}`).toBe(site.BODY_SHA);
      expect(selectCount(HEAD + tails[0]), site.file).toBe(site.selects);
      expect((HEAD + tails[0]).match(/\?/g)?.length ?? 0, site.file).toBe(site.binds);
      expect(HEAD + tails[0], site.file).not.toMatch(lockRe);
    }
    // The two pre-fold projections diverge immediately after the head.
    expect(tailAfterMarker(adapter('manufacturing-package-obligations.ts'), MARKER)[0]).toMatch(
      /^to_jsonb\(s\)::text,true FROM commerce_stock s WHERE \(s\.quantity_milli<0 OR s\.value_minor<0\)/,
    );
    expect(tailAfterMarker(adapter('pms-package-obligations.ts'), MARKER)[0]).toMatch(
      /^s\.version,'invalid',s\.quantity_milli FROM commerce_stock s,scope/,
    );
  });

  test('f4: locks, native transport, count adapters and r45-r58 helpers stay untouched', () => {
    const shared = adapter('read-tail-sql.ts');
    for (const helper of [
      'transitEventQuantitySumSql', 'inList', 'reservationsCteHeadSql', 'markedCountExistsSql',
      'replayProbePrefixSql', 'countSelectHeadSql', 'packageObligationStockHeadSql',
      'lotAllocationRowsHeadSql', 'commandReceiptRecordIdGuardSql', 'countLineProductGuardSql',
      'countLineStockHeadSql',
    ]) expect(shared, helper).toContain(`export const ${helper}`);
    // The enclosing package-obligation statements keep their locks verbatim.
    expect(adapter('manufacturing-package-obligations.ts')).toMatch(
      /LOCK TABLE command_receipts,inventory_count_lot_lines/,
    );
    expect(adapter('pms-package-obligations.ts')).toMatch(
      /LOCK TABLE commerce_stock,inventory_counts,inventory_count_lines/,
    );
    expect(adapter('count-attachment-purchases.ts')).toContain(
      'SELECT id FROM assignments WHERE id=? AND company_id=? FOR SHARE',
    );
    expect(adapter('transfer-attachment-purchases.ts')).toContain(
      'SELECT id FROM assignments WHERE id=? AND company_id=? FOR SHARE',
    );
    // Native mirrors remain verbatim.
    expect(src('adapters/counts.rs')).toContain('SELECT l.*,p.sku,p.tracking_mode,');
    expect(src('adapters/native-inventory-read.ts').length).toBeGreaterThan(0);
    expect(src('adapters/native_command_sql.rs').length).toBeGreaterThan(0);
    expect(src('public/inventory-counts.ts')).toContain('countDetail, countOptions');
  });

  test('retirement:ts-statements KPI is 293 after the r59 stock-obligation head fold (strictly below r58 294)', () => {
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
    // r50 at 305, r51 at 303, r52 at 302, r53 at 300, r54 at 298, r55 at 297,
    // r56 at 296, r57 at 295 and r58 at 294. r59 folds the byte-identical
    // stock-projection head shared by the manufacturing and PMS package
    // obligation reads into one owner-local renderer: 2 source tokens -> 1,
    // i.e. 294 -> 293.
    expect(total).toBe(293);
    expect(total).toBeLessThan(294);
  });
});
