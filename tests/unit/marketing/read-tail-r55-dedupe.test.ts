import { describe, it, expect } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import ts from 'typescript';
import { marketingPackageSettlementSources } from '../../../packages/server-marketing/src/adapters/package-obligations';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);
const src = (name: string) => readFile(adapter(name), 'utf8');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

// Byte oracle captured at BEFORE=16 HEAD before r55 routed the candidate
// settlement projection through the shared selection/point-existence heads.
// The native PACKAGE_OBLIGATIONS read plan inlines this exact statement, so the
// TS source must emit the same bytes without owning another SELECT head.
const CANDIDATES = `SELECT c.*,valid.id IS NOT NULL AND EXISTS(SELECT 1 FROM marketing_assets form JOIN marketing_asset_events activated ON activated.asset_id=form.id AND activated.version=c.form_version AND activated.action='activate' AND activated.actor_id<>form.created_by JOIN command_receipts command ON activated.id='marketing-asset-event-'||command.command_id AND command.operation='marketing.asset.activate' AND command.record_id=form.id AND command.actor_id=activated.actor_id AND command.authority_json::jsonb->>'companyId'=c.company_id WHERE form.id=c.form_id AND form.company_id=c.company_id) source_valid FROM marketing_lead_candidates c LEFT JOIN (SELECT candidate.* FROM marketing_lead_candidates candidate
 JOIN marketing_assets form ON form.id=candidate.form_id AND form.company_id=candidate.company_id AND form.kind='form' AND form.version>=candidate.form_version
 JOIN marketing_asset_events captured ON captured.asset_id=form.id AND captured.version=candidate.form_version AND captured.action='activate' AND captured.status='active'
 JOIN marketing_engagement_events submission ON submission.candidate_id=candidate.id AND submission.version=1 AND submission.event_type='form_submit'
 WHERE length(trim(candidate.consent_evidence))>=3) valid ON valid.id=c.id AND valid.company_id=c.company_id`;

/** AST evidence: count SELECT tokens only inside string/template text nodes. */
function headTokens(text: string): number {
  const file = ts.createSourceFile('owner.ts', text, ts.ScriptTarget.Latest, true);
  let total = 0;
  const visit = (node: ts.Node) => {
    if (
      ts.isStringLiteral(node) ||
      ts.isNoSubstitutionTemplateLiteral(node) ||
      ts.isTemplateHead(node) ||
      ts.isTemplateMiddle(node) ||
      ts.isTemplateTail(node)
    )
      total += (node.text.match(/SELECT /g) ?? []).length;
    ts.forEachChild(node, visit);
  };
  visit(file);
  return total;
}
async function ownerTokens(): Promise<{ spaced: number; bare: number }> {
  let spaced = 0,
    bare = 0;
  const walk = async (dir: string) => {
    for (const entry of await readdir(dir, { withFileTypes: true })) {
      const path = resolve(dir, entry.name);
      if (entry.isDirectory()) await walk(path);
      else if (entry.name.endsWith('.ts')) {
        const text = await readFile(path, 'utf8');
        spaced += headTokens(text);
        bare += (text.match(/SELECT/g) ?? []).length;
      }
    }
  };
  await walk(resolve('packages/server-marketing/src'));
  return { spaced, bare };
}

describe('marketing read-tail r55 native read-port cutover', () => {
  it('renders the byte-identical candidate settlement projection from shared heads', () => {
    const rendered = marketingPackageSettlementSources.candidates;
    expect(rendered).toBe(CANDIDATES);
    expect(sha(rendered)).toBe('25b2bfd56f7dba70af3306635aa1fe1ca3fbc7173603adaafa2c087516624a7b');
    expect(rendered).toHaveLength(1291);
    expect(rendered).toContain('command_receipts');
    expect(rendered).toContain('marketing_lead_candidates c LEFT JOIN');
  });

  it('deletes the inline candidate SELECT family from the TS owner', async () => {
    const source = await src('package-obligations.ts');
    expect(source.match(/SELECT /g)).toBeNull();
    expect(source).toContain('marketingSelectHeadSql(');
    expect(source).toContain('marketingPointExistsSql(');
    expect(source).toContain('MARKETING_RD_PACKAGE_OBLIGATIONS');
    expect(source).toContain('nativeMarketingReadAll<{kind:string;id:string;fact:unknown;pending:boolean}>(MARKETING_RD_PACKAGE_OBLIGATIONS');
  });

  it('cuts the workspace asset point/list reads over with no TS fallback', async () => {
    const source = await src('marketing.ts');
    expect(source).not.toContain('nativeMarketingReadConfigured');
    expect(source).toContain('nativeMarketingReadAll<Row>(MARKETING_RD_ASSET_LIST,{companyId:a.companyId},write)');
    expect(source).toContain('nativeMarketingReadFirst<Row>(MARKETING_RD_ASSET_POINT,{assetId:id,companyId:a.companyId},write)');
    expect(source).not.toContain("marketingCompanyListSql('marketing_assets','')");
    expect(source).not.toContain("marketingLocalPointReadSql('asset')");
  });

  it('retirement:ts-statements stays strictly below BEFORE=16 at 13 after r56', async () => {
    expect(await ownerTokens()).toEqual({ spaced: 5, bare: 5 });
  });
});
