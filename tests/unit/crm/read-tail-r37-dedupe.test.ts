import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { crmSelectBodySql } from '../../../packages/server-crm/src/adapters/read-tail-r46-sql';
import {
  crmMetricBreakdownSql,
  crmSalesOpportunitySql,
  crmSingleKeySourceSql,
} from '../../../packages/server-crm/src/adapters/read-tail-r37-sql';

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

/** Retired inline SQL, exactly as it read before r37 (interpolations kept as the
 * concrete fragments the call sites pass). */
const retired: Array<[string, string, string]> = [
  ['lead company currency source',
    crmSingleKeySourceSql('currency', 'COMMERCE_SETTINGS', 'commerce_settings'),
    'SELECT currency FROM (COMMERCE_SETTINGS) commerce_settings WHERE company_id=?'],
  ['distinct lead owner-id source',
    crmSingleKeySourceSql('DISTINCT user_id owner_id', 'ASSIGNMENTS', 'assignments'),
    'SELECT DISTINCT user_id owner_id FROM (ASSIGNMENTS) assignments WHERE company_id=?'],
  ['opportunity pipeline breakdown',
    crmMetricBreakdownSql('v.stage', 'sum(v.expected_minor)::text',
      " FROM crm_opportunities v JOIN (PURCHASED) l ON l.id=v.lead_id WHERE l.company_id=? AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.department_id=?) AND v.currency=? AND v.close_date BETWEEN ? AND ? AND v.stage IN ('discovery','proposal','negotiation') GROUP BY 1 ORDER BY 1"),
    "SELECT v.stage label,sum(v.expected_minor)::text value FROM crm_opportunities v JOIN (PURCHASED) l ON l.id=v.lead_id WHERE l.company_id=? AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.department_id=?) AND v.currency=? AND v.close_date BETWEEN ? AND ? AND v.stage IN ('discovery','proposal','negotiation') GROUP BY 1 ORDER BY 1"],
  ['overdue followups breakdown',
    crmMetricBreakdownSql('c.status', 'count(*)::text',
      " FROM (PURCHASED) c JOIN (ORDERS) o ON o.id=c.order_id AND o.company_id=c.company_id WHERE o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?) AND o.kind IN ('sales','quote') AND c.status='open' AND c.due_date BETWEEN ? AND ? AND c.due_date<to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') GROUP BY 1 ORDER BY 1"),
    "SELECT c.status label,count(*)::text value FROM (PURCHASED) c JOIN (ORDERS) o ON o.id=c.order_id AND o.company_id=c.company_id WHERE o.company_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?) AND o.kind IN ('sales','quote') AND c.status='open' AND c.due_date BETWEEN ? AND ? AND c.due_date<to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') GROUP BY 1 ORDER BY 1"],
  ['qualified opportunity customer counts',
    crmSalesOpportunitySql('v.customer_id,COUNT(*) count', 'crm_opportunities v JOIN crm_leads l ON l.id=v.lead_id',
      "l.company_id=? AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.department_id=?) AND PURCHASE_WHERE AND v.customer_id IN (?,?) AND l.status='qualified' AND v.stage NOT IN ('won','lost') GROUP BY v.customer_id"),
    "SELECT v.customer_id,COUNT(*) count FROM crm_opportunities v JOIN crm_leads l ON l.id=v.lead_id WHERE l.company_id=? AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.department_id=?) AND PURCHASE_WHERE AND v.customer_id IN (?,?) AND l.status='qualified' AND v.stage NOT IN ('won','lost') GROUP BY v.customer_id"],
  ['open qualified opportunities',
    crmSalesOpportunitySql('l.id,l.title,l.due_date,v.stage,v.close_date,v.expected_minor,v.currency', 'crm_leads l JOIN crm_opportunities v ON v.lead_id=l.id',
      "l.company_id=? AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.department_id=?) AND PURCHASE_WHERE AND v.customer_id=? AND l.status='qualified' AND v.stage NOT IN ('won','lost') ORDER BY v.close_date,l.id LIMIT 30"),
    "SELECT l.id,l.title,l.due_date,v.stage,v.close_date,v.expected_minor,v.currency FROM crm_leads l JOIN crm_opportunities v ON v.lead_id=l.id WHERE l.company_id=? AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.department_id=?) AND PURCHASE_WHERE AND v.customer_id=? AND l.status='qualified' AND v.stage NOT IN ('won','lost') ORDER BY v.close_date,l.id LIMIT 30"],
];

