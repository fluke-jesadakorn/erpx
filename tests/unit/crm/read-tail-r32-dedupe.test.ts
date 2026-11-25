import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { commercialOwnerProjectionSources as commerceProjectionSources } from '../../../packages/server-commerce/src/public/commercial-owner-projection-sources';
import { crmCommerceOrderReadSql } from '../../../packages/server-crm/src/adapters/read-tail-r32-sql';

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

const ORDERS = commerceProjectionSources.commerce_orders;

/** Retired inline SQL, exactly as it read before r32. */
const retired: Array<[string, string, string]> = [
  ['quotes',
    crmCommerceOrderReadSql('o.id,o.number,o.version,o.state', 'o', `WHERE o.company_id=? AND o.party_id=? AND o.branch_id=? AND o.department_id IS NOT DISTINCT FROM ?::text AND o.kind='quote' AND o.state='confirmed' ORDER BY o.updated_at DESC`),
    `SELECT o.id,o.number,o.version,o.state FROM (${ORDERS}) o WHERE o.company_id=? AND o.party_id=? AND o.branch_id=? AND o.department_id IS NOT DISTINCT FROM ?::text AND o.kind='quote' AND o.state='confirmed' ORDER BY o.updated_at DESC`],
  ['sales',
    crmCommerceOrderReadSql('id,number,version,state', 'commerce_orders', `WHERE company_id=? AND source_id=? AND kind='sales' AND state IN ('confirmed','partially_fulfilled','fulfilled') ORDER BY updated_at DESC`),
    `SELECT id,number,version,state FROM (${ORDERS}) commerce_orders WHERE company_id=? AND source_id=? AND kind='sales' AND state IN ('confirmed','partially_fulfilled','fulfilled') ORDER BY updated_at DESC`],
  ['source orders',
    crmCommerceOrderReadSql('id,number,kind,state,order_date', 'commerce_orders', `WHERE company_id=? AND party_id=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?) ORDER BY updated_at DESC LIMIT 50`),
    `SELECT id,number,kind,state,order_date FROM (${ORDERS}) commerce_orders WHERE company_id=? AND party_id=? AND (?::text IS NULL OR branch_id=?) AND (?::text IS NULL OR department_id=?) ORDER BY updated_at DESC LIMIT 50`],
];

describe('crm read-tail r32 byte-identical commerce-order read-head family', () => {
  test.each(retired)('%s renders byte-identical read-only SQL', (_name, rendered, expected) => {
    expect(rendered).toBe(expected);
    expect(rendered).toContain('SELECT ');
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('owns the head renderer once and composes it at all three retired call sites', () => {
    const helper = src('read-tail-r32-sql.ts');
    expect(helper.match(/crmCommerceOrderReadSql =/g)).toHaveLength(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);

    const leads = src('leads.ts');
    expect(leads).toContain("crmCommerceOrderReadSql('o.id,o.number,o.version,o.state','o',");
    expect(leads).toContain("crmCommerceOrderReadSql('id,number,version,state','commerce_orders',");
    expect(leads).toContain("crmCommerceOrderReadSql('id,number,kind,state,order_date','commerce_orders',");

    expect(leads).not.toContain('SELECT o.id,o.number,o.version,o.state FROM (${commerceProjectionSources.commerce_orders})');
    expect(leads).not.toContain('SELECT id,number,version,state FROM (${commerceProjectionSources.commerce_orders})');
    expect(leads).not.toContain('SELECT id,number,kind,state,order_date FROM (${commerceProjectionSources.commerce_orders})');
  });

  test('leaves locks, command receipts, native kinds and r8-r31 helpers untouched', () => {
    const purchases = src('purchases.ts'), analytics = src('analytics.ts'), ledger = src('leads.ts');
    expect(purchases).toContain('const crmAssignmentAllowedSql=(alias:string,permission:string)=>crmCurrentScopeAllowedSql(assignmentReadSql(alias,permission),actorScopeSql,alias);');
    expect(purchases).toContain('LOCK TABLE crm_accounts,crm_contacts,crm_guest_links,crm_guest_sharing_events,crm_leads,crm_opportunities,crm_case_events,crm_lead_activities,crm_lead_activity_events IN SHARE ROW EXCLUSIVE MODE');
    expect(purchases).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(purchases).toContain("SELECT (jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array('marketing'::text)) allowed FROM assignments WHERE id=?");
    expect(purchases).toContain('export const crmAssignmentModuleExists=');
    expect(analytics).toContain('crmAssignmentScopeExistsSql');
    for (const kind of ['crm_stmt_1', 'crm_stmt_4', 'crm_stmt_16', 'crm_stmt_17']) expect(ledger).toContain(kind);
    for (const [file, token] of [
      ['read-tail-r24-sql.ts', 'export const crmCreatorLeadScopeSql'],
      ['read-tail-r25-sql.ts', 'export const crmPurchaseRowProjection'],
      ['read-tail-r26-sql.ts', 'export const crmGuestLinkReservationSourceSql'],
      ['read-tail-r27-sql.ts', 'const crmOpportunityOwnerAnchor ='],
      ['read-tail-r28-sql.ts', 'export const crmCaseEventReadSql'],
      ['read-tail-r29-sql.ts', 'const crmAccountReadAnchor ='],
      ['read-tail-r30-sql.ts', 'export const crmCurrentScopeAllowedSql'],
      ['read-tail-r31-sql.ts', 'export const crmRevenueJournalHeadSql'],
    ] as const) expect(src(file)).toContain(token);
  });

  test('retirement:ts-statements stays strictly below the recorded BEFORE=122', () => {
    expect(totalSelects()).toBe(54);
    expect(totalSelects()).toBeLessThan(122);
  });
});
