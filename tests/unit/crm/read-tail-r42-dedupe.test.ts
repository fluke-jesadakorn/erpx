import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { crmSelectOneSql } from '../../../packages/server-crm/src/adapters/read-tail-r42-sql';

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

/** Concrete stand-ins for the fragments each retired site passed. */
const SOURCE_ROWS = 'CRM_SOURCE_ROWS';
const OPP_ROWS = 'CRM_OPPORTUNITY_ROWS';
const VALID_LEADS = 'CRM_VALID_LEADS';
const NATIVE_IDS = 'CRM_NATIVE_IDS';
const HOTEL_SHARED = 'CRM_HOTEL_SHARED';
const TOUCHES = 'CRM_TOUCHES';
const TABLE = 'crm_example_table';
const TOUCH_SOURCE = 'CRM_MARKETING_TOUCHES';
const FINANCE_CUSTOMER = 'CRM_FINANCE_CUSTOMER';
const SALES_ORDERS = 'CRM_SALES_ORDERS';
const HOTEL_GUEST = 'CRM_HOTEL_GUEST';
const ALIAS = 'l';
const COMPANY = 'a.companyId';
const NO_EXTRA: string = '';
const WITH_EXTRA: string = 'scope';

const purchaseNative = `(${SOURCE_ROWS}) crm_native WHERE crm_native.id=${ALIAS}.id AND crm_native.company_id=${ALIAS}.company_id`;
const purchaseOpportunity = `(${OPP_ROWS}) native_opportunity WHERE native_opportunity.id=${ALIAS}.id AND native_opportunity.company_id=${ALIAS}.company_id`;
const scopedLead = `(${VALID_LEADS}) scoped_lead JOIN crm_leads native ON native.id=scoped_lead.id WHERE scoped_lead.company_id=crm_source.company_id AND (native.account_id=crm_source.account_id OR native.contact_id=crm_source.contact_id) AND scoped_lead.branch_id=? AND (?::text IS NULL OR scoped_lead.department_id=?)`;
const hotelShared = `(${HOTEL_SHARED}) shared WHERE shared.lead_id=l.id AND shared.company_id=l.company_id AND shared.reservation_id=r.id AND shared.property_id=r.property_id AND shared.guest_id=r.guest_id`;
const touchProbe = `(${TOUCHES}) touch WHERE touch.lead_id=l.id AND touch.company_id=l.company_id AND touch.campaign_id=?`;
const marketingTouch = `(${TOUCH_SOURCE}) touch WHERE touch.id='touch-'||child.command_id AND touch.company_id=c.company_id AND touch.lead_id=c.crm_lead_id AND touch.source_kind='form' AND touch.source_id=c.id AND touch.campaign_id IS NOT DISTINCT FROM NULLIF(c.attribution_json::jsonb->>'campaignId','')`;
const salesOrder = `(${SALES_ORDERS}) actual_sale JOIN (${SALES_ORDERS}) quote ON quote.id=actual_sale.source_id AND quote.company_id=actual_sale.company_id AND quote.kind='quote' WHERE actual_sale.id=o.sales_id AND actual_sale.kind='sales' AND actual_sale.company_id=l.company_id AND actual_sale.party_id=o.customer_id AND actual_sale.currency=o.currency AND actual_sale.branch_id=l.branch_id AND actual_sale.department_id IS NOT DISTINCT FROM l.department_id AND quote.id=o.quote_id`;
const guestIdentity = `crm_contacts contact JOIN (${HOTEL_GUEST}) original ON original.id=guest.guest_id AND original.property_id=guest.property_id AND original.company_id=guest.company_id WHERE contact.id=guest.contact_id AND contact.company_id=guest.company_id`;

/** The fourteen retired inline literals, exactly as they read at the r41
 * AFTER=91 baseline, paired with the helper rendering that replaces them. */
