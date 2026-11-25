import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { countLineStockHeadSql } from '../../../packages/server-inventory/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const src = (name: string) => readFileSync(resolve(srcRoot, name), 'utf8');
const adapter = (name: string) => src(`adapters/${name}`);
const sha = (text: string) => createHash('sha256').update(text).digest('hex');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;
const MARKER = '${countLineStockHeadSql}';

// Verbatim byte oracles captured from the pre-r58 source. The folded head is
// bind-free and read-only; each caller keeps its own projection tail, table,
// binds and independent statement.
const HEAD = 'SELECT l.*,p.sku,p.tracking_mode,';
const HEAD_SHA = '6e72a82aa95691118354cd6d5420a9f890e43487eb53f66d5d06ea6c171ea749';
const SITES = [
  {
    file: 'inventory-counts.ts',
    // SHA of the full pre-r58 literal body (head + tail, no quotes).
    BODY_SHA: '2797d5dabbe8e209850d1ea6772ce9f01465f756c262a363f6c1673c21ef491d',
    TAIL_SHA: '6d8f8ecabef8704fbcefc5cfe56571231f9973940bb434c7d29538aa30b6b8c5',
    binds: 1,
  },
  {
    file: 'inventory-transfers.ts',
    BODY_SHA: '0fb7f9ac500123b952c410c51738684b4abcde310cc16be1c52a97bf69600631',
    TAIL_SHA: '41c653c5a03f2775fe8ef597127fb020a6c2dd2f1db9ba4932de207741705cda',
    binds: 1,
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

describe('Inventory read-tail r58 count-line stock head dedupe', () => {
  test('f1: the folded head is a single byte-identical, bind-free projection prefix', () => {
    expect(countLineStockHeadSql).toBe(HEAD);
    expect(sha(countLineStockHeadSql)).toBe(HEAD_SHA);
    expect(selectCount(countLineStockHeadSql)).toBe(1);
    expect(countLineStockHeadSql).not.toContain('?');
    expect(countLineStockHeadSql).not.toMatch(lockRe);
    expect(adapter('read-tail-sql.ts').split('export const countLineStockHeadSql').length - 1).toBe(1);
  });

  test('f2: both owner-local call sites delegate once and no inline head remains', () => {
    for (const site of SITES) {
      const text = adapter(site.file);
      expect(text.split(MARKER).length - 1, site.file).toBe(1);
      expect(text, site.file).not.toContain(HEAD);
    }
    expect(adapter('inventory-counts.ts')).toContain('countSelectHeadSql,decisionActorRowsSql,countLineStockHeadSql');
    expect(adapter('inventory-transfers.ts')).toContain('lotBalanceRowsHeadSql,countLineStockHeadSql');
  });

  test('f3: rendered SQL, projection tails and bind order stay byte-identical', () => {
    for (const site of SITES) {
      const tails = tailAfterMarker(adapter(site.file), MARKER);
      expect(tails, site.file).toHaveLength(1);
      expect(sha(tails[0]), `tail:${site.file}`).toBe(site.TAIL_SHA);
      expect(sha(HEAD + tails[0]), `body:${site.file}`).toBe(site.BODY_SHA);
      expect(selectCount(HEAD + tails[0]), site.file).toBe(1);
      expect((HEAD + tails[0]).match(/\?/g)?.length ?? 0, site.file).toBe(site.binds);
      expect(HEAD + tails[0], site.file).not.toMatch(lockRe);
    }
    expect(adapter('inventory-counts.ts')).toContain('WHERE l.count_id=? ORDER BY l.ordinal`).bind(warehouse_id,id).all<CountRow>()');
    expect(adapter('inventory-transfers.ts')).toContain('WHERE l.transfer_id=? ORDER BY l.ordinal`).bind(transfer.source_warehouse_id,id).all<TransferRow>()');
  });

  test('f4: locks, native transport, count adapters and r45-r57 helpers stay untouched', () => {
    const shared = adapter('read-tail-sql.ts');
    for (const helper of [
      'transitEventQuantitySumSql', 'inList', 'reservationsCteHeadSql', 'markedCountExistsSql',
      'replayProbePrefixSql', 'countSelectHeadSql', 'packageObligationStockHeadSql',
      'lotAllocationRowsHeadSql', 'commandReceiptRecordIdGuardSql', 'countLineProductGuardSql',
    ]) expect(shared, helper).toContain(`export const ${helper}`);
    expect(adapter('count-attachment-purchases.ts')).toContain('SELECT id FROM assignments WHERE id=? AND company_id=? FOR SHARE');
    expect(adapter('transfer-attachment-purchases.ts')).toContain('SELECT id FROM assignments WHERE id=? AND company_id=? FOR SHARE');
    expect(adapter('sales-package-obligations.ts')).toContain('LOCK TABLE');
    expect(adapter('supply-package-obligations.ts')).toContain('LOCK TABLE');
    // The native Rust mirror of the same projection remains verbatim.
    expect(src('adapters/counts.rs')).toContain(HEAD);
    expect(src('adapters/native-inventory-read.ts').length).toBeGreaterThan(0);
    expect(src('adapters/native_command_sql.rs').length).toBeGreaterThan(0);
    expect(src('public/inventory-counts.ts')).toContain('countDetail, countOptions');
  });

  test('retirement:ts-statements KPI is 294 after the r58 count-line stock head fold (strictly below r57 295)', () => {
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
    // r56 at 296 and r57 at 295. r58 folds the byte-identical count-line stock
    // projection head shared by the count-line and transfer-line detail reads
    // into one owner-local renderer: 2 source tokens -> 1, i.e. 295 -> 294.
    expect(total).toBe(284);
    expect(total).toBeLessThan(294);
  });
});
