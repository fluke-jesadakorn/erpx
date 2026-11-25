import { describe, it, expect } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import ts from 'typescript';
import { commercialOwnerProjectionSources } from '@erpx/server-organization/public/commercial-owner-projection-sources';
import { marketingExecutiveLabelSql } from '../../../packages/server-marketing/src/adapters/executive-label-sql';
import { marketingSelectHeadSql } from '../../../packages/server-marketing/src/adapters/select-head-sql';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);
const src = (name: string) => readFile(adapter(name), 'utf8');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

// Byte oracles captured at HEAD 614418b37bd2aa6fdd9ea437956b9992bdd2054b,
// before r51 routed the branch/department executive label's byte-identical
// terminal head through the canonical owner-local head renderer.
const HEAD = '614418b37bd2aa6fdd9ea437956b9992bdd2054b';
const nodes = commercialOwnerProjectionSources.organization_nodes;

const labelOracle = (ref: string, fallback: string) =>
  `COALESCE((SELECT n.display_name FROM (${nodes}) n WHERE n.id=${ref} AND n.company_id=c.company_id),'${fallback}')`;

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
async function ownerHeadTokens(): Promise<{ spaced: number; bare: number }> {
  let spaced = 0, bare = 0;
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

describe('marketing read-tail r51 dedupe', () => {
  it(`renders the byte-identical branch/department label head from ${HEAD}`, () => {
    for (const [variant, ref, fallback, digest, len] of [
      ['branch', 'c.branch_id', 'Branch', '142749a99a578505885371cc8c5f037319e2169e9c6bd656551985a87fe5a0df', 236],
      ['department', 'c.department_id', 'Unassigned', '9359fd4738f3e93f90e54b01e09641e0e0826a1a302b7af4eb0a8f4252a23b6e', 244],
    ] as const) {
      const rendered = marketingExecutiveLabelSql(variant);
      expect(rendered, variant).toBe(labelOracle(ref, fallback));
      expect(sha(rendered), variant).toBe(digest);
      expect(rendered, variant).toHaveLength(len);
    }
  });

  it('routes the label terminal head through the canonical renderer', async () => {
    const renderer = await src('select-head-sql.ts');
    expect(renderer).toContain('`SELECT ${head}${tail}`');
    expect(headTokens(renderer)).toBe(1);
    expect(marketingSelectHeadSql('t')).toBe('SELECT t');
    expect(marketingSelectHeadSql('t', ' FROM s WHERE id=?')).toBe('SELECT t FROM s WHERE id=?');

    const label = await src('executive-label-sql.ts');
    expect(label).toContain("import { marketingSelectHeadSql } from './select-head-sql'");
    expect(label).toContain('`COALESCE((${marketingSelectHeadSql(head)}),\'${fallback}\')`');
    expect(label).not.toContain('COALESCE((SELECT');
    expect(headTokens(label)).toBe(0);
  });

  it('leaves the protected r38/r40/r44/r45 heads and the candidate-adjacent union untouched', async () => {
    for (const name of [
      'form-asset-read-head.ts',
      'owner-point-read-sql.ts',
      'marketing-point-exists-sql.ts',
    ])
      expect(headTokens(await src(name)), name).toBe(1);
    expect((await src('read-tail-sql.ts')).match(/SELECT/g)).toHaveLength(1);
    expect(headTokens(await src('marketing-purchase-union-sql.ts'))).toBe(0);
    expect(await src('crm-purchase-sources.ts')).toContain(
      'SELECT candidate.* FROM marketing_lead_candidates candidate',
    );
    expect(await src('marketing-source-id-sql.ts')).toContain('marketingPurchaseUnionSql(sourceIdTables');
  });

  it('leaves settlement, receipts, locks and native ports untouched', async () => {
    expect(await src('purchases.ts')).toContain('nativeMarketingReadFirst<Record<string,unknown>>(MARKETING_RD_COMMAND_RECEIPT_PRIOR');
    expect(await src('package-obligations.ts')).toContain("command.operation='marketing.asset.activate'");
    expect(await src('public-submission.ts')).toContain('readMarketingLane40ForCompany(LANE40_PUBLIC_RECEIPT,companyId,');
    expect(await src('public-form.ts')).toContain('FOR SHARE');
    expect(await src('marketing-local-point-read-sql.ts')).toContain('FOR UPDATE');
    expect(await src('native-marketing-read.ts')).toContain('export');
  });

  it('pins the owner head KPI to 9 (lane 40 retired 3) (BEFORE=23)', async () => {
    expect(await ownerHeadTokens()).toEqual({ spaced: 9, bare: 9 });
  });
});
