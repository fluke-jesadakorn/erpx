import { describe, it, expect } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import ts from 'typescript';
import { marketingSourceRowGuardSql, attachmentOwnerVersionGuardSql } from '../../../packages/server-marketing/src/adapters/attachment-owner-version-guard';
import { actorScopeAssignmentSql } from '../../../packages/server-marketing/src/adapters/marketing-actor-scope-guard';
import { attachmentReviewerGuardSql } from '../../../packages/server-marketing/src/adapters/attachment-reviewer-guard';
import { marketingSourceIdsSql } from '../../../packages/server-marketing/src/adapters/marketing-source-id-sql';
import { marketingActorScopeSql } from '../../../packages/server-marketing/src/adapters/marketing-actor-scope-sql';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);
const src = (name: string) => readFile(adapter(name), 'utf8');

// Oracles are frozen from base 4a403bb7b1b5c385d1115e92410273bee69cefff (HEAD
// at task start); no other worker touched these files before this packet.
const SCOPE = '(SELECT ?::text company_id,?::text branch_id,?::text department_id)';

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
type GuardOracle = {
  file: string;
  headOracle: string;   // exact HEAD source-row guard bytes
  call: string;         // exact replacement helper call in the working tree
  source: string;       // runtime source fragment (sentinel-substituted)
  tail: string;         // runtime where-tail fragment (sentinel-substituted)
  values: Record<string, string>;
};
const sub = (s: string, values: Record<string, string>) =>
  s.replace(/\$\{([A-Za-z0-9_]+)\}/g, (m, key: string) => (key in values ? values[key] : m));

const sourceRowGuards: GuardOracle[] = [
  {
    file: 'analytics-sources.ts',
    headOracle: 'EXISTS(SELECT 1 FROM (${crmMarketingCandidateSources}) candidate WHERE candidate.id=touch.source_id AND candidate.company_id=touch.company_id AND candidate.crm_lead_id=touch.lead_id)',
    call: "marketingSourceRowGuardSql('('+crmMarketingCandidateSources+') candidate','candidate.id=touch.source_id AND candidate.company_id=touch.company_id AND candidate.crm_lead_id=touch.lead_id')",
    values: { crmMarketingCandidateSources: '«CS»' },
    source: '(«CS») candidate',
    tail: 'candidate.id=touch.source_id AND candidate.company_id=touch.company_id AND candidate.crm_lead_id=touch.lead_id',
  },
  {
    file: 'analytics-sources.ts',
    headOracle: 'EXISTS(SELECT 1 FROM (${crmHotelReservationSources}) reservation WHERE reservation.id=touch.source_id AND reservation.company_id=touch.company_id AND reservation.branch_id=lead.branch_id)',
    call: "marketingSourceRowGuardSql('('+crmHotelReservationSources+') reservation','reservation.id=touch.source_id AND reservation.company_id=touch.company_id AND reservation.branch_id=lead.branch_id')",
    values: { crmHotelReservationSources: '«HR»' },
    source: '(«HR») reservation',
    tail: 'reservation.id=touch.source_id AND reservation.company_id=touch.company_id AND reservation.branch_id=lead.branch_id',
  },
  {
    file: 'analytics-sources.ts',
    headOracle: 'EXISTS(SELECT 1 FROM (${crmSharedHotelAnalyticsSources}) shared WHERE shared.company_id=marketing_report.company_id AND shared.lead_id=marketing_report.lead_id AND shared.reservation_id=marketing_report.source_id)',
    call: "marketingSourceRowGuardSql('('+crmSharedHotelAnalyticsSources+') shared','shared.company_id=marketing_report.company_id AND shared.lead_id=marketing_report.lead_id AND shared.reservation_id=marketing_report.source_id')",
    values: { crmSharedHotelAnalyticsSources: '«SH»' },
    source: '(«SH») shared',
    tail: 'shared.company_id=marketing_report.company_id AND shared.lead_id=marketing_report.lead_id AND shared.reservation_id=marketing_report.source_id',
  },
  {
    file: 'purchases.ts',
    headOracle: 'EXISTS(SELECT 1 FROM (${marketingPurchaseCandidateRows}) native_candidate WHERE native_candidate.id=${alias}.id AND native_candidate.company_id=${alias}.company_id)',
    call: "marketingSourceRowGuardSql('('+marketingPurchaseCandidateRows+') native_candidate','native_candidate.id='+alias+'.id AND native_candidate.company_id='+alias+'.company_id')",
    values: { marketingPurchaseCandidateRows: '«PR»', alias: 'c' },
    source: '(«PR») native_candidate',
    tail: 'native_candidate.id=c.id AND native_candidate.company_id=c.company_id',
  },
  {
    file: 'record-purchases.ts',
    headOracle: 'EXISTS(SELECT 1 FROM (${nativeIds}) native_marketing WHERE native_marketing.id=${id})',
    call: "marketingSourceRowGuardSql('('+nativeIds+') native_marketing','native_marketing.id='+id)",
    values: { nativeIds: '«NI»', id: "'abc'" },
    source: '(«NI») native_marketing',
    tail: "native_marketing.id='abc'",
  },
  {
    file: 'record-purchases.ts',
    headOracle: "EXISTS(SELECT 1 FROM marketing_lead_candidates native WHERE native.id=marketing_work.id AND native.company_id=marketing_work.company_id AND native.status='pending' AND native.created_by<>?)",
    call: "marketingSourceRowGuardSql('marketing_lead_candidates native',\"native.id=marketing_work.id AND native.company_id=marketing_work.company_id AND native.status='pending' AND native.created_by<>?\")",
    values: {},
    source: 'marketing_lead_candidates native',
    tail: "native.id=marketing_work.id AND native.company_id=marketing_work.company_id AND native.status='pending' AND native.created_by<>?",
  },
];

