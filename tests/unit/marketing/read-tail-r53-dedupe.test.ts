import { describe, it, expect } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import ts from 'typescript';
import { marketingSelectHeadSql } from '../../../packages/server-marketing/src/adapters/select-head-sql';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);
const src = (name: string) => readFile(adapter(name), 'utf8');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

// Byte oracles captured at HEAD e302bb280444f06f8debd3c704c1e8e4a24f7ef9
// (marketing.ts source sha256 10a1d9ed438baa8015a0c45d4e28a2067eb14131a06c543ec1d864f4b5b1c8d5)
// before r53 routed the branches() organization-projection head through the
// canonical owner-local renderer. «NODES» stands for the imported node source.
const HEAD = 'e302bb280444f06f8debd3c704c1e8e4a24f7ef9';
const REPO_SHA = '10a1d9ed438baa8015a0c45d4e28a2067eb14131a06c543ec1d864f4b5b1c8d5';
const NODES = '«NODES»';

const variants = [
  [
    'branches',
    'id,display_name name',
    ` FROM (${NODES}) organization_nodes WHERE company_id=? AND kind IN ('branch','headquarters') AND status='active' AND (?::text IS NULL OR id=?) ORDER BY display_name`,
    `SELECT id,display_name name FROM (${NODES}) organization_nodes WHERE company_id=? AND kind IN ('branch','headquarters') AND status='active' AND (?::text IS NULL OR id=?) ORDER BY display_name`,
    'd54a5824cc703c0b935fc92ab84dbcb1af1458217ac3faa2c340e720304bee69',
    190,
    3,
  ],
  [
    'altProjection',
    'a.*,b.kind',
    ` FROM (${NODES}) organization_nodes WHERE company_id=? AND kind IN ('branch','headquarters') AND status='active' AND (?::text IS NULL OR id=?) ORDER BY display_name`,
    `SELECT a.*,b.kind FROM (${NODES}) organization_nodes WHERE company_id=? AND kind IN ('branch','headquarters') AND status='active' AND (?::text IS NULL OR id=?) ORDER BY display_name`,
    '6a39487150819a062684824d026b6118f8ef6884e9512117f163f0759238829a',
    180,
    3,
  ],
  [
    'altTail',
    'id,display_name name',
    ` FROM (${NODES}) organization_nodes WHERE company_id=? AND kind='branch' AND status='active' AND (?::text IS NULL OR id=?) ORDER BY id`,
    `SELECT id,display_name name FROM (${NODES}) organization_nodes WHERE company_id=? AND kind='branch' AND status='active' AND (?::text IS NULL OR id=?) ORDER BY id`,
    '18043cc89f186cb625f50c595862aa0271e2f9d7caa9f4b4e139b61442eb56b7',
    160,
    3,
  ],
] as const;

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

describe('marketing read-tail r53 dedupe', () => {
  it(`renders byte-identical branches heads for every admitted variant from ${HEAD}`, () => {
    for (const [label, head, tail, oracle, digest, len, binds] of variants) {
      const rendered = marketingSelectHeadSql(head, tail);
      expect(rendered, label).toBe(oracle);
      expect(sha(rendered), label).toBe(digest);
      expect(rendered, label).toHaveLength(len);
      expect(rendered.split('SELECT ').length - 1, label).toBe(1);
      expect(rendered.match(/\?/g) ?? [], label).toHaveLength(binds);
    }
  });

  it('routes the branches head through the canonical renderer', async () => {
    const renderer = await src('select-head-sql.ts');
    expect(renderer).toContain('`SELECT ${head}${tail}`');
    expect(headTokens(renderer)).toBe(1);
    expect(marketingSelectHeadSql('t')).toBe('SELECT t');
    expect(marketingSelectHeadSql('t', ' FROM s WHERE id=?')).toBe('SELECT t FROM s WHERE id=?');

    const repo = await src('marketing.ts');
    expect(repo).toContain("import {marketingSelectHeadSql} from './select-head-sql';");
    expect(repo).toContain("marketingSelectHeadSql('id,display_name name',");
    expect(repo).toContain("organization_nodes WHERE company_id=? AND kind IN ('branch','headquarters')");
    expect(repo).not.toContain('SELECT id,display_name name FROM (');
    expect(headTokens(repo)).toBe(0); // lane 40 moved the aggregate to native Rust
  });

  it('leaves the r38-r52 head families and the remaining repository read intact', async () => {
    for (const name of [
      'form-asset-read-head.ts',
      'owner-point-read-sql.ts',
      'owner-projection-source.ts',
      'marketing-point-exists-sql.ts',
    ])
      expect(headTokens(await src(name)), name).toBe(1);
    expect((await src('read-tail-sql.ts')).match(/SELECT/g)).toHaveLength(1);
    expect(headTokens(await src('marketing-purchase-union-sql.ts'))).toBe(1);
    expect(headTokens(await src('marketing-clocked-report-frame.ts'))).toBe(0);
    expect(headTokens(await src('executive-label-sql.ts'))).toBe(0);
    expect((await src('marketing.ts')).match(/SELECT COALESCE\(NULLIF\(BTRIM\(attribution_json/)).toBeNull();
    expect(await src('marketing.ts')).toContain('LANE40_CAMPAIGN_SUMMARY');
  });

  it('leaves settlement, receipts, candidate sources, locks, command_receipts and the purchase union untouched', async () => {
    expect(await src('purchases.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(await src('public-submission.ts')).toContain(
      'readMarketingLane40ForCompany(LANE40_PUBLIC_RECEIPT,companyId,',
    );
    expect(await src('form-asset-lock-sql.ts')).toContain('FOR SHARE');
    expect(await src('marketing-local-point-read-sql.ts')).toContain('FOR UPDATE');
    expect(await src('crm-purchase-sources.ts')).toContain(
      'SELECT candidate.* FROM marketing_lead_candidates candidate',
    );
    await expect(src('candidate-form-rows.ts')).rejects.toThrow();
    expect(await src('marketing.ts')).toContain('LANE40_CANDIDATE_LIST');
    expect(headTokens(await src('marketing-purchase-union-sql.ts'))).toBe(1);
  });

  it('pins the owner head KPI to 14 (BEFORE=21; lane 40 retired 3 TS SELECTs)', async () => {
    expect(await ownerHeadTokens()).toEqual({ spaced: 14, bare: 14 });
  });
});
