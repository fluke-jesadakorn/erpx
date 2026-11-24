import { describe, expect, test } from 'vitest';
import { readFileSync } from 'node:fs';
import {
  reservedWarehouseProductQtySql,
  singleCountLineMarkerSql,
} from '../../../packages/server-inventory/src/adapters/read-tail-sql';

const src = (name: string) => readFileSync(new URL(`../../../packages/server-inventory/src/adapters/${name}`, import.meta.url), 'utf8');

describe('Inventory read-tail r11 byte-identical dedupe', () => {
  test('canonical literals own the exact shared text, read-only', () => {
    expect(reservedWarehouseProductQtySql).toBe('SELECT warehouse_id,product_id,sum(quantity_milli::numeric) quantity_milli');
    expect(singleCountLineMarkerSql).toBe('(SELECT count(*) FROM inventory_count_lines actual WHERE actual.count_id=c.id)=1');
    for (const sql of [reservedWarehouseProductQtySql, singleCountLineMarkerSql]) {
      expect(sql).toContain('SELECT ');
      expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    }
  });

  test('warehouse-product sum has one owner literal and two composed callers after r47', () => {
    const main = src('executive-main-evidence-sources.ts');
    const metric = src('executive-metric-projections.ts');
    for (const [name, text] of [['executive-main-evidence-sources.ts', main], ['executive-metric-projections.ts', metric]] as const) {
      expect(text, name).not.toContain(reservedWarehouseProductQtySql);
      // r47 folds the three inline heads into reservationsCteHeadSql; the two
      // executive adapters no longer reference the r11 sum helper directly.
      expect(text.match(/reservedWarehouseProductQtySql/g) ?? [], name).toHaveLength(0);
    }
    const shared = src('read-tail-sql.ts');
    expect(shared.split(reservedWarehouseProductQtySql)).toHaveLength(2);
    expect(shared.match(/reservedWarehouseProductQtySql/g) ?? []).toHaveLength(3); // definition + r36 head + r47 head
  });

  test('single-count-line marker has one owner literal and four callers', () => {
    const owners = [
      'initial-physical-stock-count-source-sql.ts',
      'manufacturing-initial-physical-stock-count-source-sql.ts',
      'pos-initial-physical-stock-count-source-sql.ts',
      'service-initial-physical-stock-count-source-sql.ts',
    ];
    for (const name of owners) {
      const text = src(name);
      expect(text, name).not.toContain(singleCountLineMarkerSql);
      expect(text.match(/singleCountLineMarkerSql/g) ?? [], name).toHaveLength(2);
    }
    expect(src('read-tail-sql.ts').split(singleCountLineMarkerSql)).toHaveLength(2);
  });

  test('posted-count journal tail is one local literal composed three times', () => {
    const text = src('finance-record-sources.ts');
    expect(text.match(/initialPhysicalCountJournalTail/g) ?? []).toHaveLength(4);
    const tail = ' WHERE c.net_value_minor>0 AND c.net_value_minor=(SELECT sum(line.counted_value_minor-line.snapshot_value_minor) FROM inventory_count_lines line WHERE line.count_id=c.id)';
    expect(text.split(tail)).toHaveLength(2);
  });

  test('manufacturing posting-state read uses the native kind, no local SQL', () => {
    const text = src('manufacturing-posting-setup-sources.ts');
    expect(text).toContain('INVENTORY_RD_POS_STARTUP_INVENTORY_STATE');
    expect(text).not.toContain('SELECT EXISTS(SELECT 1 FROM inventory_counts');
  });
});
