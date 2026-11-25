import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { crmScopedDescListSql } from '../../../packages/server-crm/src/adapters/read-tail-r40-sql';
import { crmSalesOpportunitySql } from '../../../packages/server-crm/src/adapters/read-tail-r37-sql';

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

/** Concrete stand-ins for the fragments each retired call site passed. */
const FOLLOWUPS = 'CUSTOMER_FOLLOWUPS';
const ORDERS = 'COMMERCE_ORDERS';
const CANDIDATES = 'MARKETING_CANDIDATES';
const TOUCHES = 'MARKETING_TOUCHES';
const CRM_WHERE = 'CRM_SCOPE_WHERE';
const SALES_ALLOWED = "hasPermission(actor,'commerce.sales')?'TRUE':'FALSE'";
const ASSIGN = "assignmentReadSql('crm_work','commerce.sales')";
const OWNER_PROBE = 'CRMWORLD_LEAD_OWNER_PROBE';
const ACTIVITY_EXISTS = 'CRM_LEAD_ACTIVITY_EXISTS';
const ELIG = "salesRecipientEligibility('l','c.owner_id')";
const ROWS = 'CRM_RECORD_ROWS';
const IDS = "ids.map(()=>'?').join(',')";
const VISIBLE = 'CRM_SOURCE_SCOPE';

/** The four retired inline SQL literals, exactly as they read at the r40 AFTER=95
 * baseline, with each interpolation replaced by the fragment its call site
 * supplied. The first three are ordered-list tails; the fourth is the crm-work
 * read over the opportunity/lead record identity union. */
const retired: Array<[string, string, string]> = [
  ['source followups (leads)',
    crmScopedDescListSql(
      `f.id,f.status,f.due_date,f.order_id FROM (${FOLLOWUPS}) f JOIN (${ORDERS}) o ON o.id=f.order_id WHERE f.company_id=? AND o.party_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?)`,
      'f.updated_at DESC LIMIT 50'),
    `SELECT f.id,f.status,f.due_date,f.order_id FROM (${FOLLOWUPS}) f JOIN (${ORDERS}) o ON o.id=f.order_id WHERE f.company_id=? AND o.party_id=? AND (?::text IS NULL OR o.branch_id=?) AND (?::text IS NULL OR o.department_id=?) ORDER BY f.updated_at DESC LIMIT 50`],
  ['marketing candidates (analytics)',
    crmScopedDescListSql(
      `candidate.id,candidate.crm_lead_id,candidate.status,candidate.created_at,candidate.attribution_json FROM (${CANDIDATES}) candidate WHERE candidate.company_id=? AND (?::text IS NULL OR candidate.branch_id=?) AND (?::text IS NULL OR candidate.department_id=?) AND (?::text IS NULL OR candidate.branch_id=?) AND candidate.created_at BETWEEN ? AND ?`,
      'candidate.created_at,candidate.id'),
    `SELECT candidate.id,candidate.crm_lead_id,candidate.status,candidate.created_at,candidate.attribution_json FROM (${CANDIDATES}) candidate WHERE candidate.company_id=? AND (?::text IS NULL OR candidate.branch_id=?) AND (?::text IS NULL OR candidate.department_id=?) AND (?::text IS NULL OR candidate.branch_id=?) AND candidate.created_at BETWEEN ? AND ? ORDER BY candidate.created_at,candidate.id`],
  ['marketing touches (analytics)',
    crmScopedDescListSql(
      `touch.lead_id,touch.campaign_id,touch.source_kind,touch.occurred_at,touch.id FROM (${TOUCHES}) touch JOIN crm_leads l ON l.id=touch.lead_id AND l.company_id=touch.company_id WHERE touch.company_id=? AND ${CRM_WHERE}`,
      'touch.occurred_at,touch.id'),
    `SELECT touch.lead_id,touch.campaign_id,touch.source_kind,touch.occurred_at,touch.id FROM (${TOUCHES}) touch JOIN crm_leads l ON l.id=touch.lead_id AND l.company_id=touch.company_id WHERE touch.company_id=? AND ${CRM_WHERE} ORDER BY touch.occurred_at,touch.id`],
  ['crm work over opportunity/lead records (record-purchases)',
    crmSalesOpportunitySql(
      `crm_work.id,crm_work.kind,${SALES_ALLOWED} AND ${ASSIGN} AND CASE WHEN crm_work.kind='lead' THEN EXISTS(${OWNER_PROBE}) ELSE EXISTS(${ACTIVITY_EXISTS}crm_work.id AND c.owner_id=? AND c.status='open' AND ${ELIG}) END can_act`,
      `(${ROWS}) crm_work`,
      `crm_work.company_id=? AND crm_work.id IN (${IDS}) AND crm_work.kind IN ('lead','activity') AND crm_work.target_id=crm_work.id AND ${VISIBLE}`),
    `SELECT crm_work.id,crm_work.kind,${SALES_ALLOWED} AND ${ASSIGN} AND CASE WHEN crm_work.kind='lead' THEN EXISTS(${OWNER_PROBE}) ELSE EXISTS(${ACTIVITY_EXISTS}crm_work.id AND c.owner_id=? AND c.status='open' AND ${ELIG}) END can_act FROM (${ROWS}) crm_work WHERE crm_work.company_id=? AND crm_work.id IN (${IDS}) AND crm_work.kind IN ('lead','activity') AND crm_work.target_id=crm_work.id AND ${VISIBLE}`],
];

