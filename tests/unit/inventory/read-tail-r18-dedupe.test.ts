import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import {
  activeQualityHoldExistsSql,
  pendingLossSumSql,
} from '../../../packages/server-inventory/src/adapters/read-tail-sql';

const srcRoot = resolve(__dirname, '../../../packages/server-inventory/src');
const adapterDir = resolve(srcRoot, 'adapters');
const src = (name: string) => readFileSync(resolve(adapterDir, name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;
const readOnly = (sql: string) => {
  expect(sql).toContain('SELECT ');
  expect(sql).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
};

// inventoryQualityHoldReadSource() returns this fixed Quality-owned relation.
const holdSource = 'FROM (SELECT warehouse_id, product_id, lot_id, status FROM inventory_quality_holds) h';
const holdPredicates = [
  "h.warehouse_id=b.warehouse_id AND h.lot_id=l.id AND h.status NOT IN ('released','cancelled','disposed')",
  "h.warehouse_id=s.warehouse_id AND h.product_id=s.product_id AND h.status NOT IN ('released','cancelled','disposed')",
  "h.lot_id=l.id AND h.warehouse_id=b.warehouse_id AND h.status NOT IN ('released','cancelled','disposed')",
  "h.warehouse_id=b.warehouse_id AND (h.lot_id=l.id OR (h.lot_id IS NULL AND h.product_id=l.product_id)) AND h.status NOT IN ('released','cancelled','disposed')",
];
const holdExpected = holdPredicates.map(p => `EXISTS(SELECT 1 ${holdSource} WHERE ${p})`);
const transitDecision = "NOT EXISTS(SELECT 1 FROM inventory_transit_events r WHERE r.proposal_id=e.id)";
const lossExpected = [
  `COALESCE((SELECT SUM(e.quantity_milli) FROM inventory_transit_events e WHERE e.line_id=l.id AND e.kind='loss_requested' AND ${transitDecision}),0)`,
  `COALESCE((SELECT SUM(le.quantity_milli) FROM inventory_transit_lot_events le JOIN inventory_transit_events e ON e.id=le.event_id WHERE le.transit_lot_id=tl.id AND e.kind='loss_requested' AND ${transitDecision}),0)`,
];

describe('Inventory read-tail r18 parameterized EXISTS/agg-family dedupe', () => {
  test('canonical helpers render the exact byte-identical SQL and stay read-only', () => {
    holdPredicates.forEach((predicate, i) => {
      expect(activeQualityHoldExistsSql(holdSource, predicate)).toBe(holdExpected[i]);
      readOnly(holdExpected[i]);
    });
    expect(pendingLossSumSql(
      'e.quantity_milli',
      'FROM inventory_transit_events e',
      "e.line_id=l.id AND e.kind='loss_requested' AND " + transitDecision,
    )).toBe(lossExpected[0]);
    expect(pendingLossSumSql(
      'le.quantity_milli',
      'FROM inventory_transit_lot_events le JOIN inventory_transit_events e ON e.id=le.event_id',
      "le.transit_lot_id=tl.id AND e.kind='loss_requested' AND " + transitDecision,
    )).toBe(lossExpected[1]);
    for (const sql of lossExpected) readOnly(sql);
  });

  test('each r18 helper literal is defined exactly once in the canonical owner', () => {
    const shared = src('read-tail-sql.ts');
    expect(shared.split('EXISTS(SELECT 1 ${source} WHERE ${predicate})')).toHaveLength(2);
    expect(shared.split('COALESCE((SELECT SUM(${metric}) ${from} WHERE ${where}),0)')).toHaveLength(2);
  });

  test('active quality-hold EXISTS is composed, never inlined, in every named adapter', () => {
    const owners = [
      ['inventory-consumption.ts', 1],
      ['inventory-read-projections.ts', 1],
      ['inventory-transfers.ts', 1],
      ['production-command-sources.ts', 1],
    ] as const;
    const inline = 'EXISTS(SELECT 1 ${inventoryQualityHoldReadSource()} WHERE ';
    for (const [name] of owners) expect(src(name), name).not.toContain(inline);
    for (const [name, calls] of owners) {
      expect(src(name).match(/activeQualityHoldExistsSql\(/g) ?? [], name).toHaveLength(calls);
    }
  });

  test('pending-loss SUM is composed, never inlined, in the transit adapter', () => {
    const text = src('inventory-transit.ts');
    expect(text).not.toContain('COALESCE((SELECT SUM(e.quantity_milli) FROM inventory_transit_events e WHERE ');
    expect(text).not.toContain('COALESCE((SELECT SUM(le.quantity_milli) FROM inventory_transit_lot_events le ');
    expect(text.match(/pendingLossSumSql\(/g) ?? []).toHaveLength(4);
  });

  test('retirement:ts-statements stays strictly below the r18 BEFORE=408 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(408);
  });
});
