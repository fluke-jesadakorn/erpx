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

  it('keeps one fixed owner projection for public-form content reads', async () => {
    const { readFile } = await import('node:fs/promises');
    const base = new URL('../../../packages/server-marketing/src/adapters/', import.meta.url);
    const helper = await readFile(new URL('public-form-content-source.ts', base), 'utf8');
    const head = await readFile(new URL('form-asset-read-head.ts', base), 'utf8');
    const form = await readFile(new URL('public-form.ts', base), 'utf8');

    const predicate = 'SELECT ${projection} FROM marketing_assets WHERE id=? AND company_id=? AND kind=\'content\' AND status=\'active\' AND version=?';
    // r38 moved the single form-content SELECT into the shared form/content
    // head; the wrapper only delegates. Corrects a stale assertion that still
    // expected the old inline literal at HEAD.
    expect(helper.match(/SELECT/g)).toBeNull();
    expect(helper).toContain('export const marketingFormContentSql');
    expect(helper).toContain("formAssetReadHead({ kind: 'content', projection })");
    expect(head).toContain('FROM marketing_assets WHERE ${predicate}');

    // Both the presentation assertion and the content reader share the helper.
    expect(form).toContain("const contentPredicate=`EXISTS(${marketingFormContentSql('1')})`");
    expect(form).toContain("marketingFormContentSql('id,version,preview_json')");
    expect(form).not.toContain("SELECT id,version,preview_json FROM marketing_assets");
    expect(form).not.toContain("SELECT 1 FROM marketing_assets WHERE id=? AND company_id=? AND kind='content'");
  });

  it('keeps one fixed owner predicate for campaign existence reads', async () => {
    const { readFile } = await import('node:fs/promises');
    const base = new URL('../../../packages/server-marketing/src/adapters/', import.meta.url);
    const helper = await readFile(new URL('campaign-existence.ts', base), 'utf8');
    const submission = await readFile(new URL('public-submission.ts', base), 'utf8');
    const campaigns = await readFile(new URL('campaigns.ts', base), 'utf8');

    const predicate = 'SELECT 1 FROM marketing_campaigns WHERE id=? AND company_id=?';
    // The canonical helper owns the single existence SQL; its comment must not
    // smuggle a second "SELECT" token into the retirement KPI.
    expect(helper.match(/SELECT/g)).toHaveLength(1);
    expect(helper).toContain('export const marketingCampaignExistsSql');
    expect(helper).toContain(predicate);

    for (const consumer of [submission, campaigns]) {
      expect(consumer).toContain("import {marketingCampaignExistsSql} from './campaign-existence'");
      expect(consumer).not.toContain(predicate);
    }
    expect(submission).toContain('db.prepare(marketingCampaignExistsSql).bind(id,companyId).first()');
    expect(campaigns).toContain('`EXISTS(${marketingCampaignExistsSql})`');
  });

  it('keeps one fixed owner projection for executive evidence row reads', async () => {
    const { readFile } = await import('node:fs/promises');
    const base = new URL('../../../packages/server-marketing/src/adapters/', import.meta.url);
    const helper = await readFile(new URL('executive-evidence-rows.ts', base), 'utf8');
    const evidence = await readFile(new URL('executive-evidence-sources.ts', base), 'utf8');
    const workspace = await readFile(new URL('marketing.ts', base), 'utf8');

    const candidatePredicate = 'SELECT c.id,c.contact_name,c.status,c.form_version,c.version,c.created_at,c.updated_at,c.branch_id,c.department_id,f.name form_name';
    const eventPredicate = 'SELECT e.id,e.candidate_id,e.event_type,e.channel,e.version event_version,e.created_at,c.contact_name,c.status candidate_status,c.version candidate_version,c.branch_id,c.department_id,f.name form_name';
    // One canonical SELECT per row shape; its comment must not smuggle extra
    // "SELECT" tokens into the retirement KPI.
    expect(helper.match(/SELECT/g)).toHaveLength(2);
    expect(helper).toContain('export function marketingEvidenceCandidateRows');
    expect(helper).toContain('export function marketingEvidenceEventRows');
    expect(helper).toContain(candidatePredicate);
    expect(helper).toContain(eventPredicate);

    for (const consumer of [evidence, workspace]) {
      expect(consumer).toMatch(/from ['"]\.\/executive-evidence-rows['"]/);
      expect(consumer).not.toContain(candidatePredicate);
      expect(consumer).not.toContain(eventPredicate);
    }
    expect(evidence).toContain('marketingEvidenceCandidateRows(candidateSource)');
    expect(evidence).toContain('marketingEvidenceEventRows(candidateSource)');
    expect(workspace).toContain("marketingEvidenceCandidateRows('marketing_lead_candidates')");
    expect(workspace).toContain("marketingEvidenceEventRows('marketing_lead_candidates')");
  });

  it('keeps one fixed owner projection for workspace candidate row reads', async () => {
    const { readFile } = await import('node:fs/promises');
    const base = new URL('../../../packages/server-marketing/src/adapters/', import.meta.url);
    const helper = await readFile(new URL('candidate-form-rows.ts', base), 'utf8');
    const workspace = await readFile(new URL('marketing.ts', base), 'utf8');

    const projection = 'SELECT c.*,a.name form_name FROM marketing_lead_candidates c JOIN marketing_assets a ON a.id=c.form_id';
    // The canonical helper owns the single workspace candidate projection; its
    // comment must not smuggle an extra token into the retirement KPI.
    expect(helper.match(/SELECT/g)).toHaveLength(1);
    expect(helper).toContain('export function marketingCandidateFormRows');
    expect(helper).toContain(projection);

    expect(workspace).toContain('import {marketingCandidateFormRows} from "./candidate-form-rows"');
    expect(workspace).not.toContain(projection);
    expect(workspace.match(/\$\{marketingCandidateFormRows\(\)\}/g)).toHaveLength(2);
  });

  it('keeps one fixed owner projection for campaign-cost analytics rows', async () => {
    const { readFile } = await import('node:fs/promises');
    const base = new URL('../../../packages/server-marketing/src/adapters/', import.meta.url);
    const helper = await readFile(new URL('analytics-cost-rows.ts', base), 'utf8');
    const head = await readFile(new URL('analytics-null-scope-rows.ts', base), 'utf8');
    const analytics = await readFile(new URL('analytics-sources.ts', base), 'utf8');

    const projection = 'SELECT cost.*,NULL::text branch_id,NULL::text department_id FROM ${source}';
    // r39 moves the single projection token into the shared NULL-scope head;
    // the cost wrapper delegates with its admitted projection and never emits
    // a second token or smuggles one into a comment.
    expect(helper.match(/SELECT/g)).toBeNull();
    expect(helper).toContain('export const marketingAnalyticsCostRows');
    expect(helper).toContain("marketingAnalyticsRowsSql('cost.*', source)");
    expect(head.match(/SELECT/g)).toHaveLength(1);
    expect(head).toContain('NULL::text branch_id,NULL::text department_id FROM ${source}');

    expect(analytics).toContain('import {marketingAnalyticsCostRows} from \'./analytics-cost-rows\'');
    expect(analytics).not.toContain('SELECT cost.*,NULL::text branch_id,NULL::text department_id FROM marketing_campaign_costs');
    expect(analytics.match(/marketingAnalyticsCostRows\(/g)).toHaveLength(2);
    expect(analytics).toContain("marketingAnalyticsCostRows('marketing_campaign_costs cost JOIN marketing_campaigns campaign ON campaign.id=cost.campaign_id AND campaign.company_id=cost.company_id')");
    expect(analytics).toContain("marketingAnalyticsCostRows('marketing_campaign_costs cost')");
  });

  it('keeps one fixed owner guard for attachment reviewer checks', async () => {
    const { readFile } = await import('node:fs/promises');
    const base = new URL('../../../packages/server-marketing/src/adapters/', import.meta.url);
    const helper = await readFile(new URL('attachment-reviewer-guard.ts', base), 'utf8');
    const purchases = await readFile(new URL('attachment-purchases.ts', base), 'utf8');

    const predicate = "SELECT (${assignmentReadSql('marketing_file_review','records.review')} OR ${assignmentReadSql('marketing_file_review','commerce.approve')})${alias ? ` ${alias}` : ''} FROM ${marketingActorScopeSql} marketing_file_review";
    // The guard owns one read token and the canonical actor-scope helper owns
    // the other; neither a comment nor a consumer smuggles a duplicate KPI token.
    expect(helper.match(/SELECT/g)).toHaveLength(1);
    expect(helper).toContain('export const attachmentReviewerGuardSql');
    expect(helper).toContain(predicate);
    const scopeHelper = await readFile(new URL('marketing-actor-scope-sql.ts', base), 'utf8');
    expect(scopeHelper.match(/SELECT/g)).toHaveLength(1);
    expect(helper).not.toContain('(SELECT ?::text company_id,?::text branch_id,?::text department_id)');
    expect(helper).toContain("from './marketing-actor-scope-sql'");

    expect(purchases).toContain('import {attachmentReviewerGuardSql} from "./attachment-reviewer-guard"');
    expect(purchases).not.toContain('assignmentReadSql("marketing_file_review"');
    expect(purchases).toContain("attachmentReviewerGuardSql('allowed')");
    expect(purchases).toContain('`(${attachmentReviewerGuardSql()})`');
  });

  it('keeps one fixed owner predicate for record-scope authority witnesses', async () => {
    const { readFile } = await import('node:fs/promises');
    const base = new URL('../../../packages/server-marketing/src/adapters/', import.meta.url);
    const helper = await readFile(new URL('record-scope-authority.ts', base), 'utf8');
    const recordPurchases = await readFile(new URL('record-purchases.ts', base), 'utf8');
    const analytics = await readFile(new URL('analytics-sources.ts', base), 'utf8');

    const predicate = "EXISTS(SELECT 1 FROM (SELECT ${source}.company_id,COALESCE(${source}.branch_id,?::text) branch_id,COALESCE(${source}.department_id,?::text) department_id) ${authority} WHERE ${assignmentReadSql(authority,'records.read')})";
    // The canonical helper owns both read tokens; its comment must not smuggle
    // another token into the retirement KPI.
    expect(helper.match(/SELECT/g)).toHaveLength(2);
    expect(helper).toContain('export const marketingRecordScopeAuthoritySql');
    expect(helper).toContain(predicate);

    for (const consumer of [recordPurchases, analytics]) {
      expect(consumer).toContain('marketingRecordScopeAuthoritySql(');
      expect(consumer).not.toContain('FROM (SELECT marketing_');
    }
    // Distinct source/authority aliases and bind order are preserved verbatim.
    expect(recordPurchases).toContain("marketingRecordScopeAuthoritySql('marketing_source','marketing_authority')");
    expect(analytics).toContain("marketingRecordScopeAuthoritySql('marketing_report','marketing_scope')");
  });
});
