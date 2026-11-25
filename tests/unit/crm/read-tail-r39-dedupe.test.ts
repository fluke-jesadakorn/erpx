import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { crmSingleKeySourceSql } from '../../../packages/server-crm/src/adapters/read-tail-r37-sql';
import { crmCampaignOptionsSql } from '../../../packages/server-crm/src/adapters/read-tail-r27-sql';
import { crmSelectBodySql } from '../../../packages/server-crm/src/adapters/read-tail-r46-sql';
import { crmClockedReadSql } from '../../../packages/server-crm/src/adapters/read-tail-r39-sql';

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

const CLOCK = 'report_clock AS MATERIALIZED(SELECT package_clock now_ms)';
const PARTIES = 'FINANCE_PARTIES';
const NODES = 'ORG_NODES';

/** Retired inline SQL, exactly as it read before r39 (interpolations replaced by
 * the concrete fragments the call sites pass). */
const retired: Array<[string, string, string]> = [
  ['active customer parties',
    crmSingleKeySourceSql('id,name', PARTIES, 'finance_parties', " AND kind='customer' AND status='active' ORDER BY name"),
    `SELECT id,name FROM (${PARTIES}) finance_parties WHERE company_id=? AND kind='customer' AND status='active' ORDER BY name`],
  ['campaign options (code aliased to name)',
    crmCampaignOptionsSql('CAMPAIGN_SOURCES', 'campaign', true),
    'SELECT id,code name FROM (CAMPAIGN_SOURCES) campaign WHERE company_id=? ORDER BY code'],
  ['campaign options (name column)',
    crmCampaignOptionsSql('CAMPAIGN_SOURCES', 'campaign'),
    'SELECT id,code,name FROM (CAMPAIGN_SOURCES) campaign WHERE company_id=? ORDER BY code'],
  ['branch options (active only)',
    crmSingleKeySourceSql('id,display_name name', NODES, 'organization_nodes',
      " AND kind IN ('branch','headquarters') AND status='active' AND (?::text IS NULL OR id=?) ORDER BY display_name"),
    `SELECT id,display_name name FROM (${NODES}) organization_nodes WHERE company_id=? AND kind IN ('branch','headquarters') AND status='active' AND (?::text IS NULL OR id=?) ORDER BY display_name`],
  ['branch options (all statuses)',
    crmSingleKeySourceSql('id,display_name', NODES, 'organization_nodes',
      " AND kind IN ('branch','headquarters') AND (?::text IS NULL OR id=?) ORDER BY display_name"),
    `SELECT id,display_name FROM (${NODES}) organization_nodes WHERE company_id=? AND kind IN ('branch','headquarters') AND (?::text IS NULL OR id=?) ORDER BY display_name`],
  ['clocked property options',
    crmClockedReadSql(CLOCK,
      'property.id,property.name FROM pms_properties property CROSS JOIN report_clock WHERE property.company_id=? AND (?::text IS NULL OR property.branch_id=?) ORDER BY property.name'),
    `WITH ${CLOCK} SELECT property.id,property.name FROM pms_properties property CROSS JOIN report_clock WHERE property.company_id=? AND (?::text IS NULL OR property.branch_id=?) ORDER BY property.name`],
  ['clocked costs',
    crmClockedReadSql(CLOCK,
      "cost.campaign_id,COALESCE(SUM(line.debit_minor-line.credit_minor),0) amount FROM (COST_SOURCES) cost GROUP BY cost.campaign_id"),
    `WITH ${CLOCK} SELECT cost.campaign_id,COALESCE(SUM(line.debit_minor-line.credit_minor),0) amount FROM (COST_SOURCES) cost GROUP BY cost.campaign_id`],
];

