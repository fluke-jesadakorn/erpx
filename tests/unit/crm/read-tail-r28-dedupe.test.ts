import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import {
  crmCaseEventReadSql,
  crmOpportunityProbeSql,
} from '../../../packages/server-crm/src/adapters/read-tail-r28-sql';

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

/** Retired inline SQL, exactly as it read before r28. */
const retired: Array<[string, string, string]> = [
  ['leads case-event history',
    crmCaseEventReadSql('*', 'lead_id=?', ' ORDER BY created_at DESC,id DESC'),
    'SELECT * FROM crm_case_events WHERE lead_id=? ORDER BY created_at DESC,id DESC'],
  ['purchases case-event create snapshot',
    crmCaseEventReadSql('snapshot_json::text snapshot_json', "id=? AND lead_id=? AND action='create' AND actor_id=?"),
    "SELECT snapshot_json::text snapshot_json FROM crm_case_events WHERE id=? AND lead_id=? AND action='create' AND actor_id=?"],
  ['leads opportunity evidence source probe',
    crmOpportunityProbeSql('lead_id', 'id=?'),
    'SELECT lead_id FROM crm_opportunities WHERE id=?'],
  ['purchases opportunity source probe',
    crmOpportunityProbeSql('1', 'lead_id=?'),
    'SELECT 1 FROM crm_opportunities WHERE lead_id=?'],
];

describe('crm read-tail r28 byte-identical read families', () => {
  test.each(retired)('%s renders byte-identical read-only SQL', (_name, rendered, expected) => {
    expect(rendered).toBe(expected);
    expect(rendered).toContain('SELECT ');
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('owns each renderer once and composes it at the four retired call sites', () => {
    const helper = src('read-tail-r28-sql.ts');
    expect(helper.match(/crmCaseEventReadSql =/g)).toHaveLength(1);
    expect(helper.match(/crmOpportunityProbeSql =/g)).toHaveLength(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(0);
    const leads = src('leads.ts'), purchases = src('purchases.ts');
    expect(leads).toContain("crmCaseEventReadSql('*','lead_id=?',' ORDER BY created_at DESC,id DESC')");
    expect(purchases).toContain("crmCaseEventReadSql('snapshot_json::text snapshot_json',");
    expect(leads).toContain("crmOpportunityProbeSql('lead_id','id=?')");
    expect(purchases).toContain("crmOpportunityProbeSql('1','lead_id=?')");
    expect(leads).not.toContain('SELECT * FROM crm_case_events WHERE lead_id=?');
    expect(purchases).not.toContain('SELECT snapshot_json::text snapshot_json FROM crm_case_events');
    expect(leads).not.toContain('SELECT lead_id FROM crm_opportunities WHERE id=?');
    expect(purchases).not.toContain("SELECT 1 FROM crm_opportunities WHERE lead_id=?");
  });

  test('leaves locks, command receipts, r8-r27 helpers and native kinds untouched', () => {
    const leads = src('leads.ts'), purchases = src('purchases.ts'), analytics = src('analytics.ts');
    for (const token of ['crmGuestLinkReservationSourceSql(', 'activeCustomerPartiesSql', 'leadOwnerNameSql', 'crmBranchOptionsSql']) {
      expect(leads).toContain(token);
    }
    expect(analytics.match(/crmCaseEventLeadReadSql\(/g)).toHaveLength(2);
    expect(purchases).toContain('read-tail-r24-sql');
    expect(purchases).toContain('read-tail-r25-sql');
    expect(purchases).toContain('LOCK TABLE crm_accounts,crm_contacts,crm_guest_links,crm_guest_sharing_events,crm_leads,crm_opportunities,crm_case_events,crm_lead_activities,crm_lead_activity_events IN SHARE ROW EXCLUSIVE MODE');
    expect(purchases).toContain('crmCaseEventReceiptWitnessSql');
    expect(purchases).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(purchases).toContain("crmSelectExprSql(\"jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array('marketing'::text)) allowed FROM assignments WHERE id=?\")");
    for (const kind of ['crm_stmt_1', 'crm_stmt_4', 'crm_stmt_16', 'crm_stmt_17']) expect(leads).toContain(kind);
    expect(src('read-tail-r24-sql.ts')).toContain('export const crmCreatorLeadScopeSql');
    expect(src('read-tail-r25-sql.ts')).toContain('export const crmPurchaseRowProjection');
    expect(src('read-tail-r26-sql.ts')).toContain('export const crmCaseEventLeadReadSql');
    expect(src('read-tail-r27-sql.ts')).toContain('export const crmCampaignOptionsSql');
  });

  test('retirement:ts-statements stays strictly below the r28 BEFORE=132 baseline', () => {
    expect(totalSelects()).toBe(43);
    expect(totalSelects()).toBeLessThan(132);
  });
});