const retired: Array<[string, string, string]> = [
  ['purchases native source',
    `EXISTS(${crmSelectOneSql(purchaseNative)})`,
    `EXISTS(SELECT 1 FROM ${purchaseNative})`],
  ['purchases opportunity absence',
    `NOT EXISTS(${crmSelectOneSql(`crm_opportunities native_opportunity WHERE native_opportunity.lead_id=${ALIAS}.id`)})`,
    `NOT EXISTS(SELECT 1 FROM crm_opportunities native_opportunity WHERE native_opportunity.lead_id=${ALIAS}.id)`],
  ['purchases opportunity presence',
    `EXISTS(${crmSelectOneSql(purchaseOpportunity)})`,
    `EXISTS(SELECT 1 FROM ${purchaseOpportunity})`],
  ['record-purchases scoped lead',
    `EXISTS(${crmSelectOneSql(scopedLead)})`,
    `EXISTS(SELECT 1 FROM ${scopedLead})`],
  ['record-purchases native identity',
    `EXISTS(${crmSelectOneSql(`(${NATIVE_IDS}) native_crm WHERE native_crm.id=${ALIAS}`)})`,
    `EXISTS(SELECT 1 FROM (${NATIVE_IDS}) native_crm WHERE native_crm.id=${ALIAS})`],
  ['record-purchases standalone native probe',
    `${crmSelectOneSql(`(${NATIVE_IDS}) actual WHERE id=?`)}`,
    `SELECT 1 FROM (${NATIVE_IDS}) actual WHERE id=?`],
  ['analytics shared hotel source',
    `EXISTS(${crmSelectOneSql(hotelShared)})`,
    `EXISTS(SELECT 1 FROM ${hotelShared})`],
  ['analytics marketing touch',
    `EXISTS(${crmSelectOneSql(touchProbe)})`,
    `EXISTS(SELECT 1 FROM ${touchProbe})`],
  ['attachments-service-facts version guard (no extra)',
    `EXISTS(${crmSelectOneSql(`${TABLE} WHERE id=? AND company_id=? AND version=?${NO_EXTRA ? ` AND ${NO_EXTRA}` : ''}`)})`,
    `EXISTS(SELECT 1 FROM ${TABLE} WHERE id=? AND company_id=? AND version=?)`],
  ['attachments-service-facts version guard (with extra)',
    `EXISTS(${crmSelectOneSql(`${TABLE} WHERE id=? AND company_id=? AND version=?${WITH_EXTRA ? ` AND ${WITH_EXTRA}` : ''}`)})`,
    `EXISTS(SELECT 1 FROM ${TABLE} WHERE id=? AND company_id=? AND version=? AND scope)`],
  ['marketing touch witness',
    `EXISTS(${crmSelectOneSql(marketingTouch)})`,
    `EXISTS(SELECT 1 FROM ${marketingTouch})`],
  ['marketing handoff candidate',
    `EXISTS(${crmSelectOneSql(`candidates c WHERE c.id=touch.source_id AND c.source_valid AND c.status='handed_off' AND c.crm_lead_id=touch.lead_id`)})`,
    `EXISTS(SELECT 1 FROM candidates c WHERE c.id=touch.source_id AND c.source_valid AND c.status='handed_off' AND c.crm_lead_id=touch.lead_id)`],
  ['sales retained-customer guard',
    `NOT EXISTS(${crmSelectOneSql(`(${FINANCE_CUSTOMER}) customer WHERE customer.id=account.finance_party_id AND customer.company_id=${COMPANY} AND customer.kind='customer'`)})`,
    `NOT EXISTS(SELECT 1 FROM (${FINANCE_CUSTOMER}) customer WHERE customer.id=account.finance_party_id AND customer.company_id=${COMPANY} AND customer.kind='customer')`],
  ['sales actual order',
    `EXISTS(${crmSelectOneSql(salesOrder)})`,
    `EXISTS(SELECT 1 FROM ${salesOrder})`],
  ['sales guest identity',
    `NOT EXISTS(${crmSelectOneSql(guestIdentity)})`,
    `NOT EXISTS(SELECT 1 FROM ${guestIdentity})`],
];

