import { describe, expect, test } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { packageObligationStockHeadSql } from '../../../packages/server-inventory/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const src = (name: string) => readFileSync(resolve(srcRoot, name), 'utf8');
const adapter = (name: string) => src(`adapters/${name}`);
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const lockRe = /INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i;
const sha = (text: string) => createHash('sha256').update(text).digest('hex');
const tailsAfter = (text: string, marker: string) => {
  const out: string[] = [];
  let i = 0;
  while (true) {
    const at = text.indexOf(marker, i);
    if (at < 0) break;
    const start = at + marker.length;
    // Stop at the next UNION ALL, not the statement's closing backtick: two
    // heads share one statement, so the rest of it contains the sibling fold.
    const close = text.indexOf('`', start);
    const next = text.indexOf(' UNION ALL ', start);
    const end = next >= 0 && next < close ? next : close;
    out.push(text.slice(start, end));
    i = start;
  }
  return out;
};

// Verbatim byte oracles captured from the pre-r54 source. They are identity
// checks only; the canonical renderer owns the text and every caller keeps its
// own statement, rows, binds and guard/ORDER tails.
const BODY = "concat(stock.warehouse_id,':',stock.product_id),to_jsonb(stock),stock.quantity_milli<0 OR stock.value_minor<0";
const HEAD = (kind: string) => `SELECT '${kind}',${BODY}`;
const KINDS = ['initial-physical-stock', 'stock-invalid', 'invalid-stock'] as const;
// SHA of each full head as it appeared before the fold.
const HEAD_SHAS: Record<string, string> = {
  'initial-physical-stock': 'db4d85920298f05eb4eb07c3664789a98f597c860b5f65ba02194ac3abe8b27f',
  'stock-invalid': 'c7b9afd0b5ccc9d80b4e0d8f278bd9fef2e2c2d845ee977585fa6dffc0d877ff',
  'invalid-stock': '32ff32f725bed6191841b81724e873fbfa4f270958997d20d84612273279ad8f',
};
// SHA of each LOCAL guard/FROM/ORDER tail from the pre-r54 parent blob, read up
// to the next UNION ALL. The initial-physical-stock statement also holds the
// stock-invalid head, so pinning the local tail keeps the oracle on the guard
// text rather than on the sibling fold. Verified byte-identical to the parent.
const TAIL_SHAS: Record<string, string> = {
  'initial-physical-stock': 'bb248329070742fe8118f15e6577e4c7317ef4e427fd8ebe513c386b36bd7c56',
  'stock-invalid': 'a9a0c259ae7939602aa0561d4e0c355ccdebce6efa395ab520c0e1ce54b47673',
  'invalid-stock': 'c37d8f022c7292cffa3c9b10c19af7fd44b0622ce5f7c960617acdc508a90e87',
};

