import { describe, it, expect, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { qualitySourceArmHead } from '../../../packages/server-quality/src/adapters/quality-source-arm-head';
import { RD_HOLD_EXISTS, RD_SPEC_EXISTS } from '../../../packages/server-quality/src/adapters/native-quality-read';

const source = (path: string) => resolve(process.cwd(), path);
const read = (path: string) => readFileSync(source(path), 'utf8');

// Byte oracles captured from the deleted shared helper's five call sites.
const pre = [
  'SELECT 1 FROM inventory_quality_holds actual_hold WHERE actual_hold.id=${id}',
  'SELECT 1 FROM inventory_quality_alerts actual_alert WHERE actual_alert.id=${id}',
  'SELECT 1 FROM inventory_quality_specs actual_spec WHERE actual_spec.id=${id}',
  'SELECT 1 FROM inventory_quality_holds WHERE id=?',
  'SELECT 1 FROM inventory_quality_specs WHERE id=?',
] as const;

const id = '${id}';

describe('quality record source id-exists family retirement (r30)', () => {
  it('the three aliased probes are composed byte-for-byte by the shared arm head', () => {
    expect(qualitySourceArmHead('1', 'inventory_quality_holds actual_hold', ` WHERE actual_hold.id=${id}`)).toBe(pre[0]);
    expect(qualitySourceArmHead('1', 'inventory_quality_alerts actual_alert', ` WHERE actual_alert.id=${id}`)).toBe(pre[1]);
    expect(qualitySourceArmHead('1', 'inventory_quality_specs actual_spec', ` WHERE actual_spec.id=${id}`)).toBe(pre[2]);
    for (const sql of pre.slice(0, 3)) {
      expect(sql.startsWith('SELECT 1 FROM ')).toBe(true);
      expect(sql).not.toContain('?');
    }
  });

  it('the two unaliased probes are closed one-bind native kinds', () => {
    expect(RD_HOLD_EXISTS).toBe('quality_rd_hold_exists');
    expect(RD_SPEC_EXISTS).toBe('quality_rd_spec_exists');
    const plans = read('packages/server-quality/src/adapters/native_quality_read_sql.rs');
    expect(plans).toContain(`"${pre[3]}"`);
    expect(plans).toContain(`"${pre[4]}"`);
    expect(plans).toContain('pub const RD_HOLD_EXISTS: &str = "quality_rd_hold_exists";');
    expect(plans).toContain('pub const RD_SPEC_EXISTS: &str = "quality_rd_spec_exists";');
  });

  it('the adapter drops the deleted helper but keeps three arm heads and two native reads', () => {
    const adapter = read('packages/server-quality/src/adapters/record-purchases.ts');
    expect(adapter).not.toContain('quality-record-source-sql');
    expect(adapter).not.toContain('qualitySourceIdExistsSql');
    expect(adapter.match(/qualitySourceArmHead\(/g)).toHaveLength(6);
    expect(adapter.match(/nativeQualityReadFirst\(/g)).toHaveLength(2);
    expect(adapter).toContain('qualitySourceArmHead(\'1\',\'inventory_quality_holds actual_hold\',` WHERE actual_hold.id=${id}`)');
    expect(adapter).toContain('nativeQualityReadFirst(RD_HOLD_EXISTS,[id],database)');
    expect(adapter).toContain('nativeQualityReadFirst(RD_SPEC_EXISTS,[id],database)');
    for (const literal of pre) expect(adapter).not.toContain(literal);
  });

  it('prior read families, locks and the read-only head stay independent', () => {
    const head = read('packages/server-quality/src/adapters/quality-source-arm-head.ts');
    expect(head).toContain('evidenceHead(columns, ` FROM ${source}`, predicate)');
    expect(head.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(head).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    const adapter = read('packages/server-quality/src/adapters/record-purchases.ts');
    expect(adapter).toContain('qualityClockCte("quality_source_clock")');
    expect(adapter).toContain('qualityAlertHoldId');
    expect(read('packages/server-quality/src/adapters/purchase-sources.ts')).toContain('qualityPurchaseSourceReadLocks');
    for (const name of ['quality-child-agg-sql', 'quality-receipt-guard-sql']) {
      expect(read(`packages/server-quality/src/adapters/${name}.ts`).match(/SELECT /g)).toHaveLength(1);
      expect(read('packages/server-quality/src/adapters/quality-source-rows-sql.ts').match(/SELECT /g) ?? []).toHaveLength(0); // r37 fold
    }
  });
});
