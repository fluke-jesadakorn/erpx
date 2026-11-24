import { describe, it, expect, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
vi.mock('../../../packages/server-quality/src/adapters/native-quality-read', () => ({
  RD_ALERT_HOLD_ID: 'quality_rd_alert_hold_id',
  nativeQualityReadFirst: vi.fn(),
}));
import { qualityAlertHoldId } from '../../../packages/server-quality/src/adapters/purchases';
import { nativeQualityReadFirst, RD_ALERT_HOLD_ID } from '../../../packages/server-quality/src/adapters/native-quality-read';

const src = (path: string) => resolve(process.cwd(), path);
const literal = /SELECT hold_id FROM inventory_quality_alerts WHERE id=\?/g;

describe('quality alert hold id lookup dedupe', () => {
  it('owns no TypeScript SELECT; the statement lives in the native owner registry', () => {
    const purchases = readFileSync(src('packages/server-quality/src/adapters/purchases.ts'), 'utf8');
    const record = readFileSync(src('packages/server-quality/src/adapters/record-purchases.ts'), 'utf8');
    const plans = readFileSync(src('packages/server-quality/src/adapters/native_quality_read_sql.rs'), 'utf8');
    expect(purchases.match(literal) ?? []).toHaveLength(0);
    expect(record.match(literal) ?? []).toHaveLength(0);
    expect(plans.match(literal) ?? []).toHaveLength(1);
  });

  it('is the single canonical lookup shared by both purchase-scope readers', () => {
    const purchases = readFileSync(src('packages/server-quality/src/adapters/purchases.ts'), 'utf8');
    const record = readFileSync(src('packages/server-quality/src/adapters/record-purchases.ts'), 'utf8');
    expect(purchases).toContain('nativeQualityReadFirst<{hold_id:string}>(RD_ALERT_HOLD_ID,[id],database)');
    expect(purchases).toContain('target.kind==="alert"?await qualityAlertHoldId(database,target.id)');
    expect(record).toContain('qualityAlertHoldId(database,id)');
  });

  it('resolves the hold id through the native read plan with one unscoped bind', async () => {
    const db = { native: true } as never;
    vi.mocked(nativeQualityReadFirst).mockResolvedValueOnce({ hold_id: 'hold-1' });
    await expect(qualityAlertHoldId(db, 'alert-1')).resolves.toBe('hold-1');
    expect(nativeQualityReadFirst).toHaveBeenCalledWith(RD_ALERT_HOLD_ID, ['alert-1'], db);
  });
});
