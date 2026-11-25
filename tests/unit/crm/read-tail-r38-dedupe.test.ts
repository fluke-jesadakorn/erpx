import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { crmSalesOpportunitySql } from '../../../packages/server-crm/src/adapters/read-tail-r37-sql';
import { crmOwnedSourceRowSql } from '../../../packages/server-crm/src/adapters/read-tail-r34-sql';
import { crmCurrentScopeAllowedSql } from '../../../packages/server-crm/src/adapters/read-tail-r30-sql';

const owner = resolve(__dirname, '../../../packages/server-crm/src');
const src = (name: string) => readFileSync(resolve(owner, 'adapters', name), 'utf8');
const totalSelects = () => {
  let total = 0;
  for (const rel of readdirSync(owner, { recursive: true })) {
    if (!String(rel).endsWith('.ts')) continue;
    total += readFileSync(resolve(owner, String(rel)), 'utf8').match(/SELECT /g)?.length ?? 0;
  }
  return total;
};

const ASSIGN = 'ASSIGN_EXPR';
const SCOPE = '(SELECT ?::text company_id,?::text branch_id,?::text department_id)';
const ACTIVITY_FROM = 'crm_lead_activities c JOIN crm_leads l ON l.id=c.lead_id AND l.company_id=c.company_id';

/** Retired inline SQL, exactly as it read before r38 (interpolations replaced by
 * the concrete fragments the call sites pass). */
const retired: Array<[string, string, string]> = [
  ['lead-activity existence probe',
    crmSalesOpportunitySql('1', ACTIVITY_FROM, 'c.id='),
    `SELECT 1 FROM ${ACTIVITY_FROM} WHERE c.id=`],
  ['opportunity existence probe (trailing-space WHERE)',
    crmSalesOpportunitySql('1', '(OPPORTUNITY_ROWS) opportunity', ''),
    'SELECT 1 FROM (OPPORTUNITY_ROWS) opportunity WHERE '],
  ['opportunity lead probe',
    crmSalesOpportunitySql('1', 'crm_opportunities opportunity', 'opportunity.lead_id='),
    'SELECT 1 FROM crm_opportunities opportunity WHERE opportunity.lead_id='],
  ['source existence probe',
    crmOwnedSourceRowSql('1', 'CAMPAIGN_SOURCES', 'campaign'),
    'SELECT 1 FROM (CAMPAIGN_SOURCES) campaign WHERE id=? AND company_id=?'],
  ['marketing candidate rows',
    crmOwnedSourceRowSql('*', 'CANDIDATE_SOURCES', 'candidate'),
    'SELECT * FROM (CANDIDATE_SOURCES) candidate WHERE id=? AND company_id=?'],
  ['assignment-authorization read',
    crmCurrentScopeAllowedSql(ASSIGN, SCOPE, 'crm_scope'),
    `SELECT ${ASSIGN} allowed FROM ${SCOPE} crm_scope`],
];

