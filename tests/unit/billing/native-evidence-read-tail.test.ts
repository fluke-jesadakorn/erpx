import { describe, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));

// The retired owner-local evidence reader builds no SQL of its own. Mock the
// native owner port and assert the exact typed kind and positional values it
// sends; the caller's pool must never receive a TypeScript-authored SELECT.
const ownerQuery = vi.hoisted(() => vi.fn(async (_client: unknown, _kind: string, _values: unknown[]) => ({ rows: [] } as never)));
const ownerBind = vi.hoisted(() => vi.fn(async (_db: unknown, _kind: string, _values: readonly unknown[]) => ({ bind: vi.fn(), first: vi.fn(async () => ({})) })));
vi.mock('../../../packages/server-billing/src/adapters/native-billing-owner-write', () => ({
  nativeBillingOwnerQuery: ownerQuery,
  nativeBillingOwnerBind: ownerBind,
}));
vi.mock('../../../packages/server-commercial/src/adapters/native-owner-read', () => ({
  nativeCommercialOwnerRead: vi.fn(async () => 'SELECT 1 FROM billing_usage_snapshots'),
  COMMERCIAL_RD_USAGE_METER: 'usage.meter',
  COMMERCIAL_RD_USAGE_SNAPSHOT: 'usage.snapshot',
}));
const { drainBillingEvidenceScans } = await import('@erpx/server-billing/adapters/billing-attachments');
const { billingApiRepository } = await import('@erpx/server-billing/adapters/billing-api');
const { billingLink, applyLegacyStripeEvent } = await import('@erpx/server-billing/adapters/beta-billing');
const { nativeSeatForecastRepository } = await import('@erpx/server-billing/adapters/billing-forecast');
const { nativeTargetRolloutRepository } = await import('@erpx/server-billing/adapters/billing-target-rollout');
const { nativeTargetBillingRepository } = await import('@erpx/server-billing/adapters/billing-target');

function env() {
  const query = vi.fn(async () => ({ rows: [] }));
  return {
    env: {
      CONTROL_DB: { pool: { query } },
      ERPX_DATA_DIR: '/tmp/erpx-billing-evidence-read-tail',
      ERPX_ENVIRONMENT: 'test',
    },
    query,
  };
}

describe('Billing owner-local evidence read tail', () => {
  it('reads a scan source by typed kind with only positional values and no local SQL', async () => {
    ownerQuery.mockReset();
    ownerQuery.mockResolvedValueOnce({ rows: [{ id: 'job', evidence_id: 'ev-1', attempt: 1 }] } as never);
    ownerQuery.mockResolvedValueOnce({ rows: [] } as never);
    const { env: runtime, query } = env();

    await expect(drainBillingEvidenceScans(runtime as never, 1)).rejects.toThrow('BILLING_EVIDENCE_SOURCE_MISSING');

    expect(ownerQuery).toHaveBeenCalledWith(expect.anything(), 'scan_claim', [expect.any(Number), expect.any(Number), expect.any(String)]);
    expect(ownerQuery).toHaveBeenCalledWith(expect.anything(), 'scan_source', ['ev-1']);
    expect(query).not.toHaveBeenCalled();
  });

  it('reads API evidence and account existence by typed kinds with only positional values and no local SQL', async () => {
    ownerQuery.mockReset();
    const query = vi.fn(async () => ({ rows: [] }));
    const repository = billingApiRepository(() => ({ pool: { query } }) as never, true);

    await repository.scopedEvidence('ev-1', 'h001');
    expect(ownerQuery).toHaveBeenCalledWith(expect.anything(), 'evidence_scoped_read', ['ev-1', 'h001']);
    await repository.existingEvidence('ev-1');
    expect(ownerQuery).toHaveBeenCalledWith(expect.anything(), 'evidence_existing_read', ['ev-1']);
    await repository.billingAccountExists('h001');
    expect(ownerQuery).toHaveBeenCalledWith(expect.anything(), 'billing_account_exists', ['h001']);

    expect(ownerQuery).toHaveBeenCalledTimes(3);
    expect(query).not.toHaveBeenCalled();
  });
});

describe('Billing beta webhook read tail', () => {
  it('routes customer, inboxState and legacyCustomer through typed owner kinds with no local SQL', async () => {
    const prepare = vi.fn();
    const runtime = {
      CONTROL_DB: { prepare, batch: vi.fn(async () => ({})) },
      BETTER_AUTH_URL: 'https://fixture.example',
      STRIPE_SECRET_KEY: 'sk_test_fixture',
      STRIPE_WEBHOOK_SECRET: 'whsec_test',
    } as never;
    ownerBind.mockReset();
    ownerBind.mockImplementation(async (_db: unknown, kind: string, _values: readonly unknown[]) => ({
      bind: vi.fn(),
      first: vi.fn(async () =>
        kind === 'beta_customer'
          ? { customer_id: 'cus_1' }
          : kind === 'beta_inbox_state'
            ? { state: 'pending' }
            : { holding_id: 'h001' },
      ),
    }));
    vi.stubGlobal('fetch', vi.fn(async () => Response.json({ url: 'https://billing.stripe.com/b/session/test' })));
    try {
      await billingLink(runtime, 'h001', 'acme', 'owner@test', 'portal');
      await applyLegacyStripeEvent(
        runtime,
        {
          id: 'evt-1',
          type: 'customer.subscription.updated',
          created: 1,
          livemode: false,
          data: { object: { customer: 'cus_1' } },
        } as never,
        async () => ({ data: [] } as never),
      );
    } finally {
      vi.unstubAllGlobals();
    }
    expect(ownerBind).toHaveBeenCalledWith(expect.anything(), 'beta_customer', ['h001']);
    expect(ownerBind).toHaveBeenCalledWith(expect.anything(), 'beta_inbox_state', ['evt-1']);
    expect(ownerBind).toHaveBeenCalledWith(expect.anything(), 'beta_legacy_customer', ['cus_1']);
    expect(prepare).not.toHaveBeenCalled();
  });
});

