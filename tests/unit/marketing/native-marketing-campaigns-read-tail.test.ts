import { describe, expect, it, vi } from 'vitest';
vi.mock('server-only', () => ({}));

// The retired owner-local campaign list built its own SELECT. The adapter now
// sends only the owner-authored statement kind and positional values; assert
// the exact native transport call and that the caller's SQL database is never
// asked to prepare the removed query.
const native = vi.hoisted(() => {
  const statement = {
    bind: vi.fn(),
    all: vi.fn(),
    first: vi.fn(),
    run: vi.fn(),
    raw: vi.fn(),
  };
  statement.bind.mockReturnValue(statement);
  statement.all.mockResolvedValue({
    results: [{ id: 'campaign-1', code: 'AUTUMN', name: 'Autumn', version: 1 }],
    success: true,
    meta: { changes: 1, duration: 0 },
  });
  return { statement, statementFn: vi.fn(() => statement), bind: statement.bind, all: statement.all };
});

vi.mock('@erpx/feature-marketing/adapters/server/native-marketing-command', () => ({
  nativeMarketingStatement: native.statementFn,
  nativeCampaignCostLink: vi.fn(),
}));
vi.mock('@erpx/feature-marketing/adapters/server/native-marketing-wave2', () => ({
  nativeCreateCampaign: vi.fn(),
}));
vi.mock('@erpx/server-marketing/adapters/purchases', () => ({
  requireMarketingPurchasedRead: vi.fn(async () => undefined),
  marketingFinanceHistory: vi.fn(async () => []),
  withMarketingPurchasedCommand: vi.fn(),
}));
vi.mock('@erpx/server-identity_access/public/iam-permissions', () => ({
  requirePermission: vi.fn(),
}));
vi.mock('@erpx/catalogue/module-access', () => ({ hasModuleAccess: vi.fn(() => true) }));
vi.mock('../../../packages/server-runtime/src/public/runtime', () => ({
  holdingResources: vi.fn(async () => ({ write: {} })),
}));

const { marketingCampaigns } = await import('@erpx/server-marketing/adapters/campaigns');

const actor = {
  userId: 'user', holdingId: 'h001', holdingSlug: 'acme', companyId: 'company',
  branchId: null, departmentId: null, assignmentId: 'assignment', role: 'owner',
  modules: ['marketing'], iam: { permissions: ['records.read'] },
};

describe('Marketing owner-local campaign read tail', () => {
  it('reads the workspace campaign list through the native owner statement', async () => {
    native.statementFn.mockClear();
    native.bind.mockClear();
    native.all.mockClear();

    const rows = await marketingCampaigns(actor as never);

    expect(rows).toEqual([{ id: 'campaign-1', code: 'AUTUMN', name: 'Autumn', version: 1 }]);
    expect(native.statementFn).toHaveBeenCalledWith('marketing_campaigns_read', { actor });
    expect(native.bind).toHaveBeenCalledWith('company');
    expect(native.all).toHaveBeenCalledTimes(1);
  });
});