describe('crm read-tail r38 byte-identical head composition', () => {
  test.each(retired)('%s renders byte-identical read-only SQL', (_name, rendered, expected) => {
    expect(rendered).toBe(expected);
    expect(rendered).toContain('SELECT ');
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('reuses the r37/r34/r30 renderers once each and composes them at the six retired sites', () => {
    for (const [file, name, count] of [
      ['read-tail-r37-sql.ts', 'crmSalesOpportunitySql', 1],
      ['read-tail-r34-sql.ts', 'crmOwnedSourceRowSql', 1],
      ['read-tail-r30-sql.ts', 'crmCurrentScopeAllowedSql', 1],
    ] as const) {
      expect(src(file).match(new RegExp(`${name} =`, 'g'))).toHaveLength(count);
    }
    expect((src('read-tail-r37-sql.ts').match(/SELECT /g) ?? []).length).toBe(1);
    expect((src('read-tail-r34-sql.ts').match(/SELECT /g) ?? []).length).toBe(0);
    expect((src('read-tail-r30-sql.ts').match(/SELECT /g) ?? []).length).toBe(1);

    const p = src('purchases.ts');
    expect(p.match(/crmSalesOpportunitySql\(/g)).toHaveLength(3);
    expect(p.match(/crmOwnedSourceRowSql\(/g)).toHaveLength(5);
    expect(p.match(/crmCurrentScopeAllowedSql\(/g)).toHaveLength(1);
    expect(p).toContain("crmAssignmentAllowedSql=(alias:string,permission:string)=>crmCurrentScopeAllowedSql(assignmentReadSql(alias,permission),actorScopeSql,alias);");
    expect(p).toContain("crmSourceExistsSql=(source:string,alias:string,tail='')=>crmOwnedSourceRowSql('1',source,alias,tail);");
    expect(p).toContain("crmMarketingCandidateRows=crmOwnedSourceRowSql('*',crmMarketingCandidateSources,'candidate');");
    expect(p).toContain("crmLeadActivityExists=crmSalesOpportunitySql('1','crm_lead_activities c JOIN crm_leads l ON l.id=c.lead_id AND l.company_id=c.company_id','c.id=');");
    expect(p).toContain("crmOpportunityExistsSql=crmSalesOpportunitySql('1',`(${crmPurchaseOpportunityRows}) opportunity`,'');");
    expect(p).toContain("crmOpportunityLeadSql=crmSalesOpportunitySql('1','crm_opportunities opportunity','opportunity.lead_id=');");

    expect(p).not.toContain('`SELECT ${assignmentReadSql(alias,permission)} allowed FROM ${actorScopeSql} ${alias}`');
    expect(p).not.toContain('`SELECT 1 FROM (${source}) ${alias} WHERE id=? AND company_id=?${tail}`');
    expect(p).not.toContain('`SELECT * FROM (${crmMarketingCandidateSources}) candidate WHERE id=? AND company_id=?`');
    expect(p).not.toContain('`SELECT 1 FROM crm_lead_activities c JOIN crm_leads l');
    expect(p).not.toContain('`SELECT 1 FROM (${crmPurchaseOpportunityRows}) opportunity WHERE `');
    expect(p).not.toContain('`SELECT 1 FROM crm_opportunities opportunity WHERE opportunity.lead_id=`');
  });

  test('keeps bind order and every caller tail unchanged', () => {
    const p = src('purchases.ts');
    expect(p).toContain("crmAssignmentAllowedSql('crm_scope',permission)).bind(actor.assignmentId,actor.userId,actor.role,scope.company_id,scope.branch_id,scope.department_id)");
    expect(p).toContain("crmAssignmentAllowedSql('crm_reference',permission)).bind(actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId)");
    expect(p).toContain("crmSourceExistsSql(crmMarketingCampaignSources,'campaign')).bind(input.campaignId,actor.companyId)");
    expect(p).toContain("crmSourceExistsSql(crmMarketingCandidateSources,'candidate',' AND branch_id=? AND (?::text IS NULL OR department_id=?)')).bind(input.sourceId??\"\",actor.companyId,input.branchId,actor.departmentId,actor.departmentId)");
    expect(p).toContain('${crmMarketingCandidateRows} AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?)');
    expect(p).toContain('${crmLeadActivityExists}? AND c.company_id=? AND ${salesRecipientEligibility(\'l\',\'c.owner_id\')}');
    expect(p).toContain('`${crmOpportunityExistsSql}id=? AND company_id=?`');
    expect(p).toContain("crmOpportunityLeadSql=crmSalesOpportunitySql('1','crm_opportunities opportunity','opportunity.lead_id=')");
  });

  test('leaves locks, command receipts, native kinds and r8-r37 helpers untouched', () => {
    const p = src('purchases.ts');
    expect(p).toContain('LOCK TABLE crm_accounts,crm_contacts,crm_guest_links,crm_guest_sharing_events,crm_leads,crm_opportunities,crm_case_events,crm_lead_activities,crm_lead_activity_events IN SHARE ROW EXCLUSIVE MODE');
    expect(p).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    for (const [file, token] of [
      ['read-tail-r24-sql.ts', 'export const crmCreatorLeadScopeSql'],
      ['read-tail-r25-sql.ts', 'export const crmPurchaseRowProjection'],
      ['read-tail-r26-sql.ts', 'export const crmGuestLinkReservationSourceSql'],
      ['read-tail-r27-sql.ts', 'const crmOpportunityOwnerAnchor ='],
      ['read-tail-r28-sql.ts', 'export const crmCaseEventReadSql'],
      ['read-tail-r29-sql.ts', 'const crmAccountReadAnchor ='],
      ['read-tail-r30-sql.ts', 'export const crmCurrentScopeAllowedSql'],
      ['read-tail-r31-sql.ts', 'export const crmRevenueJournalHeadSql'],
      ['read-tail-r32-sql.ts', 'export const crmCommerceOrderReadSql'],
      ['read-tail-r33-sql.ts', 'export const crmClockedSourceScopeSql'],
      ['read-tail-r34-sql.ts', 'export const crmOwnedSourceRowSql'],
      ['read-tail-r35-sql.ts', 'export const crmContactAccountReadSql'],
      ['read-tail-r36-sql.ts', 'export const crmOpportunitySourceAnchorSql'],
      ['read-tail-r37-sql.ts', 'export const crmSalesOpportunitySql'],
    ] as const) expect(src(file)).toContain(token);
  });

  test('retirement:ts-statements stays strictly below the r37 AFTER=110 baseline', () => {
    expect(totalSelects()).toBe(40);
    expect(totalSelects()).toBeLessThan(110);
  });
});
