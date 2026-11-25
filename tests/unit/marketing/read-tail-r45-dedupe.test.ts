import { describe, it, expect } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import ts from 'typescript';
import { marketingCompanyListSql } from '../../../packages/server-marketing/src/adapters/marketing-company-list-sql';
import { marketingLocalPointReadSql } from '../../../packages/server-marketing/src/adapters/marketing-local-point-read-sql';
import { marketingPlainSourceSql } from '../../../packages/server-marketing/src/adapters/marketing-plain-source-sql';
import { marketingReadTailSql } from '../../../packages/server-marketing/src/adapters/read-tail-sql';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);
const src = (name: string) => readFile(adapter(name), 'utf8');

// Oracles frozen from base 777ce3b0e9078a1ead7e4a027857f43854be29fe (HEAD at
// task start); no other worker touched these paths before this packet.
const HEAD = '777ce3b0e9078a1ead7e4a027857f43854be29fe';

/** AST evidence: count SELECT tokens only inside string/template text nodes. */
function selectTokens(text: string): number {
  const file = ts.createSourceFile('owner.ts', text, ts.ScriptTarget.Latest, true);
  let total = 0;
  const visit = (node: ts.Node) => {
    if (
      ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node) ||
      ts.isTemplateHead(node) || ts.isTemplateMiddle(node) || ts.isTemplateTail(node)
    )
      total += (node.text.match(/SELECT /g) ?? []).length;
    ts.forEachChild(node, visit);
  };
  visit(file);
  return total;
}
async function ownerSelectTokens(): Promise<number> {
  let total = 0;
  const walk = async (dir: string) => {
    for (const entry of await readdir(dir, { withFileTypes: true })) {
      const path = resolve(dir, entry.name);
      if (entry.isDirectory()) await walk(path);
      else if (entry.name.endsWith('.ts')) total += selectTokens(await readFile(path, 'utf8'));
    }
  };
  await walk(resolve('packages/server-marketing/src'));
  return total;
}

describe('marketing read-tail r45 dedupe', () => {
  it(`renders the company-list, point and plain-source heads byte-identically from ${HEAD}`, () => {
    expect(marketingCompanyListSql('marketing_assets', '')).toBe(
      'SELECT * FROM marketing_assets WHERE company_id=? ORDER BY updated_at DESC');
    expect(marketingCompanyListSql('marketing_assets', ' AND status=?')).toBe(
      'SELECT * FROM marketing_assets WHERE company_id=? AND status=? ORDER BY updated_at DESC');
    expect(marketingCompanyListSql('marketing_lead_candidates', '')).toBe(
      'SELECT * FROM marketing_lead_candidates WHERE company_id=? ORDER BY updated_at DESC');
    expect(marketingCompanyListSql('marketing_lead_candidates', " AND status='active'")).toBe(
      "SELECT * FROM marketing_lead_candidates WHERE company_id=? AND status='active' ORDER BY updated_at DESC");

    expect(marketingLocalPointReadSql('asset')).toBe(
      'SELECT * FROM marketing_assets WHERE id=? AND company_id=?');
    expect(marketingLocalPointReadSql('asset', true)).toBe(
      'SELECT * FROM marketing_assets WHERE id=? AND company_id=?');
    expect(marketingLocalPointReadSql('history')).toBe(
      'SELECT * FROM marketing_engagement_events WHERE candidate_id=? ORDER BY version');
    expect(marketingLocalPointReadSql('history', true)).toBe(
      'SELECT * FROM marketing_engagement_events WHERE candidate_id=? ORDER BY version');

    for (const [source, table] of [
      ['candidate', 'marketing_lead_candidates'],
      ['engagements', 'marketing_engagement_events'],
      ['costs', 'marketing_campaign_costs'],
      ['touches', 'marketing_touches'],
      ['campaigns', 'marketing_campaigns'],
    ] as const)
      expect(marketingPlainSourceSql(source)).toBe(`SELECT * FROM ${table}`);
  });

  it('preserves the candidate trailing space and FOR UPDATE tail bytes', () => {
    expect(marketingLocalPointReadSql('candidate', false)).toBe(
      'SELECT * FROM marketing_lead_candidates WHERE id=? AND company_id=? ');
    expect(marketingLocalPointReadSql('candidate', true)).toBe(
      'SELECT * FROM marketing_lead_candidates WHERE id=? AND company_id=? FOR UPDATE');
  });

  it('keeps one owner-local terminal head and no duplicate head literals', async () => {
    const head = await src('read-tail-sql.ts');
    expect(head).toContain('export const marketingReadTailSql');
    expect(head).toContain('`SELECT * FROM ${table}${tail}`');
    expect(head.match(/SELECT/g)).toHaveLength(1);
    expect(selectTokens(head)).toBe(1);
    expect(marketingReadTailSql('t')).toBe('SELECT * FROM t');
    expect(marketingReadTailSql('t', ' WHERE id=?')).toBe('SELECT * FROM t WHERE id=?');

    for (const name of [
      'marketing-company-list-sql.ts',
      'marketing-local-point-read-sql.ts',
      'marketing-plain-source-sql.ts',
    ]) {
      const body = await src(name);
      expect(body).not.toContain('SELECT * FROM');
      expect(body).toContain('marketingReadTailSql(');
      expect(selectTokens(body)).toBe(0);
    }
  });

  it('leaves native ports and pinned settled/floor statements untouched', async () => {
    expect(await src('native-marketing-read.ts')).toContain('export');
    expect(await src('record-purchases.ts')).toContain('`SELECT 1 FROM (${nativeIds}) actual WHERE id=?`');
    expect(await src('purchases.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(await src('form-asset-lock-sql.ts')).toContain('FOR SHARE');
    expect(await src('package-obligations.ts')).toContain('EXISTS(SELECT 1 FROM marketing_assets form');
  });

  it('pins the owner SELECT KPI to 24 (BEFORE=36)', async () => {
    expect(await ownerSelectTokens()).toBe(24);
    expect(await ownerSelectTokens()).toBeLessThan(32);
  });
});
