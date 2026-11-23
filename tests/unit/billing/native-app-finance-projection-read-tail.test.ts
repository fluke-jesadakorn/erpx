import { describe, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));

// The retired owner-local finance projection reader builds no SQL of its own.
// Mock the native owner port and assert the exact typed kind, positional
// values and holding scope it sends; the caller's pool must never receive a
// TypeScript-authored SELECT.
const ownerQuery = vi.hoisted(() => vi.fn(async (_client: unknown, _kind: string, _values: unknown[]) => ({ rows: [] } as never)));
const ownerBind = vi.hoisted(() => vi.fn(async (_db: unknown, _kind: string, _values: unknown[]) => ({
  first: vi.fn(async () => ({ status: 'active' })),
})));
vi.mock('../../../packages/server-billing/src/adapters/native-billing-owner-write', () => ({
  nativeBillingOwnerQuery: ownerQuery,
  nativeBillingOwnerBind: ownerBind,
}));
const { appBillingFinanceProjection } = await import('@erpx/server-billing/adapters/app-finance-projection-source');
const { betaManagementFacts } = await import('@erpx/server-billing/adapters/beta-management-facts');

describe('Billing owner-local finance projection read tail', () => {
  it('reads the test finance projection by typed kind with invoice and holding binds only and no local SQL', async () => {
    ownerQuery.mockReset();
    const projection = {
      invoice_satang: '100', receivable_satang: '100', received_satang: '0',
      reversed_satang: '0', credited_satang: '0', stripe_status: 'open', synced_at: '1',
    };
    ownerQuery.mockResolvedValueOnce({ rows: [projection] } as never);
    const pool = { query: vi.fn(async () => ({ rows: [] })) };
    const db = { pool } as never;

    await expect(appBillingFinanceProjection(db, 'inv-1', 'h001')).resolves.toEqual(projection);

    expect(ownerQuery).toHaveBeenCalledWith(pool, 'app_finance_projection', ['inv-1', 'h001']);
    expect(pool.query).not.toHaveBeenCalled();
  });

  it('returns undefined when the native owner projection has no row', async () => {
    ownerQuery.mockReset();
    ownerQuery.mockResolvedValueOnce({ rows: [] } as never);
    const pool = { query: vi.fn(async () => ({ rows: [] })) };

    await expect(appBillingFinanceProjection({ pool } as never, 'inv-missing', 'h001')).resolves.toBeUndefined();
    expect(ownerQuery).toHaveBeenCalledWith(pool, 'app_finance_projection', ['inv-missing', 'h001']);
    expect(pool.query).not.toHaveBeenCalled();
  });
});

describe('Billing beta management facts read tail', () => {
  it('reads beta billing status by typed kind with only the holding bind and no local SQL', async () => {
    ownerBind.mockReset();
    const prepare = vi.fn();
    const db = { prepare } as never;

    await expect(betaManagementFacts(db).billing('h001')).resolves.toEqual({ status: 'active' });

    expect(ownerBind).toHaveBeenCalledWith(db, 'beta_management_billing_status', ['h001']);
    expect(prepare).not.toHaveBeenCalled();
  });
});
