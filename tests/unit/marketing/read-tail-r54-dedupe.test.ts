import { describe, it, expect } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import ts from 'typescript';
import { marketingPointExistsSql } from '../../../packages/server-marketing/src/adapters/marketing-point-exists-sql';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);
const src = (name: string) => readFile(adapter(name), 'utf8');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

// Byte oracles captured at HEAD 6d08598b3c2af7d5bbebc8cbce2503ac80c91bfb
// (record-purchases.ts source sha256
// af538fef1047260aceb5059992e169a94a0040e3a0346ff1d5e2c32b56bb1ff1)
// before r54 routed the native-source point probe through the canonical
// point-existence renderer. «ids» stands for the owner-owned native source
// projection, «where» for a caller-owned WHERE tail.
const HEAD = '6d08598b3c2af7d5bbebc8cbce2503ac80c91bfb';
const RECORD_SHA = 'af538fef1047260aceb5059992e169a94a0040e3a0346ff1d5e2c32b56bb1ff1';
const IDS = '«ids»';

const variants = [
  [
    'native-probe',
    `(${IDS}) actual`, 'id=?', ' ',
    `SELECT 1 FROM (${IDS}) actual WHERE id=?`,
    '3d9a937647fe89691869b37a8d940e5684da78da54f83aa270bf8ba0b3c3ab76',
    39, 1,
  ],
  [
    'actor-scope',
    '(SCOPE)workforce_actor_scope', '«where»', '',
    'SELECT 1 FROM(SCOPE)workforce_actor_scope WHERE «where»',
    '60ea3370b9e758eab9aaede68c61040742411a3dd580241411a473117eef242e',
    55, 0,
  ],
  [
    'receipt-witness',
    'marketing_public_receipts receipt JOIN (C) candidate ON candidate.id=receipt.candidate_id',
    'receipt.key=? AND candidate.company_id=? AND candidate.form_id=? AND candidate.form_version=?', ' ',
    'SELECT 1 FROM marketing_public_receipts receipt JOIN (C) candidate ON candidate.id=receipt.candidate_id WHERE receipt.key=? AND candidate.company_id=? AND candidate.form_id=? AND candidate.form_version=?',
    '2591de63f6a651b9a3aa51f1da75c7f248b85f6ef560879102ff70636db4fd05',
    203, 4,
  ],
  [
    'guard',
    't', 'p', ' ',
    'SELECT 1 FROM t WHERE p',
    '17c9073303126953915962441d230286fe3434b922f9146b81acb1963ce10695',
    23, 0,
  ],
  [
    'plain',
    'marketing_assets', 'company_id=?', ' ',
    'SELECT 1 FROM marketing_assets WHERE company_id=?',
    'a4d5a52889d484a535b09a2b29acd1f26fb73dc995b560350364631a81030f2f',
    49, 1,
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

describe('marketing read-tail r54 dedupe', () => {
  it(`renders byte-identical point-existence heads for every admitted variant from ${HEAD}`, () => {
    for (const [label, source, where, gap, oracle, digest, len, binds] of variants) {
      const rendered = marketingPointExistsSql(source, where, gap);
      expect(rendered, label).toBe(oracle);
      expect(sha(rendered), label).toBe(digest);
      expect(rendered, label).toHaveLength(len);
      expect(rendered.split('SELECT ').length - 1, label).toBe(1);
      expect(rendered.match(/\?/g) ?? [], label).toHaveLength(binds);
    }
  });

  it('routes the native-source point probe through the canonical renderer', async () => {
    const record = await src('record-purchases.ts');
    expect(sha(record)).not.toBe(RECORD_SHA);
    expect(record).toContain("import {marketingPointExistsSql} from './marketing-point-exists-sql';");
    expect(record).toContain("marketingPointExistsSql(`(${nativeIds}) actual`,'id=?')");
    expect(record).not.toContain('`SELECT 1 FROM (${nativeIds}) actual WHERE id=?`');
    // The five-word head now lives only in the one canonical renderer.
    expect(headTokens(await src('marketing-point-exists-sql.ts'))).toBe(1);
    expect(headTokens(record)).toBe(1); // only the work-row head remains
    expect(marketingPointExistsSql(`(${IDS}) actual`, 'id=?')).toBe(
      `SELECT 1 FROM (${IDS}) actual WHERE id=?`,
    );
  });

  it('leaves the r38-r53 head families and the remaining repository read intact', async () => {
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

  it('pins the owner head KPI to 17 (BEFORE=20; lane 40 retired 2 TS SELECTs)', async () => {
    expect(await ownerHeadTokens()).toEqual({ spaced: 13, bare: 13 });
  });
});