describe('crm read-tail r39 byte-identical head composition', () => {
  test.each(retired)('%s renders byte-identical read-only SQL', (_name, rendered, expected) => {
    expect(rendered).toBe(expected);
    expect(rendered).toContain('SELECT ');
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('clocked head preserves the multiline body whitespace verbatim', () => {
    const body = `l.id,l.account_id,
     CASE WHEN v.win_source='pms' THEN r.id IS NOT NULL ELSE TRUE END source_visible,
     FROM crm_leads l CROSS JOIN report_clock`;
    expect(crmClockedReadSql(CLOCK, body)).toBe(`WITH ${CLOCK} SELECT l.id,l.account_id,
     CASE WHEN v.win_source='pms' THEN r.id IS NOT NULL ELSE TRUE END source_visible,
     FROM crm_leads l CROSS JOIN report_clock`);
  });

  test('owns each renderer once and composes it at the six retired sites', () => {
    for (const [file, name, count] of [
      ['read-tail-r37-sql.ts', 'crmSingleKeySourceSql', 1],
      ['read-tail-r39-sql.ts', 'crmClockedReadSql', 1],
    ] as const) {
      expect(src(file).match(new RegExp(`${name} =`, 'g'))).toHaveLength(count);
    }
    expect((src('read-tail-r37-sql.ts').match(/SELECT /g) ?? []).length).toBe(0);
    expect((src('read-tail-r39-sql.ts').match(/SELECT /g) ?? []).length).toBe(0);
    expect((src('read-tail-r27-sql.ts').match(/SELECT /g) ?? []).length).toBe(0);

    const parties = src('active-customer-parties.ts');
    expect(parties.match(/crmSingleKeySourceSql\(/g)).toHaveLength(1);
    expect(parties).toContain('export const activeCustomerPartiesSql = crmSingleKeySourceSql(');
    expect(parties).toContain("crmSingleKeySourceSql('id,name',financeProjectionSources.finance_parties,'finance_parties',\" AND kind='customer' AND status='active' ORDER BY name\")");
    expect(parties).not.toContain('`SELECT id,name FROM (');

    const r27 = src('read-tail-r27-sql.ts');
    expect(r27.match(/crmSingleKeySourceSql\(/g)).toHaveLength(1);
    expect(r27).toContain("crmCampaignOptionsSql = (source: string, alias: string, aliasCode = false) =>");
    expect(r27).toContain("crmSingleKeySourceSql(`id,code${aliasCode ? ' name' : ',name'}`,source,alias,' ORDER BY code')");
    expect(r27).not.toContain('`SELECT id,code');

    const p = src('purchases.ts');
    expect(p.match(/crmSingleKeySourceSql\(/g)).toHaveLength(1);
    expect(p).toContain("crmBranchOptionsSql=(nameAlias:string,activeOnly:boolean)=>crmSingleKeySourceSql(`id,display_name${nameAlias}`,organizationProjectionSources.organization_nodes,'organization_nodes',");
    expect(p).not.toContain('`SELECT id,display_name${nameAlias} FROM (');

    const a = src('analytics.ts');
    expect(a.match(/crmClockedReadSql\(/g)).toHaveLength(3);
    expect(a).toContain("import {crmClockedReadSql} from './read-tail-r39-sql';");
    expect(a).not.toContain('`WITH ${clock} SELECT ');
    expect(a.split('`WITH ${clock},source_journals AS(')).toHaveLength(2);
    expect(a).toContain('crmClockedReadSql(clock,`property.id,property.name FROM pms_properties property');
    expect(a).toContain('crmClockedReadSql(clock,`l.id,l.account_id,l.contact_id,l.created_at,l.owner_id,l.status,l.due_date,v.id opportunity_id,');
    expect(a).toContain('crmClockedReadSql(clock,`cost.campaign_id,COALESCE(SUM(line.debit_minor-line.credit_minor),0) amount');
    expect(a).toContain('(SELECT MIN(stay.checked_in_at) FROM (${crmHotelAnalyticsStays}) stay');
    expect(a).toContain('EXISTS(${crmSelectOneSql(`(${touch.sql}) touch');
  });

  test('keeps every caller bind order unchanged', () => {
    const l = src('leads.ts'), a = src('analytics.ts');
    expect(l).toContain("crmBranchOptionsSql('',true)).bind(a.companyId,a.branchId,a.branchId)");
    expect(l).toContain("crmCampaignOptionsSql(marketingProjectionSources.marketing_campaigns,'marketing_campaigns')).bind(a.companyId)");
    expect(l).toContain('write.prepare(activeCustomerPartiesSql).bind(a.companyId)');
    expect(src('identities.ts')).toContain('write.prepare(activeCustomerPartiesSql).bind(a.companyId)');
    expect(a).toContain("crmBranchOptionsSql(' name',false)).bind(a.companyId,a.branchId,a.branchId)");
    expect(a).toContain("crmCampaignOptionsSql(marketing.sql,'campaign',true)).bind(...marketing.args,a.companyId)");
    expect(a).toContain('.bind(a.companyId,a.branchId,a.branchId,...(access?[a.departmentId,a.assignmentId,a.userId,a.role]:[]))');
    expect(a).toContain('.bind(...sales.args,...hotel.args,a.companyId,');
    expect(a).toContain('.bind(...marketing.args,a.companyId,f.from,f.to,');
  });

  test('leaves locks, command receipts, native kinds and r8-r38 helpers untouched', () => {
    expect(src('purchases.ts')).toContain('LOCK TABLE crm_accounts,crm_contacts,crm_guest_links,crm_guest_sharing_events,crm_leads,crm_opportunities,crm_case_events,crm_lead_activities,crm_lead_activity_events IN SHARE ROW EXCLUSIVE MODE');
    expect(src('purchases.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    for (const kind of ['crm_stmt_1', 'crm_stmt_4', 'crm_stmt_16', 'crm_stmt_17']) expect(src('leads.ts')).toContain(kind);
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

  test.each([
    'property.id,property.name FROM pms_properties property CROSS JOIN report_clock WHERE property.company_id=? ORDER BY property.name',
    'cost.campaign_id,COALESCE(SUM(line.debit_minor-line.credit_minor),0) amount FROM (COST_SOURCES) cost GROUP BY cost.campaign_id',
    "l.id,l.account_id,\n     CASE WHEN v.win_source='pms' THEN r.id IS NOT NULL ELSE TRUE END source_visible,\n     FROM crm_leads l CROSS JOIN report_clock",
  ])('the clocked head folds into the owner select-body plan for %#', (body) => {
    expect(crmClockedReadSql(CLOCK, body)).toBe(`WITH ${CLOCK} ${crmSelectBodySql(body)}`);
    expect(crmClockedReadSql(CLOCK, body)).toBe(`WITH ${CLOCK} SELECT ${body}`);
  });

  test('retirement:ts-statements folds the clocked CTE head (32 -> 31)', () => {
    expect(totalSelects()).toBe(30);
    expect(totalSelects()).toBeLessThan(32);
  });
});