describe('Billing seat-forecast owner-local price read tail', () => {
  it('reads current and future prices by typed owner kinds with only the forecast month bind and no local SQL', async () => {
    ownerQuery.mockReset();
    const query = vi.fn(async () => ({ rows: [], rowCount: 0 }));
    const repository = nativeSeatForecastRepository({ query } as never);

    await repository.currentPrice('2027-02');
    expect(ownerQuery).toHaveBeenCalledWith(expect.anything(), 'forecast_current_price', ['2027-02']);
    await repository.futurePrice('2027-02');
    expect(ownerQuery).toHaveBeenCalledWith(expect.anything(), 'forecast_future_price', ['2027-02']);

    expect(ownerQuery).toHaveBeenCalledTimes(2);
    expect(query).not.toHaveBeenCalled();
  });

  it('leaves the commercial usage reads on the caller client', async () => {
    ownerQuery.mockReset();
    const query = vi.fn(async (_sql: string, _values: unknown[]) => ({ rows: [], rowCount: 0 }));
    const repository = nativeSeatForecastRepository({ query } as never);

    await repository.usageSnapshot('h001', '2027-01');
    await repository.retainedStorageTotals('h001', '2027-01', 'snap-1');

    expect(ownerQuery).not.toHaveBeenCalled();
    expect(query).toHaveBeenCalledTimes(2);
  });
});

describe('Billing target-rollout owner-local price read tail', () => {
  it('reads the effective price and rated shadow period by typed owner kinds with only positional binds and no local SQL', async () => {
    ownerQuery.mockReset();
    const query = vi.fn(async () => ({ rows: [], rowCount: 0 }));
    const repository = nativeTargetRolloutRepository({ query } as never);

    await repository.effectivePrice('2027-02');
    expect(ownerQuery).toHaveBeenCalledWith(expect.anything(), 'target_rollout_effective_price', ['2027-02']);
    await repository.ratedShadowPeriod('h001', '2027-02');
    expect(ownerQuery).toHaveBeenCalledWith(expect.anything(), 'target_rollout_rated_shadow_period', ['h001', '2027-02']);

    expect(ownerQuery).toHaveBeenCalledTimes(2);
    expect(query).not.toHaveBeenCalled();
  });

  it('reads available operating costs and capacity review by typed owner kinds with only positional binds and no local SQL', async () => {
    ownerQuery.mockReset();
    const query = vi.fn(async () => ({ rows: [], rowCount: 0 }));
    const repository = nativeTargetRolloutRepository({ query } as never);

    await repository.availableOperatingCosts('h001', '2027-02');
    expect(ownerQuery).toHaveBeenCalledWith(expect.anything(), 'target_rollout_available_operating_costs', ['h001', '2027-02']);
    await repository.capacityReview('h001', '2027-02');
    expect(ownerQuery).toHaveBeenCalledWith(expect.anything(), 'target_rollout_capacity_review', ['h001', '2027-02']);

    expect(ownerQuery).toHaveBeenCalledTimes(2);
    expect(query).not.toHaveBeenCalled();
  });

  it('leaves the commercial usage snapshot on the caller client', async () => {
    ownerQuery.mockReset();
    const query = vi.fn(async (_sql: string, _values: unknown[]) => ({ rows: [], rowCount: 0 }));
    const repository = nativeTargetRolloutRepository({ query } as never);

    await repository.usageSnapshot('h001', '2027-01');

    expect(ownerQuery).not.toHaveBeenCalled();
    expect(query).toHaveBeenCalledTimes(1);
  });
});

describe('Billing target statements owner-local read tail', () => {
  it('reads approved and provider statements by one typed owner kind with only positional binds', async () => {
    ownerQuery.mockReset();
    const query = vi.fn(async (_sql: string, _values: unknown[]) => ({ rows: [], rowCount: 0 }));
    const repository = nativeTargetBillingRepository({ query } as never);

    await repository.approvedStatements('h001', '2027-02');
    expect(ownerQuery).toHaveBeenCalledWith(expect.anything(), 'target_statements', ['h001', '2027-02']);
    await repository.providerStatements('h001', '2027-02');
    expect(ownerQuery).toHaveBeenCalledWith(expect.anything(), 'target_statements', ['h001', '2027-02']);

    expect(ownerQuery).toHaveBeenCalledTimes(2);
    expect(query).not.toHaveBeenCalled();
  });
});
