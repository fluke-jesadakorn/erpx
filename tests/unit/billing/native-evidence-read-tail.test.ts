import { describe, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));

// The retired owner-local evidence reader builds no SQL of its own. Mock the
// native owner port and assert the exact typed kind and positional values it
// sends; the caller's pool must never receive a TypeScript-authored SELECT.
const ownerQuery = vi.hoisted(() => vi.fn(async (_client: unknown, _kind: string, _values: unknown[]) => ({ rows: [] } as never)));
const ownerBind = vi.hoisted(() => vi.fn(async () => ({ bind: vi.fn() })));
vi.mock('../../../packages/server-billing/src/adapters/native-billing-owner-write', () => ({
  nativeBillingOwnerQuery: ownerQuery,
  nativeBillingOwnerBind: ownerBind,
}));
const { drainBillingEvidenceScans } = await import('@erpx/server-billing/adapters/billing-attachments');
const { billingApiRepository } = await import('@erpx/server-billing/adapters/billing-api');

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
