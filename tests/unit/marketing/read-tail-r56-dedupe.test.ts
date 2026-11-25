import { describe, it, expect } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import ts from 'typescript';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);
const src = (name: string) => readFile(adapter(name), 'utf8');

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

describe('marketing read-tail r56 native campaign-existence cutover', () => {
  it('deletes the campaign-existence TS wrapper and wires both consumers to the owner port', async () => {
    await expect(src('campaign-existence.ts')).rejects.toThrow();
    const submission = await src('public-submission.ts');
    const campaigns = await src('campaigns.ts');
    expect(submission).toContain('nativeMarketingReadFirst<Record<string,unknown>>(MARKETING_RD_CAMPAIGN_EXISTS,{campaignId:id,companyId},db)');
    expect(campaigns).toContain('const exists=await nativeMarketingPlan(MARKETING_RD_CAMPAIGN_EXISTS)');
    expect(campaigns).toContain('`EXISTS(${exists.sql})`');
    for (const consumer of [submission, campaigns]) {
      expect(consumer).not.toContain('campaign-existence');
      expect(consumer).not.toContain('marketingCampaignExistsSql');
    }
  });

  it('renders the analytics touch source from the shared owner head', async () => {
    const source = await src('analytics-sources.ts');
    expect(source.match(/SELECT /g)).toBeNull();
    expect(source).toContain("marketingSelectHeadSql('touch.*',` FROM marketing_touches touch");
    expect(source).toContain('touch:marketingAnalyticsTouchSql(true,touchRows)');
    expect(source).toContain('touch:marketingAnalyticsTouchSql(false,touchRows)');
  });

  it('leaves the shared owner point head and the retained settlement/lock reads intact', async () => {
    expect((await src('owner-point-read-sql.ts')).match(/SELECT /g)).toHaveLength(1);
    expect(await src('purchases.ts')).toContain('nativeMarketingReadFirst<Record<string,unknown>>(MARKETING_RD_COMMAND_RECEIPT_PRIOR');
    expect(await src('public-form.ts')).toContain('FOR SHARE');
    expect(await src('native-marketing-read.ts')).toContain('MARKETING_RD_CAMPAIGN_EXISTS');
  });

  it('retirement:ts-statements stays strictly below BEFORE=14 at 12 after r57', async () => {
    expect(await ownerTokens()).toEqual({ spaced: 10, bare: 10 });
  });
});
