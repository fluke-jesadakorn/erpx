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
    const head = await readFile(new URL('owner-projection-source.ts', base), 'utf8');
    const commercial = await readFile(new URL('commercial-owner-projection-sources.ts', base), 'utf8');

    // r40 renders all four quoted projections from one closed column-list/table
    // head; the byte oracles are frozen in read-tail-r40-dedupe.test.ts.
    expect(head.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(head).toContain('marketingSelectHeadSql(');
    for (const source of [hospitality, lastAudit, commercial]) expect(source).not.toMatch(/SELECT /);
    expect(hospitality).toContain('marketingProjectionSourceSql(');
    expect(lastAudit).toContain('marketingProjectionSourceSql(');
    expect(commercial).toContain('marketingProjectionSourceSql(');
    expect(commercial).toContain('marketingCampaignProjection');
    expect(commercial).toContain('marketingCampaignCostProjection');
  });

  it('keeps one fixed owner predicate for attachment owner version guards', async () => {
    const { readFile } = await import('node:fs/promises');
    const base = new URL('../../../packages/server-marketing/src/adapters/', import.meta.url);
    const guard = await readFile(new URL('attachment-owner-version-guard.ts', base), 'utf8');
    const service = await readFile(new URL('attachments-service-facts.ts', base), 'utf8');
    const operational = await readFile(new URL('attachments-operational-facts.ts', base), 'utf8');

    const predicate = 'EXISTS(SELECT 1 FROM ${source} WHERE ${tail})';
    // The shared point-existence head owns the single guard token; the wrappers
    // now delegate to it with an owner table and a fixed where-tail.
    expect(guard.match(/SELECT/g)).toBeNull();
    expect(guard).toContain('export const attachmentOwnerVersionGuardSql');
    expect(guard).toContain('export const marketingSourceRowGuardSql');
    expect(guard).toContain('`EXISTS(${marketingPointExistsSql(source, tail)})`');
    expect(guard).not.toContain(predicate);
    expect(guard).toContain('marketingSourceRowGuardSql(table, `id=? AND company_id=? AND version=?${extra ? ` AND ${extra}` : ""}`)');

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
    // r44: the witness now renders from the shared point-existence head.
    expect(sources).not.toContain(witness);
    expect(sources).not.toMatch(/SELECT 1 FROM marketing_public_receipts receipt/);
    expect(sources).toContain('export const publicSubmissionReceiptWitness=marketingPointExistsSql(');
    expect(sources).toContain("'marketing_public_receipts receipt JOIN ('+crmMarketingCandidateSources+') candidate ON candidate.id=receipt.candidate_id'");

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
    // r40 moved the single source-witness SELECT into the shared point-read
    // head; this wrapper only delegates with the qualifier flag.
    const head = await readFile(new URL('owner-point-read-sql.ts', base), 'utf8');
    const { marketingOwnerPointReadSql } = await import(
      '../../../packages/server-marketing/src/adapters/owner-point-read-sql'
    );
    expect(marketingOwnerPointReadSql('${projection}', '${table} source', true)).toBe(predicate);
    expect(head.match(/SELECT /g) ?? []).toHaveLength(0);
    expect(head).toContain('marketingSelectHeadSql(');
    expect(witness).not.toMatch(/SELECT/);
    expect(witness).toContain('export const attachmentSourceWitnessSql');
    expect(witness).toContain('marketingOwnerPointReadSql(projection, `${table} source`, true)');

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

    // The presentation assertion keeps the shared EXISTS helper; the content
    // reader itself is now a closed native lane-40 owner read.
    expect(form).toContain("const contentPredicate=`EXISTS(${marketingFormContentSql('1')})`");
    expect(form).toContain('readMarketingLane40ForCompany(LANE40_FORM_CONTENT_ROW,companyId,');
    expect(form).not.toContain("marketingFormContentSql('id,version,preview_json')");
    expect(form).not.toContain("SELECT id,version,preview_json FROM marketing_assets");
    expect(form).not.toContain("SELECT 1 FROM marketing_assets WHERE id=? AND company_id=? AND kind='content'");
  });

  it('routes campaign existence reads through the native owner read port', async () => {
    const { readFile } = await import('node:fs/promises');
    const base = new URL('../../../packages/server-marketing/src/adapters/', import.meta.url);
    await expect(readFile(new URL('campaign-existence.ts', base), 'utf8')).rejects.toThrow();
    const submission = await readFile(new URL('public-submission.ts', base), 'utf8');
    const campaigns = await readFile(new URL('campaigns.ts', base), 'utf8');

    const predicate = 'SELECT 1 FROM marketing_campaigns WHERE id=? AND company_id=?';
    // r56 deleted the TS wrapper: the owner read port owns the exact statement
    // and both consumers now run it on their own Holding transaction.
    for (const consumer of [submission, campaigns]) {
      expect(consumer).toContain('MARKETING_RD_CAMPAIGN_EXISTS');
      expect(consumer).not.toContain('campaign-existence');
      expect(consumer).not.toContain(predicate);
    }
    expect(submission).toContain('nativeMarketingReadFirst<Record<string,unknown>>(MARKETING_RD_CAMPAIGN_EXISTS,{campaignId:id,companyId},db)');
    expect(campaigns).toContain('nativeMarketingPlan(MARKETING_RD_CAMPAIGN_EXISTS)');
    expect(campaigns).toContain('`EXISTS(${exists.sql})`');
    expect(campaigns).toContain('[i.campaignId,a.companyId]');
  });

  it('keeps one fixed owner projection for executive evidence row reads', async () => {
    const { readFile } = await import('node:fs/promises');
    const base = new URL('../../../packages/server-marketing/src/adapters/', import.meta.url);
    const helper = await readFile(new URL('executive-evidence-rows.ts', base), 'utf8');
    const evidence = await readFile(new URL('executive-evidence-sources.ts', base), 'utf8');
    const workspace = await readFile(new URL('marketing.ts', base), 'utf8');

    const candidatePredicate = 'SELECT c.id,c.contact_name,c.status,c.form_version,c.version,c.created_at,c.updated_at,c.branch_id,c.department_id,f.name form_name';
    const eventPredicate = 'SELECT e.id,e.candidate_id,e.event_type,e.channel,e.version event_version,e.created_at,c.contact_name,c.status candidate_status,c.version candidate_version,c.branch_id,c.department_id,f.name form_name';
    // r42 merges both tails into one closed-variant builder; r47 folds its
    // SELECT head into the shared renderer. One asset join and the two
    // owner-local column lists remain.
    expect(helper.match(/SELECT/g)).toBeNull();
    expect(helper).toContain('marketingSelectHeadSql(');
    expect(helper.match(/ JOIN marketing_assets f ON f.id=c.form_id AND f.company_id=c.company_id/g)).toHaveLength(1);
    expect(helper).toContain('export function marketingEvidenceCandidateRows');
    expect(helper).toContain('export function marketingEvidenceEventRows');
    expect(helper).toContain(candidatePredicate.slice('SELECT '.length));
    expect(helper).toContain(eventPredicate.slice('SELECT '.length));

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

  it('routes workspace candidate row reads through the native lane-40 owner', async () => {
    const { readFile } = await import('node:fs/promises');
    const base = new URL('../../../packages/server-marketing/src/adapters/', import.meta.url);
    const workspace = await readFile(new URL('marketing.ts', base), 'utf8');
    const client = await readFile(new URL('native-lane-40/client.ts', base), 'utf8');

    // Lane 40 deleted the TS candidate projection: the axum owner now owns
    // the query and the form-name calculation, and the bridge owns no SQL.
    await expect(readFile(new URL('candidate-form-rows.ts', base), 'utf8')).rejects.toThrow();
    expect(workspace).not.toContain('marketingCandidateFormRows');
    expect(workspace).toContain('LANE40_CANDIDATE_LIST');
    expect(workspace).toContain('LANE40_CANDIDATE_DETAIL');
    expect(client).toContain("MARKETING_LANE40_READ_UNAVAILABLE");
    expect(client).not.toMatch(/SELECT /);
  });

  it('keeps one fixed owner projection for campaign-cost analytics rows', async () => {
    const { readFile } = await import('node:fs/promises');
    const base = new URL('../../../packages/server-marketing/src/adapters/', import.meta.url);
    const helper = await readFile(new URL('analytics-cost-rows.ts', base), 'utf8');
    const head = await readFile(new URL('analytics-null-scope-rows.ts', base), 'utf8');
    const analytics = await readFile(new URL('analytics-sources.ts', base), 'utf8');

    const projection = 'SELECT cost.*,NULL::text branch_id,NULL::text department_id FROM ${source}';
    // r39 moves the single projection token into the shared NULL-scope head;
    // r47 folds that head into marketingSelectHeadSql. The cost wrapper still
    // delegates with its admitted projection and never smuggles a token into a
    // comment.
    expect(helper.match(/SELECT/g)).toBeNull();
    expect(helper).toContain('export const marketingAnalyticsCostRows');
    expect(helper).toContain("marketingAnalyticsRowsSql('cost.*', source)");
    expect(head.match(/SELECT/g)).toBeNull();
    expect(head).toContain('marketingSelectHeadSql(');
    expect(head).toContain('NULL::text branch_id,NULL::text department_id`');
    expect(head).toContain('` FROM ${source}`');

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

    // r43 routes the reviewer guard through the shared actor-scope head; the
    // canonical scope helper and the new head each own exactly one read token.
    // r49 routes the canonical scope head, r50 the assignment guard head, through the shared renderer (1 -> 0).
    expect(helper.match(/SELECT/g) ?? []).toHaveLength(0);
    expect(helper).toContain('export const attachmentReviewerGuardSql');
    expect(helper).toContain("actorScopeAssignmentSql(`(${assignmentReadSql('marketing_file_review','records.review')} OR ${assignmentReadSql('marketing_file_review','commerce.approve')})`,'marketing_file_review',alias ? ` ${alias}` : '')");
    const scopeHelper = await readFile(new URL('marketing-actor-scope-sql.ts', base), 'utf8');
    expect(scopeHelper.match(/SELECT/g) ?? []).toHaveLength(0);
    const headHelper = await readFile(new URL('marketing-actor-scope-guard.ts', base), 'utf8');
    expect(headHelper.match(/SELECT/g) ?? []).toHaveLength(0);
    expect(headHelper).toContain("from './marketing-actor-scope-sql'");
    expect(headHelper).toContain('marketingSelectHeadSql(');
    expect(helper).not.toContain('(SELECT ?::text company_id,?::text branch_id,?::text department_id)');
    expect(helper).toContain("from './marketing-actor-scope-guard'");

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
    // The canonical helper owned both read tokens; its comment must not smuggle
    // another token into the retirement KPI.
    // r44: the outer EXISTS head renders from the shared source-row guard;
    // r49 routes the remaining scope tuple through the shared head renderer.
    expect(helper.match(/SELECT/g) ?? []).toHaveLength(0);
    expect(helper).toContain('export const marketingRecordScopeAuthoritySql');
    expect(helper).not.toContain(predicate);
    expect(helper).toContain('marketingSourceRowGuardSql(');

    for (const consumer of [recordPurchases, analytics]) {
      expect(consumer).toContain('marketingRecordScopeAuthoritySql(');
      expect(consumer).not.toContain('FROM (SELECT marketing_');
    }
    // Distinct source/authority aliases and bind order are preserved verbatim.
    expect(recordPurchases).toContain("marketingRecordScopeAuthoritySql('marketing_source','marketing_authority')");
    expect(analytics).toContain("marketingRecordScopeAuthoritySql('marketing_report','marketing_scope')");
  });
});
