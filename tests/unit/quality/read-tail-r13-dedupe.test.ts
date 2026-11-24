import { describe, it, expect, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { qualitySourceIdExistsSql } from '../../../packages/server-quality/src/adapters/quality-record-source-sql';

const source = (path: string) => resolve(process.cwd(), path);
const pre = [
  'SELECT 1 FROM inventory_quality_holds actual_hold WHERE actual_hold.id=${id}',
  'SELECT 1 FROM inventory_quality_alerts actual_alert WHERE actual_alert.id=${id}',
  'SELECT 1 FROM inventory_quality_specs actual_spec WHERE actual_spec.id=${id}',
  'SELECT 1 FROM inventory_quality_holds WHERE id=?',
  'SELECT 1 FROM inventory_quality_specs WHERE id=?',
] as const;

describe('quality record source id-exists family dedupe (r13)', () => {
  it('one parameterized helper reproduces all five probes byte-for-byte', () => {
    expect(qualitySourceIdExistsSql('inventory_quality_holds', 'actual_hold', '${id}')).toBe(pre[0]);
    expect(qualitySourceIdExistsSql('inventory_quality_alerts', 'actual_alert', '${id}')).toBe(pre[1]);
    expect(qualitySourceIdExistsSql('inventory_quality_specs', 'actual_spec', '${id}')).toBe(pre[2]);
    expect(qualitySourceIdExistsSql('inventory_quality_holds', '', '?')).toBe(pre[3]);
    expect(qualitySourceIdExistsSql('inventory_quality_specs', '', '?')).toBe(pre[4]);
  });

  it('the five callers share the helper with no inline copies', () => {
    const adapter = readFileSync(source('packages/server-quality/src/adapters/record-purchases.ts'), 'utf8');
    expect(adapter.match(/qualitySourceIdExistsSql\(/g)).toHaveLength(5);
    for (const literal of pre) expect(adapter).not.toContain(literal);
    const helper = readFileSync(source('packages/server-quality/src/adapters/quality-record-source-sql.ts'), 'utf8');
    expect(helper.match(/SELECT /g)).toHaveLength(1);
  });

  it('the helper is read-only and locks, r10/r11/r12 helpers stay independent', () => {
    const helper = readFileSync(source('packages/server-quality/src/adapters/quality-record-source-sql.ts'), 'utf8');
    expect(helper).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
    const adapter = readFileSync(source('packages/server-quality/src/adapters/record-purchases.ts'), 'utf8');
    expect(adapter).toContain('qualityClockCte("quality_source_clock")');
    expect(adapter).toContain('qualityAlertHoldId');
    for (const name of ['quality-child-agg-sql', 'quality-receipt-guard-sql', 'quality-source-rows-sql']) {
      const h = readFileSync(source(`packages/server-quality/src/adapters/${name}.ts`), 'utf8');
      expect(h.match(/SELECT /g)).toHaveLength(1);
    }
  });
});
