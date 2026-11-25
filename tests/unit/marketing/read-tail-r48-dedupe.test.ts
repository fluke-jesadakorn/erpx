import { describe, it, expect } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import ts from 'typescript';
import { crmMarketingCandidateSources } from '../../../packages/server-marketing/src/adapters/crm-purchase-sources';
import { marketingPurchaseRowsSql } from '../../../packages/server-marketing/src/adapters/marketing-purchase-rows-sql';
import { marketingSelectHeadSql } from '../../../packages/server-marketing/src/adapters/select-head-sql';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);
const src = (name: string) => readFile(adapter(name), 'utf8');

// Byte oracles frozen at HEAD 9e26b1a5f659765f1e822aad018b400d15b0cbfa (task
// start). The four witness-row projections and the source() read were rendered
// by the original template literals before r48 routed their heads through
// marketingSelectHeadSql.
const HEAD = '9e26b1a5f659765f1e822aad018b400d15b0cbfa';

const expectedRows = {
  asset:
    "SELECT asset.id,asset.company_id,NULL::text branch_id,NULL::text department_id,(to_jsonb(asset)-'status'-'reviewed_by'-'version'-'updated_at') witness FROM marketing_assets asset",
  candidate: `SELECT candidate.id,candidate.company_id,candidate.branch_id,candidate.department_id,(to_jsonb(candidate)-'status'-'crm_lead_id'-'reviewed_by'-'version'-'updated_at') witness FROM (${crmMarketingCandidateSources}) candidate`,
  campaign:
    'SELECT campaign.id,campaign.company_id,NULL::text branch_id,NULL::text department_id,to_jsonb(campaign) witness FROM marketing_campaigns campaign',
  'campaign-cost':
    'SELECT cost.id,cost.company_id,NULL::text branch_id,NULL::text department_id,to_jsonb(cost) witness FROM marketing_campaign_costs cost JOIN marketing_campaigns campaign ON campaign.id=cost.campaign_id AND campaign.company_id=cost.company_id',
} as const;

const sourceOracle = (row: string) =>
  `SELECT id,company_id,branch_id,department_id,witness::text witness FROM (${row}) actual WHERE id=? AND company_id=? AND (?::text IS NULL OR branch_id IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id IS NULL OR department_id=?)`;

const bindsOracle = [
  'target.id',
  'actor.companyId',
  'actor.branchId',
  'actor.branchId',
  'actor.departmentId',
  'actor.departmentId',
] as const;

/** AST evidence: count SELECT tokens only inside string/template text nodes. */
function selectTokens(text: string): number {
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

/** Locate source() and read its database.prepare() argument plus .bind() args. */
function sourceStatement(text: string): { arg: string; binds: string[] } {
  const sf = ts.createSourceFile('purchases.ts', text, ts.ScriptTarget.Latest, true);
  let fn: ts.FunctionDeclaration | undefined;
  const findFn = (node: ts.Node): void => {
    if (ts.isFunctionDeclaration(node) && node.name?.getText(sf) === 'source') fn = node;
    else ts.forEachChild(node, findFn);
  };
  findFn(sf);
  if (!fn) throw new Error('source() not found');
  let out: { arg: string; binds: string[] } | undefined;
  const visit = (node: ts.Node): void => {
    if (ts.isCallExpression(node) && node.expression.getText(sf) === 'database.prepare') {
      const arg = node.arguments[0]!.getText(sf);
      const parent = node.parent;
      let binds: string[] = [];
      if (
        ts.isPropertyAccessExpression(parent) &&
        parent.name.getText(sf) === 'bind' &&
        ts.isCallExpression(parent.parent)
      )
        binds = parent.parent.arguments.map((a) => a.getText(sf));
      out = { arg, binds };
    }
    ts.forEachChild(node, visit);
  };
  visit(fn);
  if (!out) throw new Error('database.prepare() inside source() not found');
  return out;
}

describe('marketing read-tail r48 dedupe', () => {
  it(`renders all four purchase witness projections byte-identically from ${HEAD}`, () => {
    for (const [variant, sql] of Object.entries(expectedRows))
      expect(marketingPurchaseRowsSql(variant as keyof typeof expectedRows)).toBe(sql);
    expect(expectedRows.candidate).toContain(crmMarketingCandidateSources);
  });

  it('renders source() byte-identically and keeps the six binds in order', async () => {
    const statement = sourceStatement(await src('purchases.ts'));
    expect(statement.arg.startsWith('marketingSelectHeadSql(')).toBe(true);

    // Evaluate the extracted call the way TypeScript renders it: real helper,
    // one substituted row frame, one target kind.
    const rendered = new Function(
      'marketingSelectHeadSql',
      'rows',
      'target',
      `return ${statement.arg};`,
    )(marketingSelectHeadSql, { x: 'ROW' }, { kind: 'x' }) as string;
    expect(rendered).toBe(sourceOracle('ROW'));
    expect(rendered).toContain('FROM (ROW) actual WHERE id=? AND company_id=?');
    expect(rendered.match(/\?/g)).toHaveLength(6);

    // Bind order and text are untouched by the fold.
    expect(statement.binds).toEqual([...bindsOracle]);
    expect(statement.binds).toHaveLength(6);
  });

  it('keeps one terminal token in the shared renderer and none in the folded projection head', async () => {
    const helper = await src('select-head-sql.ts');
    expect(helper).toContain('export const marketingSelectHeadSql');
    expect(helper).toContain('`SELECT ${head}${tail}`');
    expect(selectTokens(helper)).toBe(1);

    const rows = await src('marketing-purchase-rows-sql.ts');
    expect(rows).toContain('marketingSelectHeadSql(head, ` FROM ${spec.source}`)');
    expect(selectTokens(rows)).toBe(0);

    // purchases.ts retains exactly one token: the protected command_receipts guard.
    const purchases = await src('purchases.ts');
    expect(purchases).toContain("marketingSelectHeadSql('id,company_id,branch_id,department_id,witness::text witness'");
    expect(selectTokens(purchases)).toBe(1);
  });

  it('leaves the protected r38/r40/r45/r46/r47 heads and tails untouched', async () => {
    expect((await src('form-asset-read-head.ts')).match(/SELECT /g)).toHaveLength(1);
    expect((await src('owner-point-read-sql.ts')).match(/SELECT /g)).toHaveLength(1);
    expect((await src('owner-projection-source.ts')).match(/SELECT /g)).toHaveLength(1);
    expect(await src('read-tail-sql.ts')).toContain('`SELECT * FROM ${table}${tail}`');
    expect((await src('read-tail-sql.ts')).match(/SELECT/g)).toHaveLength(1);

    expect(await src('purchases.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(await src('record-purchases.ts')).toContain('`SELECT 1 FROM (${nativeIds}) actual WHERE id=?`');
    expect(await src('package-obligations.ts')).toContain('EXISTS(SELECT 1 FROM marketing_assets form');
    expect(await src('native-marketing-read.ts')).toContain('export');
    expect(await src('form-asset-lock-sql.ts')).toContain(
      '`SELECT id FROM marketing_assets WHERE id=? AND company_id=?${scope} FOR SHARE`',
    );
    expect(await src('marketing-company-list-sql.ts')).toContain('ORDER BY updated_at DESC');
    expect(await src('marketing-local-point-read-sql.ts')).toContain('FOR UPDATE');
  });

  it('pins the owner SELECT KPI to 23 (BEFORE=29)', async () => {
    expect(await ownerSelectTokens()).toBe(23);
    expect(await ownerSelectTokens()).toBeLessThan(29);
  });
});
