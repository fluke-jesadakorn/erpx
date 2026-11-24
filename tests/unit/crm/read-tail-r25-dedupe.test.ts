import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { crmPurchaseRowProjection } from '../../../packages/server-crm/src/adapters/read-tail-r25-sql';

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

/** Retired inline prefixes, exactly as they read before r25. */
const retired: Array<[string, [string, string, string], string]> = [
  ['account', ['account', 'NULL::text branch_id', 'NULL::text department_id'], 'SELECT account.id,account.company_id,NULL::text branch_id,NULL::text department_id,'],
  ['contact', ['contact', 'NULL::text branch_id', 'NULL::text department_id'], 'SELECT contact.id,contact.company_id,NULL::text branch_id,NULL::text department_id,'],
  ['lead', ['lead', 'lead.branch_id', 'lead.department_id'], 'SELECT lead.id,lead.company_id,lead.branch_id,lead.department_id,'],
  ['activity', ['activity', 'lead.branch_id', 'lead.department_id'], 'SELECT activity.id,activity.company_id,lead.branch_id,lead.department_id,'],
  ['opportunity', ['lead', 'lead.branch_id', 'lead.department_id'], 'SELECT lead.id,lead.company_id,lead.branch_id,lead.department_id,'],
  ['guest-link', ['link', 'guest.branch_id', 'NULL::text department_id'], 'SELECT link.id,link.company_id,guest.branch_id,NULL::text department_id,'],
];

describe('crm read-tail r25 purchase row projection family', () => {
  test.each(retired)('%s projection renders byte-identical read-only SQL', (_name, args, expected) => {
    const rendered = crmPurchaseRowProjection(...args);
    expect(rendered).toBe(expected);
    expect(rendered).toContain('SELECT ');
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('owns the renderer once and composes it at the six retired constants', () => {
    const helper = src('read-tail-r25-sql.ts');
    expect(helper.match(/crmPurchaseRowProjection =/g)).toHaveLength(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);
    const purchases = src('purchases.ts');
    expect(purchases.match(/crmPurchaseRowProjection\(/g)).toHaveLength(6);
    for (const [name, args] of retired) {
      const call = `crmPurchaseRowProjection('${args.join("','")}')`;
      expect(purchases).toContain(call);
    }
    for (const prefix of retired.map(([, , value]) => value)) expect(purchases).not.toContain(prefix);
  });

  test('leaves r8-r24 helpers, locks and the policy-facts newline variant untouched', () => {
    const purchases = src('purchases.ts');
    for (const token of [
      'crmCreatorLeadScopeSql', 'crmAssignmentScopeExistsSql', 'crmAccountCreatedByScopeSql',
      'crmAssignmentAllowedSql', 'opportunityWitnessSql', 'crmClockCteSql', 'crmOpportunityLeadSql',
      'crmHotelReservationProbeSql', 'crmCaseEventReceiptWitnessSql', 'crmBranchOptionsSql',
      'crmSourceExistsSql', 'crmMarketingCandidateRows', 'crmLeadActivityExists',
    ]) expect(purchases).toContain(token);
    expect(purchases).toContain('LOCK TABLE crm_accounts,crm_contacts,crm_guest_links,crm_guest_sharing_events,crm_leads,crm_opportunities,crm_case_events,crm_lead_activities,crm_lead_activity_events IN SHARE ROW EXCLUSIVE MODE');
    const policy = src('attachments-policy-facts.ts');
    expect(policy).toContain('${crmAccountCreatedByScopeSql}');
    expect(policy).not.toMatch(/EXISTS\(\s*SELECT 1 FROM crm_leads lead/);
  });

  test('retirement:ts-statements stays strictly below the r24 BEFORE=141 baseline', () => {
    expect(totalSelects()).toBeLessThanOrEqual(104);
    expect(totalSelects()).toBeLessThan(141);
  });
});