describe('crm read-tail r42 byte-identical existence-probe reuse', () => {
  test.each(retired)('%s renders byte-identical read-only SQL', (_name, rendered, expected) => {
    expect(rendered).toBe(expected);
    expect((rendered.match(/SELECT /g) ?? []).length).toBe(1);
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('owns exactly one renderer with a single read token', () => {
    const helper = src('read-tail-r42-sql.ts');
    expect((helper.match(/crmSelectOneSql =/g) ?? []).length).toBe(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);
  });

  test('every retired site uses the renderer and keeps its wrapper', () => {
    for (const [file, count] of [
      ['purchases.ts', 6],
      ['record-purchases.ts', 3],
      ['analytics.ts', 2],
      ['attachments-service-facts.ts', 1],
      ['marketing-package-obligations.ts', 2],
      ['sales-package-obligations.ts', 4],
    ] as const) {
      const text = src(file);
      expect(text).toContain("read-tail-r42-sql");
      expect((text.match(/crmSelectOneSql\(/g) ?? []).length).toBe(count);
    }
    /** Only the fourteen retired literals are gone; excluded families remain. */
    const retiredSites: Array<[string, string[]]> = [
      ['purchases.ts', [
        'EXISTS(SELECT 1 FROM (${sourceRows[kind]})',
        'NOT EXISTS(SELECT 1 FROM crm_opportunities native_opportunity',
        'EXISTS(SELECT 1 FROM (${crmPurchaseOpportunityRows})',
      ]],
      ['record-purchases.ts', [
        'EXISTS(SELECT 1 FROM (${validLeads}) scoped_lead',
        'EXISTS(SELECT 1 FROM (${nativeIds}) native_crm',
        '`SELECT 1 FROM (${nativeIds}) actual WHERE id=?`',
      ]],
      ['analytics.ts', [
        'EXISTS(SELECT 1 FROM (${crmSharedHotelAnalyticsSources})',
        'EXISTS(SELECT 1 FROM (${touch.sql})',
      ]],
      ['attachments-service-facts.ts', ['EXISTS(SELECT 1 FROM ${item.table}']],
      ['marketing-package-obligations.ts', [
        'EXISTS(SELECT 1 FROM (${marketing.touches})',
        'EXISTS(SELECT 1 FROM candidates c',
      ]],
      ['sales-package-obligations.ts', [
        'EXISTS(SELECT 1 FROM (${crmFinanceCustomerSources})',
        'EXISTS(SELECT 1 FROM (${sales.orders})',
        'NOT EXISTS(SELECT 1 FROM crm_contacts contact JOIN (${crmHotelGuestSources})',
      ]],
    ];
    for (const [file, needles] of retiredSites) {
      const text = src(file);
      for (const needle of needles) expect(text).not.toContain(needle);
    }
    const p = src('purchases.ts');
    expect(p).toContain('EXISTS(${crmSelectOneSql(');
    expect(p).toContain('NOT EXISTS(${crmSelectOneSql(');
    const r = src('record-purchases.ts');
    expect(r).toContain('database.prepare(`${crmSelectOneSql(');
    expect(src('attachments-service-facts.ts')).toContain('EXISTS(${crmSelectOneSql(');
  });

  test('keeps every caller bind order unchanged', () => {
    const p = src('purchases.ts'), r = src('record-purchases.ts'), a = src('analytics.ts');
    const s = src('attachments-service-facts.ts'), m = src('marketing-package-obligations.ts'), sales = src('sales-package-obligations.ts');
    expect(p).toContain('(kind===\'lead\'?` AND (NOT EXISTS(${crmSelectOneSql(');
    expect(r).toContain('.bind(actor.assignmentId,actor.userId,actor.role,actor.userId,actor.userId,actor.companyId,...ids,...visible.args)');
    expect(r).toContain('.bind(id,actor.companyId).first<{kind:CRMPurchaseTarget[\'kind\'];target_id:string}>()');
    expect(r).toContain('`)}`).bind(id).first())');
    expect(a).toContain('.bind(...sales.args,...hotel.args,a.companyId,a.branchId,a.branchId,a.departmentId,a.departmentId,f.branchId??null,f.branchId??null,f.ownerId??null,f.ownerId??null,f.propertyId??null,f.propertyId??null,f.campaignId??null,...touch.args,f.campaignId??null,...crm.args)');
    expect(s).toContain('[targetId, companyId, version]);');
    expect(m).toContain('.bind(input.companyId).all<{kind:string;id:string;fact:unknown;pending:boolean}>()).results;');
    expect(sales).toContain('.bind(input.companyId).all<{kind:string;id:string;fact:unknown;pending:boolean}>()).results;');
  });

  test('leaves locks, receipt reads, native kinds, no-space probe and r8-r41 helpers untouched (r45 routes the four probes)', () => {
    const p = src('purchases.ts'), r = src('attachment-read.ts');
    expect(p).toContain('LOCK TABLE crm_accounts,crm_contacts,crm_guest_links,crm_guest_sharing_events,crm_leads,crm_opportunities,crm_case_events,crm_lead_activities,crm_lead_activity_events IN SHARE ROW EXCLUSIVE MODE');
    expect(p).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(p).toContain('EXISTS(${crmSelectOneSql(`crm_guest_links guest_link WHERE guest_link.contact_id=');
    expect(p).toContain('export const crmHotelReservationProbeSql=crmSelectOneSql(`(${crmHotelReservationSources}) reservation`);');
    expect(p).toContain('export const crmCaseEventReceiptWitnessSql=crmSelectOneSql(`crm_case_events event JOIN command_receipts receipt ON');
    expect(src('sales-package-obligations.ts')).toContain('NOT EXISTS(${crmSelectOneSql(`crm_lead_activity_events event JOIN command_receipts receipt ON');
    expect(r).toContain('`SELECT 1 FROM${actorScopeSql}workforce_actor_scope WHERE ${current.where}`');
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

  test('retirement:ts-statements drops from the r42 AFTER=61 to 57 (r45)', () => {
    expect(totalSelects()).toBe(34);
    expect(totalSelects()).toBeLessThan(61);
  });
});