describe('crm read-tail r37 byte-identical read heads', () => {
  test.each(retired)('%s renders byte-identical read-only SQL', (_name, rendered, expected) => {
    expect(rendered).toBe(expected);
    expect(rendered).toContain('SELECT ');
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('owns each renderer once and composes it at both retired call sites', () => {
    const helper = src('read-tail-r37-sql.ts');
    for (const name of ['crmSingleKeySourceSql', 'crmMetricBreakdownSql', 'crmSalesOpportunitySql']) {
      expect(helper.match(new RegExp(`${name} =`, 'g'))).toHaveLength(1);
    }
    expect((helper.match(/SELECT /g) ?? []).length).toBe(0);

    const leads = src('leads.ts');
    expect(leads.match(/crmSingleKeySourceSql\(/g)).toHaveLength(2);
    expect(leads).toContain("crmSingleKeySourceSql('currency',commerceProjectionSources.commerce_settings,'commerce_settings')");
    expect(leads).toContain("crmSingleKeySourceSql('DISTINCT user_id owner_id',identity_accessProjectionSources.assignments,'assignments')");
    expect(leads).not.toContain('`SELECT currency FROM (${commerceProjectionSources.commerce_settings})');
    expect(leads).not.toContain('SELECT DISTINCT user_id owner_id FROM (');

    const metrics = src('executive-metric-projections.ts');
    expect(metrics.match(/crmMetricBreakdownSql\(/g)).toHaveLength(2);
    expect(metrics).toContain("crmMetricBreakdownSql(label,'sum(v.expected_minor)::text',");
    expect(metrics).toContain("crmMetricBreakdownSql(label,'count(*)::text',");
    expect(metrics).not.toContain('`SELECT ${label} label,');

    const sales = src('sales-projection.ts');
    expect(sales.match(/crmSalesOpportunitySql\(/g)).toHaveLength(2);
    expect(sales).toContain("crmSalesOpportunitySql('v.customer_id,COUNT(*) count','crm_opportunities v JOIN crm_leads l ON l.id=v.lead_id',");
    expect(sales).toContain("crmSalesOpportunitySql('l.id,l.title,l.due_date,v.stage,v.close_date,v.expected_minor,v.currency','crm_leads l JOIN crm_opportunities v ON v.lead_id=l.id',");
    expect(sales).not.toContain('`SELECT v.customer_id,COUNT(*) count FROM');
    expect(sales).not.toContain('`SELECT l.id,l.title,l.due_date,v.stage');
  });

  test('keeps bind order unchanged at every retired site', () => {
    const leads = src('leads.ts'), metrics = src('executive-metric-projections.ts'), sales = src('sales-projection.ts');
    expect(leads).toContain('.bind(a.companyId).first<Row>()');
    expect(leads).toContain('.bind(a.companyId,a.companyId,id).all<{id:string;name:string}>()');
    expect(metrics).toContain('sourceDb.prepare(sql).bind(...values)');
    expect(sales).toContain('.bind(...lead.args,...purchase.args, ...ids)');
    expect(sales).toContain('.bind(...lead.args,...purchase.args, id)');
  });

  test('leaves locks, command receipts, native kinds and r8-r36 helpers untouched', () => {
    expect(src('purchases.ts')).toContain('LOCK TABLE crm_accounts,crm_contacts,crm_guest_links,crm_guest_sharing_events,crm_leads,crm_opportunities,crm_case_events,crm_lead_activities,crm_lead_activity_events IN SHARE ROW EXCLUSIVE MODE');
    expect(src('purchases.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    for (const kind of ['crm_stmt_1', 'crm_stmt_4', 'crm_stmt_17']) expect(src('leads.ts')).toContain(kind);
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
    ] as const) expect(src(file)).toContain(token);
  });

  test.each([
    ['v.stage', 'sum(v.expected_minor)::text', ' FROM crm_opportunities v GROUP BY 1 ORDER BY 1'],
    ['c.status', 'count(*)::text', ' FROM (PURCHASED) c GROUP BY 1 ORDER BY 1'],
  ])('the option projection folds into the owner select-body plan for %#', (label, value, tail) => {
    expect(crmMetricBreakdownSql(label, value, tail)).toBe(
      crmSelectBodySql(`${label} label,${value} value${tail}`),
    );
    expect(crmMetricBreakdownSql(label, value, tail)).toBe(
      `SELECT ${label} label,${value} value${tail}`,
    );
  });

  test('retirement:ts-statements folds the option-projection head (31 -> 30)', () => {
    expect(totalSelects()).toBe(23);
    expect(totalSelects()).toBeLessThan(31);
  });
});
