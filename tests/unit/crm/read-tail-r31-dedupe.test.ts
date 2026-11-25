import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { crmSelectBodySql } from '../../../packages/server-crm/src/adapters/read-tail-r46-sql';
import { crmRevenueJournalHeadSql } from '../../../packages/server-crm/src/adapters/read-tail-r31-sql';

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

const SALES = 'SALES_POSTING', HOTEL = 'HOTEL_POSTING';
const SALES_ON = 'source.order_id=opportunity.sales_id AND source.company_id=l.company_id';
const HOTEL_ON = 'source.reservation_id=opportunity.reservation_id AND source.company_id=l.company_id';

/** Retired inline SQL, exactly as it read before r31. */
const retired: Array<[string, string, string]> = [
  ['sales-posting revenue journal',
    crmRevenueJournalHeadSql(' lead_id', SALES) + SALES_ON,
    `SELECT l.id lead_id,journal.primary_journal_id,journal.revenue_minor FROM crm_leads l JOIN crm_opportunities opportunity ON opportunity.lead_id=l.id JOIN (${SALES}) source ON ${SALES_ON}`],
  ['hotel-posting revenue journal',
    crmRevenueJournalHeadSql('', HOTEL) + HOTEL_ON,
    `SELECT l.id,journal.primary_journal_id,journal.revenue_minor FROM crm_leads l JOIN crm_opportunities opportunity ON opportunity.lead_id=l.id JOIN (${HOTEL}) source ON ${HOTEL_ON}`],
];

describe('crm read-tail r31 byte-identical SELECT-head family', () => {
  test.each(retired)('%s renders byte-identical read-only SQL', (_name, rendered, expected) => {
    expect(rendered).toBe(expected);
    expect(rendered).toContain('SELECT ');
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('owns the head renderer once and composes it at both retired call sites', () => {
    const helper = src('read-tail-r31-sql.ts');
    expect(helper.match(/crmRevenueJournalHeadSql =/g)).toHaveLength(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(0);

    const analytics = src('analytics.ts');
    expect(analytics).toContain("crmRevenueJournalHeadSql(' lead_id',crmAnalyticsSalesPostingSources)");
    expect(analytics).toContain("crmRevenueJournalHeadSql('',crmHotelAnalyticsPostingSources)");

    expect(analytics).not.toContain('SELECT l.id lead_id,journal.primary_journal_id,journal.revenue_minor FROM crm_leads l');
    expect(analytics).not.toContain('UNION SELECT l.id,journal.primary_journal_id,journal.revenue_minor FROM crm_leads l');
  });

  test('leaves locks, command receipts, native kinds and r8-r30 helpers untouched', () => {
    const purchases = src('purchases.ts'), analytics = src('analytics.ts'), ledger = src('leads.ts');
    expect(purchases).toContain('const crmAssignmentAllowedSql=(alias:string,permission:string)=>crmCurrentScopeAllowedSql(assignmentReadSql(alias,permission),actorScopeSql,alias);');
    expect(purchases).toContain('LOCK TABLE crm_accounts,crm_contacts,crm_guest_links,crm_guest_sharing_events,crm_leads,crm_opportunities,crm_case_events,crm_lead_activities,crm_lead_activity_events IN SHARE ROW EXCLUSIVE MODE');
    expect(purchases).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(purchases).toContain("crmSelectExprSql(\"jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array('marketing'::text)) allowed FROM assignments WHERE id=?\")");
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
    ] as const) expect(src(file)).toContain(token);
  });

  test.each([
    [' lead_id', SALES],
    ['', HOTEL],
  ])('the revenue-journal head folds into the owner select-body plan for %#', (idAlias, source) => {
    const body = `l.id${idAlias},journal.primary_journal_id,journal.revenue_minor FROM crm_leads l JOIN crm_opportunities opportunity ON opportunity.lead_id=l.id JOIN (${source}) source ON `;
    expect(crmRevenueJournalHeadSql(idAlias, source)).toBe(crmSelectBodySql(body));
    expect(crmRevenueJournalHeadSql(idAlias, source)).toBe(`SELECT ${body}`);
  });

  test('retirement:ts-statements folds the revenue-journal head (28 -> 27)', () => {
    expect(totalSelects()).toBe(23);
    expect(totalSelects()).toBeLessThan(123);
  });
});
