import { describe, it, expect } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import ts from 'typescript';
import { marketingAnalyticsRowsSql } from '../../../packages/server-marketing/src/adapters/analytics-null-scope-rows';
import {
  marketingEvidenceCandidateRows,
  marketingEvidenceEventRows,
} from '../../../packages/server-marketing/src/adapters/executive-evidence-rows';
import { marketingMetricBreakdownSql } from '../../../packages/server-marketing/src/adapters/executive-metric-breakdown-sql';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);
const src = (name: string) => readFile(adapter(name), 'utf8');

// Byte oracles frozen at HEAD 77bc573d300d2b870938de0a5d230464d6488d08 (task
// start). The three plain SELECT heads below were rendered by the original
// template literals before they were routed through marketingSelectHeadSql.
const HEAD = '77bc573d300d2b870938de0a5d230464d6488d08';

const enforced =
  'SELECT campaign.*,NULL::text branch_id,NULL::text department_id FROM marketing_campaigns campaign';
const legacy =
  'SELECT *,NULL::text branch_id,NULL::text department_id FROM marketing_campaigns';
const cost = (source: string) =>
  `SELECT cost.*,NULL::text branch_id,NULL::text department_id FROM ${source}`;

const candidateOracle =
  'SELECT c.id,c.contact_name,c.status,c.form_version,c.version,c.created_at,c.updated_at,c.branch_id,c.department_id,f.name form_name\n' +
  '      FROM (SRC) c JOIN marketing_assets f ON f.id=c.form_id AND f.company_id=c.company_id';
const eventOracle =
  'SELECT e.id,e.candidate_id,e.event_type,e.channel,e.version event_version,e.created_at,c.contact_name,c.status candidate_status,c.version candidate_version,c.branch_id,c.department_id,f.name form_name\n' +
  '      FROM marketing_engagement_events e JOIN (SRC) c ON c.id=e.candidate_id JOIN marketing_assets f ON f.id=c.form_id AND f.company_id=c.company_id';

const metricCandidatesOracle =
  'SELECT L label,count(*)::text value FROM (P) c WHERE S GROUP BY 1 ORDER BY 1';
const metricEngagementsOracle =
  'SELECT L label,count(*)::text value FROM marketing_engagement_events e JOIN (P) c ON c.id=e.candidate_id WHERE S AND e.created_at>=? AND e.created_at<? GROUP BY 1 ORDER BY 1';

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

describe('marketing read-tail r47 dedupe', () => {
  it(`renders the NULL-scope analytics head byte-identically from ${HEAD}`, () => {
    expect(marketingAnalyticsRowsSql('campaign.*', 'marketing_campaigns campaign')).toBe(enforced);
    expect(marketingAnalyticsRowsSql('*', 'marketing_campaigns')).toBe(legacy);
    expect(marketingAnalyticsRowsSql('cost.*', 'marketing_campaign_costs cost')).toBe(
      cost('marketing_campaign_costs cost'),
    );
    // The head carries no binds of its own; the caller's projection/source are
    // the only substituted fragments.
    expect(marketingAnalyticsRowsSql('campaign.*', 'marketing_campaigns campaign')).not.toContain('?');
  });

  it('renders both evidence row variants byte-identically, preserving the six-space FROM tail', () => {
    expect(marketingEvidenceCandidateRows('(SRC)')).toBe(candidateOracle);
    expect(marketingEvidenceEventRows('(SRC)')).toBe(eventOracle);
    const candidate = marketingEvidenceCandidateRows('(SRC)');
    const event = marketingEvidenceEventRows('(SRC)');
    expect(candidate).toContain('\n      FROM (SRC) c JOIN marketing_assets f');
    expect(event).toContain('\n      FROM marketing_engagement_events e JOIN (SRC) c ON c.id=e.candidate_id');
    expect(candidate.match(/\?/g)).toBeNull();
    expect(event.match(/\?/g)).toBeNull();
    expect(candidate).toContain('f.name form_name');
    expect(event).toContain('c.status candidate_status');
  });

  it('renders both metric breakdown variants byte-identically, keeping GROUP BY/ORDER and time binds', () => {
    expect(marketingMetricBreakdownSql('candidates', 'P', 'L', 'S')).toBe(metricCandidatesOracle);
    expect(marketingMetricBreakdownSql('engagements', 'P', 'L', 'S')).toBe(metricEngagementsOracle);
    const candidates = marketingMetricBreakdownSql('candidates', 'P', 'L', 'S');
    const engagements = marketingMetricBreakdownSql('engagements', 'P', 'L', 'S');
    expect(candidates).toContain('GROUP BY 1 ORDER BY 1');
    expect(engagements).toContain('GROUP BY 1 ORDER BY 1');
    expect(candidates.match(/\?/g)).toBeNull();
    expect((engagements.match(/\?/g) ?? [])).toHaveLength(2);
    expect(engagements).toContain('e.created_at>=? AND e.created_at<?');
  });

  it('keeps one terminal token in the shared renderer and none in the three folded callers', async () => {
    const head = await src('select-head-sql.ts');
    expect(head).toContain('export const marketingSelectHeadSql');
    expect(head).toContain('`SELECT ${head}${tail}`');
    expect(selectTokens(head)).toBe(1);

    for (const name of [
      'analytics-null-scope-rows.ts',
      'executive-evidence-rows.ts',
      'executive-metric-breakdown-sql.ts',
    ]) {
      const body = await src(name);
      expect(body).toContain('marketingSelectHeadSql(');
      expect(selectTokens(body)).toBe(0);
    }
  });

  it('leaves the protected r38/r40/r45 heads, settlement, candidate sources and locks untouched', async () => {
    expect((await src('form-asset-read-head.ts')).match(/SELECT /g)).toHaveLength(1);
    expect((await src('owner-point-read-sql.ts')).match(/SELECT /g)).toHaveLength(1);
    expect((await src('owner-projection-source.ts')).match(/SELECT /g)).toHaveLength(1);
    expect(await src('read-tail-sql.ts')).toContain('`SELECT * FROM ${table}${tail}`');
    expect((await src('read-tail-sql.ts')).match(/SELECT/g)).toHaveLength(1);

    expect(await src('record-purchases.ts')).toContain('`SELECT 1 FROM (${nativeIds}) actual WHERE id=?`');
    expect(await src('purchases.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(await src('package-obligations.ts')).toContain('EXISTS(SELECT 1 FROM marketing_assets form');
    expect(await src('native-marketing-read.ts')).toContain('export');
    expect(await src('marketing-candidate-source.ts')).toContain(
      "marketingPlainSourceSql('candidate')",
    );

    expect(await src('form-asset-lock-sql.ts')).toContain(
      '`SELECT id FROM marketing_assets WHERE id=? AND company_id=?${scope} FOR SHARE`',
    );
    expect(await src('marketing-company-list-sql.ts')).toContain('ORDER BY updated_at DESC');
    expect(await src('marketing-local-point-read-sql.ts')).toContain('FOR UPDATE');
  });

  it('pins the owner SELECT KPI to 29 (BEFORE=32)', async () => {
    expect(await ownerSelectTokens()).toBe(29);
    expect(await ownerSelectTokens()).toBeLessThan(32);
  });
});