describe('Inventory read-tail r54 stock-head dedupe', () => {
  test('f1: the parameterized renderer reproduces all three heads byte-identically', () => {
    for (const kind of KINDS) {
      const rendered = packageObligationStockHeadSql(kind);
      expect(rendered, kind).toBe(HEAD(kind));
      expect(sha(rendered), kind).toBe(HEAD_SHAS[kind]);
      expect(selectCount(rendered), kind).toBe(1);
      expect(rendered, kind).not.toContain('?');
      expect(rendered, kind).not.toMatch(lockRe);
    }
  });

  test('f2: one owner-local helper owns the head and the three former sites delegate', () => {
    const shared = adapter('read-tail-sql.ts');
    expect(shared.split('export const packageObligationStockHeadSql').length - 1).toBe(1);
    const sales = adapter('sales-package-obligations.ts');
    const supply = adapter('supply-package-obligations.ts');
    expect(sales.split("packageObligationStockHeadSql('initial-physical-stock')").length - 1).toBe(1);
    expect(sales.split("packageObligationStockHeadSql('stock-invalid')").length - 1).toBe(1);
    expect(supply.split("packageObligationStockHeadSql('invalid-stock')").length - 1).toBe(1);
    for (const kind of KINDS) {
      expect(sales + supply, kind).not.toContain(`SELECT '${kind}',${BODY}`);
    }
    expect(sales).toContain('disposalCountsSql,packageObligationStockHeadSql,postedInitialPhysicalStockSourceSql');
    expect(supply).toContain('childJoinJsonAggSql,packageObligationStockHeadSql,scopeExistsSql');
  });

  test('f3: guard/ORDER tails, binds and statements stay unchanged in their callers', () => {
    const cases: [string, string][] = [
      ['sales-package-obligations.ts', 'initial-physical-stock'],
      ['sales-package-obligations.ts', 'stock-invalid'],
      ['supply-package-obligations.ts', 'invalid-stock'],
    ];
    for (const [file, kind] of cases) {
      const tails = tailsAfter(adapter(file), `\${packageObligationStockHeadSql('${kind}')}`);
      expect(tails, kind).toHaveLength(1);
      expect(sha(tails[0]), `tail:${kind}`).toBe(TAIL_SHAS[kind]);
    }
    const sales = adapter('sales-package-obligations.ts');
    const supply = adapter('supply-package-obligations.ts');
    expect(sales).toContain("FROM (${sales.stock}) stock CROSS JOIN scope WHERE ${selection==='sales'?'TRUE':'FALSE'} AND ${postedInitialPhysicalStockSourceSql(initialPhysicalStockCountSourceRows,'posted_count')}");
    expect(sales).toContain('WHERE EXISTS(SELECT 1 FROM orders o JOIN (${sales.lines}) line ON line.order_id=o.id WHERE o.warehouse_id=stock.warehouse_id AND line.product_id=stock.product_id) ORDER BY kind,id`).bind(companyId)');
    expect(supply).toContain('FROM commerce_stock stock JOIN commerce_warehouses warehouse ON warehouse.id=stock.warehouse_id CROSS JOIN scope WHERE warehouse.company_id=scope.company_id AND scope.full');
    expect(supply).toContain('ORDER BY kind,id`).bind(input.companyId,input.packageId==="supply",input.packageId==="supply-quality",input.packageId==="supply-advanced")');
    expect(adapter('read-tail-sql.ts')).not.toMatch(lockRe);
  });

  test('f4: locks, native kinds and r45-r53 helpers stay untouched', () => {
    const shared = adapter('read-tail-sql.ts');
    for (const helper of [
      'transitEventQuantitySumSql', 'schemaReadySql', 'inList', 'reservationsCteHeadSql',
      'markedCountExistsSql', 'replayProbePrefixSql', 'countSelectHeadSql', 'initialPhysicalCountJournalSql',
      'evidencePositionRowsSql', 'executivePositionPointSql', 'countEvidenceRowHeadSql',
      'sourceImagesHeadSql', 'lotBalanceRowsHeadSql', 'countAuthorityScopeSql',
    ]) expect(shared, helper).toContain(`export const ${helper}`);
    // The FOR SHARE lock literals are intentionally not folded.
    expect(adapter('count-attachment-purchases.ts')).toContain('SELECT id FROM assignments WHERE id=? AND company_id=? FOR SHARE');
    expect(adapter('transfer-attachment-purchases.ts')).toContain('SELECT id FROM assignments WHERE id=? AND company_id=? FOR SHARE');
    expect(src('adapters/native_command_sql.rs').length).toBeGreaterThan(0);
    expect(src('adapters/native-inventory-read.ts').length).toBeGreaterThan(0);
    expect(adapter('sales-package-obligations.ts')).toContain('LOCK TABLE');
    expect(adapter('supply-package-obligations.ts')).toContain('LOCK TABLE');
  });

  test('retirement:ts-statements KPI is 298 after the r54 stock-head fold (strictly below r53 300)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    // r45 ended at 320, r46 at 316, r47 at 314, r49 native cutover at 307,
    // r50 at 305, r51 at 303, r52 at 302 and r53 folded the count-list head to
    // 300. r54 folds the three byte-identical package-obligation stock heads
    // (initial-physical-stock, stock-invalid, invalid-stock) into one
    // kind-parameterized owner-local renderer: 3 source tokens -> 1, i.e. 300 -> 298.
    expect(total).toBe(298);
    expect(total).toBeLessThan(300);
  });
});
