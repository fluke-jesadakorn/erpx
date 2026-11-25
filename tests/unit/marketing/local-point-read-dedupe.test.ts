import { describe, it, expect } from 'vitest';
import { readFile } from 'node:fs/promises';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);

describe('marketing owner-local point read dedupe', () => {
  it('emits byte-identical SQL for the asset, candidate/lock and history variants', async () => {
    const { marketingLocalPointReadSql } = await import(
      '../../../packages/server-marketing/src/adapters/marketing-local-point-read-sql.ts'
    );
    expect(marketingLocalPointReadSql('asset')).toBe(
      'SELECT * FROM marketing_assets WHERE id=? AND company_id=?',
    );
    expect(marketingLocalPointReadSql('candidate', false)).toBe(
      'SELECT * FROM marketing_lead_candidates WHERE id=? AND company_id=? ',
    );
    expect(marketingLocalPointReadSql('candidate', true)).toBe(
      'SELECT * FROM marketing_lead_candidates WHERE id=? AND company_id=? FOR UPDATE',
    );
    expect(marketingLocalPointReadSql('history')).toBe(
      'SELECT * FROM marketing_engagement_events WHERE candidate_id=? ORDER BY version',
    );
  });

  it('emits byte-identical SQL for the attachment owner fact reads', async () => {
    const { attachmentOwnerFactReadSql } = await import(
      '../../../packages/server-marketing/src/adapters/attachment-owner-fact-read-sql.ts'
    );
    expect(attachmentOwnerFactReadSql('asset')).toBe(
      'SELECT company_id,NULL::text branch_id,NULL::text department_id,version,kind FROM marketing_assets WHERE id=? AND company_id=?',
    );
    expect(attachmentOwnerFactReadSql('campaign')).toBe(
      'SELECT company_id,version FROM marketing_campaigns WHERE id=? AND company_id=?',
    );
  });

  it('keeps one query literal per helper and no inline point-read literals in consumers', async () => {
    const local = await readFile(adapter('marketing-local-point-read-sql.ts'), 'utf8');
    const ownerFact = await readFile(adapter('attachment-owner-fact-read-sql.ts'), 'utf8');
    const repo = await readFile(adapter('marketing.ts'), 'utf8');
    const operational = await readFile(adapter('attachments-operational-facts.ts'), 'utf8');
    const policy = await readFile(adapter('attachments-policy-facts.ts'), 'utf8');

    // r45 moved the local helper's query token into the shared read-tail head;
    // r40 moved the owner-fact token into the shared point-read head, so both
    // wrappers only delegate.
    const head = await readFile(adapter('read-tail-sql.ts'), 'utf8');
    expect(head.match(/SELECT/g)).toHaveLength(1);
    expect(local.match(/SELECT/g)).toBeNull();
    expect(local).toContain('marketingReadTailSql(table,');
    expect(ownerFact.match(/SELECT/g)).toBeNull();
    expect(ownerFact).toContain('marketingOwnerPointReadSql(columns, table)');

    expect(repo).not.toContain('FROM marketing_assets WHERE id=? AND company_id=?"');
    expect(repo).not.toContain('`SELECT * FROM marketing_lead_candidates WHERE id=?');
    expect(repo).not.toContain('"SELECT * FROM marketing_engagement_events WHERE candidate_id=?');
    expect(repo).not.toContain("marketingLocalPointReadSql('asset')");
    expect(repo).toContain('nativeMarketingReadFirst<Row>(MARKETING_RD_ASSET_POINT');
    expect(repo).toContain("marketingLocalPointReadSql('candidate',lock)");
    expect(repo).toContain('LANE40_CANDIDATE_HISTORY');

    expect(operational).toContain("attachmentOwnerFactReadSql('asset')");
    expect(operational).not.toContain('NULL::text branch_id,NULL::text department_id,version,kind FROM marketing_assets');
    expect(policy).toContain("attachmentOwnerFactReadSql('campaign')");
    expect(policy).not.toContain('SELECT company_id,version FROM marketing_campaigns');
  });

  it('preserves the bind order of every point read', async () => {
    const repo = await readFile(adapter('marketing.ts'), 'utf8');
    const operational = await readFile(adapter('attachments-operational-facts.ts'), 'utf8');
    const policy = await readFile(adapter('attachments-policy-facts.ts'), 'utf8');
    expect(repo).toContain('nativeMarketingReadFirst<Row>(MARKETING_RD_ASSET_POINT,{assetId:id,companyId:a.companyId},write)');
    expect(repo).toContain("marketingLocalPointReadSql('candidate',lock),\n    )\n    .bind(id, a.companyId)");
    expect(repo).toContain('readMarketingLane40(LANE40_CANDIDATE_HISTORY, a, {recordId: id})');
    expect(operational).toContain("attachmentOwnerFactReadSql('asset')).bind(facts.targetId,facts.companyId)");
    expect(policy).toContain("attachmentOwnerFactReadSql('campaign')).bind(facts.targetId,facts.companyId)");
  });
});
