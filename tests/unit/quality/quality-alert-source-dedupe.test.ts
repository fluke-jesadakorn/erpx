import { describe, it, expect, vi } from 'vitest';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
vi.mock('server-only', () => ({}));
import { qualityAlertHoldId, qualityAlertHoldIdSql } from '../../../packages/server-quality/src/adapters/purchases';

const src = (path: string) => resolve(process.cwd(), path);
const literal = /SELECT hold_id FROM inventory_quality_alerts WHERE id=\?/g;

describe('quality alert hold id lookup dedupe', () => {
  it('owns one fixed SELECT over the alert source', () => {
    expect(qualityAlertHoldIdSql.match(/SELECT /g)).toHaveLength(1);
    expect(qualityAlertHoldIdSql).toBe('SELECT hold_id FROM inventory_quality_alerts WHERE id=?');
  });

  it('is the single literal shared by both purchase-scope readers', () => {
    const purchases = readFileSync(src('packages/server-quality/src/adapters/purchases.ts'), 'utf8');
    const record = readFileSync(src('packages/server-quality/src/adapters/record-purchases.ts'), 'utf8');
    expect(purchases.match(literal) ?? []).toHaveLength(1);
    expect(record.match(literal) ?? []).toHaveLength(0);
    expect(record).toContain('qualityAlertHoldId');
  });

  it('resolves the hold id through the canonical statement', async () => {
    const first = vi.fn().mockResolvedValue({ hold_id: 'hold-1' });
    const bind = vi.fn().mockReturnValue({ first });
    const prepare = vi.fn().mockReturnValue({ bind });
    await expect(qualityAlertHoldId({ prepare } as never, 'alert-1')).resolves.toBe('hold-1');
    expect(prepare).toHaveBeenCalledWith(qualityAlertHoldIdSql);
    expect(bind).toHaveBeenCalledWith('alert-1');
  });
});