describe('marketing read-tail r43 dedupe', () => {
  it('renders every retired source-row guard byte-identically from HEAD', async () => {
    for (const g of sourceRowGuards) {
      const current = await src(g.file);
      expect(current).not.toContain(g.headOracle);            // inline retired
      expect(current).toContain(g.call);                      // exact composition
      expect(marketingSourceRowGuardSql(g.source, g.tail)).toBe(sub(g.headOracle, g.values));
    }
  });

  it('keeps one source-row guard head and preserves the version-guard wrapper', async () => {
    const guard = await src('attachment-owner-version-guard.ts');
    expect(selectTokens(guard)).toBe(0);
    expect(guard).toContain('`EXISTS(${marketingPointExistsSql(source, tail)})`');
    expect(marketingSourceRowGuardSql('t', 'p')).toBe('EXISTS(SELECT 1 FROM t WHERE p)');
    expect(attachmentOwnerVersionGuardSql('marketing_assets')).toBe(
      'EXISTS(SELECT 1 FROM marketing_assets WHERE id=? AND company_id=? AND version=?)',
    );
    expect(attachmentOwnerVersionGuardSql('marketing_assets', 'extra')).toBe(
      'EXISTS(SELECT 1 FROM marketing_assets WHERE id=? AND company_id=? AND version=? AND extra)',
    );
  });

  it('renders every retired actor-scope assignment guard byte-identically from HEAD', async () => {
    // purchases.ts:35
    const purchasesHead = 'SELECT ${assignmentReadSql(alias,permission)} allowed FROM ${marketingActorScopeSql} ${alias}';
    expect(await src('purchases.ts')).not.toContain(purchasesHead);
    expect(await src('purchases.ts')).toContain("actorScopeAssignmentSql(assignmentReadSql(alias,permission),alias,' allowed')");
    const arA = '«arA»';
    expect(actorScopeAssignmentSql(arA, 'marketing_scope', ' allowed')).toBe(
      purchasesHead
        .replace('${assignmentReadSql(alias,permission)}', arA)
        .replace('${marketingActorScopeSql}', SCOPE)
        .replace('${alias}', 'marketing_scope'),
    );

    // metric-purchases.ts:16
    const metricHead = 'SELECT ${assignmentReadSql("marketing_metric_scope","records.read")} AND EXISTS(${marketingModuleMemberSql()}) allowed FROM ${marketingActorScopeSql} marketing_metric_scope';
    expect(await src('metric-purchases.ts')).not.toContain(metricHead);
    expect(await src('metric-purchases.ts')).toContain("'marketing_metric_scope',' allowed'");
    const arB = '«arB»', mm = '«mm»';
    expect(actorScopeAssignmentSql(`${arB} AND EXISTS(${mm})`, 'marketing_metric_scope', ' allowed')).toBe(
      metricHead
        .replace('${assignmentReadSql("marketing_metric_scope","records.read")}', arB)
        .replace('${marketingModuleMemberSql()}', mm)
        .replace('${marketingActorScopeSql}', SCOPE),
    );

    // attachment-reviewer-guard.ts:9
    const reviewerHead = "SELECT (${assignmentReadSql('marketing_file_review','records.review')} OR ${assignmentReadSql('marketing_file_review','commerce.approve')})${alias ? ` ${alias}` : ''} FROM ${marketingActorScopeSql} marketing_file_review";
    const ar1 = '«ar1»', ar2 = '«ar2»';
    expect(actorScopeAssignmentSql(`(${ar1} OR ${ar2})`, 'marketing_file_review', ' allowed')).toBe(
      reviewerHead
        .replace("${assignmentReadSql('marketing_file_review','records.review')}", ar1)
        .replace("${assignmentReadSql('marketing_file_review','commerce.approve')}", ar2)
        .replace("${alias ? ` ${alias}` : ''}", ' allowed')
        .replace('${marketingActorScopeSql}', SCOPE),
    );
    expect(attachmentReviewerGuardSql('allowed')).toContain(` allowed FROM ${marketingActorScopeSql} marketing_file_review`);
    expect(attachmentReviewerGuardSql()).toContain(` FROM ${marketingActorScopeSql} marketing_file_review`);

    // attachment-purchases.ts:47
    const attachHead = 'SELECT ${assignmentReadSql("marketing_file_scope","records.write")} FROM ${marketingActorScopeSql} marketing_file_scope';
    expect(await src('attachment-purchases.ts')).not.toContain(attachHead);
    const arD = '«arD»';
    expect(`(${actorScopeAssignmentSql(arD, 'marketing_file_scope')})`).toBe(
      `(${attachHead
        .replace('${assignmentReadSql("marketing_file_scope","records.write")}', arD)
        .replace('${marketingActorScopeSql}', SCOPE)})`,
    );
  });

  it('routes the actor-scope head through the shared renderer and keeps no inline canonical scope token', async () => {
    expect(selectTokens(await src('marketing-actor-scope-guard.ts'))).toBe(0);
    expect(await src('marketing-actor-scope-guard.ts')).toContain('marketingSelectHeadSql(');
    expect(selectTokens(await src('marketing-actor-scope-sql.ts'))).toBe(0);
  });

  it('merges the source-id frame into the union builder without changing output', async () => {
    const expected =
      'SELECT id FROM marketing_assets UNION ALL SELECT id FROM marketing_lead_candidates UNION ALL SELECT id FROM marketing_campaigns UNION ALL SELECT id FROM marketing_campaign_costs';
    expect(marketingSourceIdsSql()).toBe(expected);
    expect(selectTokens(await src('marketing-source-id-sql.ts'))).toBe(0);
    expect(selectTokens(await src('marketing-purchase-union-sql.ts'))).toBe(1);
  });

  it('leaves FOR SHARE, command_receipts settlement, package settlement and native ports untouched', async () => {
    expect(await src('form-asset-lock-sql.ts')).toContain('FOR SHARE');
    expect(await src('purchases.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    const obligations = await src('package-obligations.ts');
    expect(obligations).toContain('candidates:`SELECT c.*,valid.id IS NOT NULL AND EXISTS(SELECT 1 FROM marketing_assets form');
    expect(obligations).toContain('SELECT c.*,valid.id IS NOT NULL');
    expect(await src('native-marketing-read.ts')).toContain('export');
  });

  it('pins the owner SELECT KPI to 23 (BEFORE=36)', async () => {
    expect(await ownerSelectTokens()).toBe(23);
    expect(await ownerSelectTokens()).toBeLessThan(32);
  });
});
