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

  it('keeps one fixed owner projection for the campaign and campaign-cost reads', async () => {
    const { readFile } = await import('node:fs/promises');
    const base = new URL('../../../packages/server-marketing/src/adapters/', import.meta.url);
    const hospitality = await readFile(new URL('hospitality-owner-projection-sources.ts', base), 'utf8');
    const lastAudit = await readFile(new URL('last-audit-owner-projection-sources.ts', base), 'utf8');
    const commercial = await readFile(new URL('commercial-owner-projection-sources.ts', base), 'utf8');

    expect(hospitality.match(/SELECT \\"id\\",\\"company_id\\",\\"code\\",\\"name\\",\\"created_by\\",\\"created_at\\",\\"version\\" FROM marketing_campaigns/g)).toHaveLength(1);
    expect(lastAudit.match(/SELECT \\"id\\",\\"company_id\\",\\"campaign_id\\",\\"journal_line_id\\",\\"evidence\\",\\"reviewed_by\\",\\"created_at\\" FROM marketing_campaign_costs/g)).toHaveLength(1);
    expect(commercial).not.toContain('SELECT \\"id\\",\\"company_id\\",\\"code\\"');
    expect(commercial).not.toContain('SELECT \\"id\\",\\"company_id\\",\\"campaign_id\\"');
    expect(commercial).toContain('marketingCampaignProjection');
    expect(commercial).toContain('marketingCampaignCostProjection');
  });

  it('keeps one fixed owner predicate for attachment owner version guards', async () => {
    const { readFile } = await import('node:fs/promises');
    const base = new URL('../../../packages/server-marketing/src/adapters/', import.meta.url);
    const guard = await readFile(new URL('attachment-owner-version-guard.ts', base), 'utf8');
    const service = await readFile(new URL('attachments-service-facts.ts', base), 'utf8');
    const operational = await readFile(new URL('attachments-operational-facts.ts', base), 'utf8');

    const predicate = 'EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=?';
    // The canonical helper owns the single guard predicate; its comment must
    // not smuggle a second "SELECT" token into the retirement KPI.
    expect(guard.match(/SELECT/g)).toHaveLength(1);
    expect(guard).toContain('export const attachmentOwnerVersionGuardSql');
    expect(guard).toContain(predicate);

    for (const consumer of [service, operational]) {
      expect(consumer).toContain('import { attachmentOwnerVersionGuardSql } from "./attachment-owner-version-guard"');
      expect(consumer).not.toContain('EXISTS(SELECT 1 FROM ${');
    }
    expect(service).toContain('attachmentOwnerVersionGuardSql(item.table, item.extra)');
    expect(operational).toContain('attachmentOwnerVersionGuardSql(table)');
  });

  it('keeps one fixed owner projection for the public-submission receipt witness', async () => {
    const { readFile } = await import('node:fs/promises');
    const base = new URL('../../../packages/server-marketing/src/adapters/', import.meta.url);
    const sources = await readFile(new URL('crm-purchase-sources.ts', base), 'utf8');
    const purchases = await readFile(new URL('purchases.ts', base), 'utf8');
    const form = await readFile(new URL('public-form.ts', base), 'utf8');

    const witness = 'SELECT 1 FROM marketing_public_receipts receipt JOIN (${crmMarketingCandidateSources}) candidate ON candidate.id=receipt.candidate_id WHERE receipt.key=? AND candidate.company_id=? AND candidate.form_id=? AND candidate.form_version=?';
    expect(sources.split(witness)).toHaveLength(2);
    expect(sources.match(/SELECT 1 FROM marketing_public_receipts receipt/g)).toHaveLength(1);
    expect(sources).toContain('export const publicSubmissionReceiptWitness=`' + witness + '`');

    for (const consumer of [purchases, form]) {
      expect(consumer).toContain('database.prepare(publicSubmissionReceiptWitness)');
      expect(consumer).not.toContain('database.prepare(`SELECT 1 FROM marketing_public_receipts receipt');
    }
    // The purchase path guards the replay before and after the command; both
    // must read the same canonical witness inside the caller's transaction.
    expect(purchases.match(/database\.prepare\(publicSubmissionReceiptWitness\)/g)).toHaveLength(2);
    expect(form.match(/database\.prepare\(publicSubmissionReceiptWitness\)/g)).toHaveLength(1);
  });

  it('keeps one fixed owner projection for attachment source-witness reads', async () => {
    const { readFile } = await import('node:fs/promises');
    const base = new URL('../../../packages/server-marketing/src/adapters/', import.meta.url);
    const witness = await readFile(new URL('attachment-source-witness.ts', base), 'utf8');
    const purchases = await readFile(new URL('attachment-purchases.ts', base), 'utf8');
    const read = await readFile(new URL('attachment-read.ts', base), 'utf8');

    const predicate = 'SELECT ${projection} FROM ${table} source WHERE source.id=? AND source.company_id=?';
    // The canonical helper owns the single source-witness SELECT; its comment
    // must not smuggle a second "SELECT" token into the retirement KPI.
    expect(witness.match(/SELECT/g)).toHaveLength(1);
    expect(witness).toContain('export const attachmentSourceWitnessSql');
    expect(witness).toContain(predicate);

    for (const consumer of [purchases, read]) {
      expect(consumer).toContain('attachmentSourceWitnessSql(');
      expect(consumer).not.toContain('FROM ${table} source WHERE source.id=?');
    }
    expect(purchases).toContain('attachmentSourceWitnessSql("(to_jsonb(source)-\'version\'-\'updated_at\')::text witness"');
    expect(read).toContain("attachmentSourceWitnessSql('to_jsonb(source)::text image'");
  });
});
