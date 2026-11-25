import { describe, it, expect } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import ts from 'typescript';
import { marketingClockedReportFrameSql } from '../../../packages/server-marketing/src/adapters/marketing-clocked-report-frame';
import { marketingSelectHeadSql } from '../../../packages/server-marketing/src/adapters/select-head-sql';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);
const src = (name: string) => readFile(adapter(name), 'utf8');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

// Byte oracles captured at HEAD 68d61c5f76caeb940f54f730e11b7c2ca0a3d903
// (frame source sha256 465fceeb81f21e261ba1dd1eb2419fd94623dc7b0aed9ae4cc899d985bcdae7d)
// before r52 routed the frame's SELECT head through marketingSelectHeadSql.
const HEAD = '68d61c5f76caeb940f54f730e11b7c2ca0a3d903';
const FRAME_SHA = '465fceeb81f21e261ba1dd1eb2419fd94623dc7b0aed9ae4cc899d985bcdae7d';

const variants = [
  [
    'report',
    ['marketing_report_clock', '', 'marketing_report.*', 'SOURCE', 'marketing_report'],
    'WITH marketing_report_clock AS MATERIALIZED(SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms) SELECT marketing_report.* FROM (SOURCE) marketing_report CROSS JOIN marketing_report_clock WHERE ',
    'c2a22b20fabc6eea51bfdf73569d24550e02c93a438b56fd0f0b5fe4ceb319e2',
    214,
  ],
  [
    'metric',
    ['marketing_metric_clock', ' ', 'marketing_metric_owned.*', 'SOURCE', 'marketing_metric_owned'],
    'WITH marketing_metric_clock AS MATERIALIZED (SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms) SELECT marketing_metric_owned.* FROM (SOURCE) marketing_metric_owned CROSS JOIN marketing_metric_clock WHERE ',
    'a22d71e10e95663b0fd37d786cfb018dcf381b9633a6fdd260a8a9e8d72b524e',
    227,
  ],
  [
    'scope',
    ['marketing_clock', '', '1', 'ROWS', 'marketing_source'],
    'WITH marketing_clock AS MATERIALIZED(SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms) SELECT 1 FROM (ROWS) marketing_source CROSS JOIN marketing_clock WHERE ',
    '05b98d6ef5e6fa35b628741cdcc304052446b0065aa5edec801c6f3e2166ff4d',
    181,
  ],
  [
    'altGap1',
    ['ms', ' ', 'a.*', 'T', 'z'],
    'WITH ms AS MATERIALIZED (SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms) SELECT a.* FROM (T) z CROSS JOIN ms WHERE ',
    '15fd0b026ad32d36cce8367e1a21595959842171fa790aaae73135c6ce45f0b4',
    140,
  ],
  [
    'altGap0',
    ['m0', '', 'b.c', 'U JOIN V', 'w'],
    'WITH m0 AS MATERIALIZED(SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms) SELECT b.c FROM (U JOIN V) w CROSS JOIN m0 WHERE ',
    'c11314ecd528230a27e0a7e2eee27d8f649ac1d0b6cbd932bf5b3c528ddca26e',
    146,
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

describe('marketing read-tail r52 dedupe', () => {
  it(`renders byte-identical frames for every admitted variant from ${HEAD}`, () => {
    for (const [label, args, oracle, digest, len] of variants) {
      const rendered = marketingClockedReportFrameSql(
        args[0] as string,
        args[1] as '' | ' ',
        args[2] as string,
        args[3] as string,
        args[4] as string,
      );
      expect(rendered, label).toBe(oracle);
      expect(sha(rendered), label).toBe(digest);
      expect(rendered, label).toHaveLength(len);
      expect(rendered.split('SELECT ').length - 1, label).toBe(2); // clock body + frame head
      expect(rendered.match(/\?/g), label).toBeNull(); // binds stay at call sites
    }
  });

  it('routes the frame head through the canonical renderer', async () => {
    const renderer = await src('select-head-sql.ts');
    expect(renderer).toContain('`SELECT ${head}${tail}`');
    expect(headTokens(renderer)).toBe(1);
    expect(marketingSelectHeadSql('t')).toBe('SELECT t');
    expect(marketingSelectHeadSql('t', ' FROM s WHERE id=?')).toBe('SELECT t FROM s WHERE id=?');

    const frame = await src('marketing-clocked-report-frame.ts');
    expect(sha(frame)).not.toBe(FRAME_SHA);
    expect(frame).toContain("import {marketingSelectHeadSql} from './select-head-sql'");
    expect(frame).toContain('marketingSelectHeadSql(projection, ` FROM (${source}) ${alias} CROSS JOIN ${clock} WHERE `)');
    expect(frame).not.toContain('SELECT ${projection}');
    expect(headTokens(frame)).toBe(0);
    expect(frame.match(/SELECT/g)).toBeNull();
  });

  it('leaves the three callers wired to the frame with their exact tails', async () => {
    const analytics = await src('analytics-sources.ts');
    const metric = await src('metric-purchases.ts');
    const record = await src('record-purchases.ts');
    expect(analytics).toContain(
      "marketingClockedReportFrameSql('marketing_report_clock','', 'marketing_report.*', definitions[kind], 'marketing_report')",
    );
    expect(metric).toContain(
      "marketingClockedReportFrameSql('marketing_metric_clock',' ', 'marketing_metric_owned.*', crmMarketingCandidateSources, 'marketing_metric_owned')",
    );
    expect(record).toContain(
      "EXISTS(${marketingClockedReportFrameSql('marketing_clock','', '1', rows, 'marketing_source')}marketing_source.id=${id}",
    );
    expect(analytics).toContain("'pms.reservations','history','marketing_report_clock.now_ms'");
    expect(metric).toContain('"marketing_metric_clock.now_ms"');
    expect(record).toContain("'marketing_clock.now_ms'");
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
    expect(headTokens(await src('marketing-purchase-union-sql.ts'))).toBe(1);
  });

  it('pins the owner head KPI to 14 (lane 40 retired 3) (BEFORE=22)', async () => {
    expect(await ownerHeadTokens()).toEqual({ spaced: 14, bare: 14 });
  });
});