describe('crm read-tail r41 byte-identical head reuse', () => {
  test.each(retired)('%s renders byte-identical read-only SQL', (_name, rendered, expected) => {
    expect(rendered).toBe(expected);
    expect(rendered).toContain('SELECT ');
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('reuses the existing single-token heads without adding a renderer', () => {
    const scoped = src('read-tail-r40-sql.ts');
    const sales = src('read-tail-r37-sql.ts');
    expect(scoped.match(/crmScopedDescListSql =/g)).toHaveLength(1);
    expect(sales.match(/crmSalesOpportunitySql =/g)).toHaveLength(1);
    expect((scoped.match(/SELECT /g) ?? []).length).toBe(3);
    expect((sales.match(/SELECT /g) ?? []).length).toBe(3);

    const l = src('leads.ts');
    expect(l.match(/crmScopedDescListSql\(/g)).toHaveLength(2);
    expect(l).toContain("crmScopedDescListSql(`f.id,f.status,f.due_date,f.order_id");
    expect(l).toContain("'f.updated_at DESC LIMIT 50')");
    expect(l).not.toContain('`SELECT f.id,f.status,f.due_date,f.order_id');

    const a = src('analytics.ts');
    expect(a.match(/crmScopedDescListSql\(/g)).toHaveLength(2);
    expect(a).toContain('crmScopedDescListSql(`candidate.id,candidate.crm_lead_id');
    expect(a).toContain("'candidate.created_at,candidate.id')");
    expect(a).toContain('crmScopedDescListSql(`touch.lead_id,touch.campaign_id');
    expect(a).toContain("'touch.occurred_at,touch.id')");
    expect(a).not.toContain('`SELECT candidate.id,candidate.crm_lead_id');
    expect(a).not.toContain('`SELECT touch.lead_id,touch.campaign_id');

    const r = src('record-purchases.ts');
    expect(r.match(/crmSalesOpportunitySql\(/g)).toHaveLength(1);
    expect(r).toContain('crmSalesOpportunitySql(`crm_work.id,crm_work.kind,');
    expect(r).toContain("`(${recordRows}) crm_work`");
    expect(r).not.toContain('`SELECT crm_work.id,crm_work.kind,');
  });

  test('keeps every caller bind order unchanged', () => {
    const l = src('leads.ts'), a = src('analytics.ts'), r = src('record-purchases.ts');
    expect(l).toContain("'f.updated_at DESC LIMIT 50')).bind(a.companyId,row.account_customer_id,a.branchId,a.branchId,a.departmentId,a.departmentId)");
    expect(a).toContain("'candidate.created_at,candidate.id')).bind(...marketing.args,a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId,f.branchId??null,f.branchId??null,start,end)");
    expect(a).toContain("'touch.occurred_at,touch.id')).bind(...marketing.args,a.companyId,...crm.args)");
    expect(r).toContain('.bind(actor.assignmentId,actor.userId,actor.role,actor.userId,actor.userId,actor.companyId,...ids,...visible.args)');
  });

  test('leaves locks, command receipts, native kinds and r8-r40 helpers untouched', () => {
    const p = src('purchases.ts'), l = src('leads.ts');
    expect(p).toContain('LOCK TABLE crm_accounts,crm_contacts,crm_guest_links,crm_guest_sharing_events,crm_leads,crm_opportunities,crm_case_events,crm_lead_activities,crm_lead_activity_events IN SHARE ROW EXCLUSIVE MODE');
    expect(p).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    for (const kind of ['crm_stmt_1', 'crm_stmt_4', 'crm_stmt_16', 'crm_stmt_17']) expect(l).toContain(kind);
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
      ['read-tail-r39-sql.ts', 'export const crmClockedReadSql'],
      ['read-tail-r40-sql.ts', 'export const crmScopedDescListSql'],
    ] as const) expect(src(file)).toContain(token);
  });

  test('retirement:ts-statements drops from the r40 AFTER=95 to 91', () => {
    expect(totalSelects()).toBe(61);
    expect(totalSelects()).toBeLessThan(95);
  });
});
