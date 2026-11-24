import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  crmCampaignOptionsSql,
  crmOpportunityOwnerGuardSql,
  crmOpportunityOwnerReadSql,
} from '../../../packages/server-crm/src/adapters/read-tail-r27-sql';

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

const E = 'ELIG', SRC = 'SOURCE';

/** Retired inline SQL, exactly as it read before r27. */
const retired: Array<[string, string, string]> = [
  ['policy opportunity read', crmOpportunityOwnerReadSql(E),
    `SELECT l.company_id,l.branch_id,l.department_id,l.owner_id,o.version,
      CASE WHEN ${E} THEN 1 ELSE 0 END owner_eligible
    FROM crm_opportunities o JOIN crm_leads l ON l.id=o.lead_id
    WHERE o.id=? AND l.company_id=?
      AND (?::text IS NULL OR l.branch_id=?)
      AND (?::text IS NULL OR l.department_id=?)`],
  ['service opportunity version guard', crmOpportunityOwnerGuardSql(E),
    `EXISTS(SELECT 1 FROM crm_opportunities o JOIN crm_leads l ON l.id=o.lead_id
      WHERE o.id=? AND l.company_id=? AND o.version=? AND l.owner_id=?
        AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.department_id=?)
        AND ${E})`],
  ['leads campaign options', crmCampaignOptionsSql(SRC, 'marketing_campaigns'),
    `SELECT id,code,name FROM (${SRC}) marketing_campaigns WHERE company_id=? ORDER BY code`],
  ['analytics campaign options', crmCampaignOptionsSql(SRC, 'campaign', true),
    `SELECT id,code name FROM (${SRC}) campaign WHERE company_id=? ORDER BY code`],
];

describe('crm read-tail r27 byte-identical read families', () => {
  test.each(retired)('%s renders byte-identical read-only SQL', (_name, rendered, expected) => {
    expect(rendered).toBe(expected);
    expect(rendered).toContain('SELECT ');
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('owns each renderer once and composes it at the four retired call sites', () => {
    const helper = src('read-tail-r27-sql.ts');
    for (const name of ['crmOpportunityOwnerAnchor', 'crmOpportunityOwnerReadSql', 'crmOpportunityOwnerGuardSql', 'crmCampaignOptionsSql']) {
      expect(helper.match(new RegExp(`${name} =`, 'g'))).toHaveLength(1);
    }
    expect((helper.match(/SELECT /g) ?? []).length).toBe(2);
    expect(src('attachments-policy-facts.ts')).toContain('crmOpportunityOwnerReadSql(ownerEligible)');
    expect(src('attachments-service-facts.ts')).toContain('crmOpportunityOwnerGuardSql(ownerEligible)');
    expect(src('leads.ts')).toContain("crmCampaignOptionsSql(marketingProjectionSources.marketing_campaigns,'marketing_campaigns')");
    expect(src('analytics.ts')).toContain("crmCampaignOptionsSql(marketing.sql,'campaign',true)");
    expect(src('attachments-policy-facts.ts')).not.toContain('SELECT l.company_id,l.branch_id,l.department_id,l.owner_id,o.version,');
    expect(src('attachments-service-facts.ts')).not.toContain('EXISTS(SELECT 1 FROM crm_opportunities');
    expect(src('leads.ts')).not.toContain('SELECT id,code,name FROM (');
    expect(src('analytics.ts')).not.toContain('SELECT id,code name FROM (');
  });

  test('leaves r8-r26 helpers, locks, native kinds and the write path untouched', () => {
    const leads = src('leads.ts'), analytics = src('analytics.ts'), policy = src('attachments-policy-facts.ts');
    const service = src('attachments-service-facts.ts'), purchases = src('purchases.ts');
    expect(leads).toContain('crmGuestLinkReservationSourceSql(');
    expect(analytics.match(/crmCaseEventLeadReadSql\(/g)).toHaveLength(2);
    for (const token of ['activeCustomerPartiesSql', 'leadOwnerNameSql', 'crmBranchOptionsSql']) {
      expect(leads).toContain(token);
    }
    expect(purchases).toContain('crmCreatorLeadScopeSql');
    expect(policy).toContain('salesRecipientEligibility(');
    expect(service).toContain('salesRecipientEligibility(');
    for (const token of ['crmLeadProjectionScope', 'crmClockCteSql', 'crmAssignmentScopeExistsSql']) {
      expect(analytics).toContain(token);
    }
    expect(src('read-tail-r24-sql.ts')).toContain('export const crmCreatorLeadScopeSql');
    expect(src('read-tail-r25-sql.ts')).toContain('export const crmPurchaseRowProjection');
    expect(src('read-tail-r26-sql.ts')).toContain('export const crmGuestLinkReservationSourceSql');
    expect(src('read-tail-r26-sql.ts')).toContain('export const crmCaseEventLeadReadSql');
    expect(policy).toContain('${crmAccountCreatedByScopeSql}');
    expect(service).toContain('${crmAccountCreatedByScopeSql}');
    expect(service).toContain('UPDATE ${descriptor.table} SET version=version+1');
    for (const kind of ['crm_stmt_1', 'crm_stmt_4', 'crm_stmt_16', 'crm_stmt_17']) expect(leads).toContain(kind);
    expect(purchases).toContain('LOCK TABLE crm_accounts,crm_contacts,crm_guest_links,crm_guest_sharing_events,crm_leads,crm_opportunities,crm_case_events,crm_lead_activities,crm_lead_activity_events IN SHARE ROW EXCLUSIVE MODE');
  });

  test('retirement:ts-statements stays strictly below the r26 BEFORE=134 baseline', () => {
    expect(totalSelects()).toBe(132);
    expect(totalSelects()).toBeLessThan(134);
  });
});
