import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { crmUnionAllSql } from '../../../packages/server-crm/src/adapters/read-tail-r44-sql';

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

/** Concrete stand-ins for the nested cross-owner sources, kind interpolations
 * and r42/r43 renderers each retired site passes. Those nested renderers keep
 * their own byte-oracle tests; this one pins the UNION-ALL head and its body. */
const KIND = 'lead-history';
const HISTORY_SOURCE = 'crm_case_events event JOIN leads l ON l.id=event.lead_id';
const FINANCE_MISSING_LEAD = 'CRM_FINANCE_MISSING_LEAD';
const FINANCE_MISSING_ACCOUNT = 'CRM_FINANCE_MISSING_ACCOUNT';
const CASE_DEAL = 'CRM_CASE_EVENT_RECEIPT_WITNESS';
const SALES_ORDER = 'CRM_SALES_ORDER_EXISTS';
const HOTEL_RESERVATION = 'CRM_HOTEL_RESERVATION_PROBE';
const ACCOUNT_CONTACT = 'CRM_ACCOUNT_CONTACT_PROBE';
const HOTEL_GUEST = 'CRM_HOTEL_GUEST_SOURCE';
const MARKETING_TOUCH = 'CRM_MARKETING_TOUCH_PROBE';
const PURCHASE_LEADS = 'CRM_PURCHASE_LEAD_ROWS';
const PURCHASE_ACTIVITIES = 'CRM_PURCHASE_ACTIVITY_ROWS';
const PURCHASE_ACCOUNTS = 'CRM_PURCHASE_ACCOUNT_ROWS';
const PURCHASE_CONTACTS = 'CRM_PURCHASE_CONTACT_ROWS';
const PURCHASE_LINKS = 'CRM_PURCHASE_GUEST_LINK_ROWS';
const ACTIVITY_LEAD_ID = 'CRM_ACTIVITY_LEAD_ID';

const history = `'${KIND}',event.id,to_jsonb(event),false FROM ${HISTORY_SOURCE}`;
const leadIdentity = `'lead-identity',l.id,jsonb_build_object('lead',to_jsonb(l),'account',to_jsonb(account),'contact',to_jsonb(contact)),account.id IS NULL OR contact.id IS NULL OR(${FINANCE_MISSING_LEAD}) FROM leads l LEFT JOIN crm_accounts account ON account.id=l.account_id AND account.company_id=l.company_id LEFT JOIN crm_contacts contact ON contact.id=l.contact_id AND contact.company_id=l.company_id AND contact.account_id=account.id`;
const opportunity = `'opportunity',o.id,to_jsonb(o),NOT COALESCE(o.stage IN ('won','lost') AND EXISTS(${CASE_DEAL}'-deal' AND receipt.record_id=l.id AND receipt.actor_id=event.actor_id AND receipt.operation=CASE WHEN o.stage='lost' THEN 'crm.opportunity.lose' WHEN o.win_source='pms' THEN 'crm.opportunity.win-pms' ELSE 'crm.opportunity.win' END AND receipt.authority_json::jsonb->>'companyId'=l.company_id WHERE event.lead_id=l.id AND event.opportunity_id=o.id AND event.version=o.version AND event.action=CASE WHEN o.stage='lost' THEN 'lose' WHEN o.win_source='pms' THEN 'win-pms' ELSE 'win' END AND event.snapshot_json->>'stage'=o.stage)`;
const activity = `'activity',activity.id,to_jsonb(activity),l.id IS NULL OR activity.company_id<>l.company_id OR activity.status='open' OR NOT EXISTS(SELECT 1 FROM crm_lead_activity_events event JOIN command_receipts receipt ON event.id='crm-activity-event-'||receipt.command_id AND receipt.record_id=activity.id AND receipt.operation='crm.activity.'||event.action AND receipt.actor_id=event.actor_id AND receipt.authority_json::jsonb->>'companyId'=activity.company_id WHERE event.activity_id=activity.id AND event.version=activity.version AND event.action=CASE activity.status WHEN 'completed' THEN 'complete' WHEN 'cancelled' THEN 'cancel' END AND event.owner_id=activity.owner_id AND event.due_date=activity.due_date AND receipt.result_json::jsonb->>'version'=activity.version::text) FROM crm_lead_activities activity LEFT JOIN crm_leads l ON l.id=activity.lead_id CROSS JOIN scope WHERE activity.company_id=scope.company_id OR l.company_id=scope.company_id`;
const account = `'account',account.id,to_jsonb(account),${FINANCE_MISSING_ACCOUNT} FROM crm_accounts account,scope WHERE account.company_id=scope.company_id`;
const contact = `'contact',contact.id,to_jsonb(contact),NOT EXISTS(${ACCOUNT_CONTACT}) FROM crm_contacts contact,scope WHERE contact.company_id=scope.company_id`;
const guestIdentity = `'guest-identity',guest.id,to_jsonb(guest),NOT EXISTS(${HOTEL_GUEST}) FROM crm_guest_links guest,scope WHERE guest.company_id=scope.company_id ORDER BY kind,id`;
const attribution = `'attribution',touch.id,to_jsonb(touch),NOT COALESCE(lead.id IS NOT NULL AND (touch.campaign_id IS NULL OR campaign.id IS NOT NULL) AND (touch.source_kind='staff' AND touch.source_id=touch.lead_id OR touch.source_kind='form' AND EXISTS(${MARKETING_TOUCH}) OR touch.source_kind='pms' AND EXISTS(${HOTEL_RESERVATION} JOIN crm_opportunities opportunity ON opportunity.lead_id=touch.lead_id AND opportunity.reservation_id=reservation.id WHERE reservation.id=touch.source_id AND reservation.company_id=touch.company_id AND reservation.branch_id=lead.branch_id)),false)`;
const recordActivity = `activity.id,'activity',activity.id,activity.company_id,activity.branch_id,activity.department_id,NULL,NULL,NULL FROM (${PURCHASE_ACTIVITIES}) activity JOIN (${PURCHASE_LEADS}) lead ON lead.id=${ACTIVITY_LEAD_ID} AND lead.company_id=activity.company_id`;
const recordOpportunity = `opportunity.id,'lead',lead.id,lead.company_id,lead.branch_id,lead.department_id,NULL,NULL,NULL FROM crm_opportunities opportunity JOIN (${PURCHASE_LEADS}) lead ON lead.id=opportunity.lead_id`;
const recordAccount = `account.id,'account',account.id,account.company_id,NULL,NULL,account.created_by,account.id,NULL FROM (${PURCHASE_ACCOUNTS}) account`;
const recordContact = `contact.id,'contact',contact.id,contact.company_id,NULL,NULL,contact.created_by,NULL,contact.id FROM (${PURCHASE_CONTACTS}) contact`;
const recordGuest = `link.id,'guest-link',link.id,link.company_id,link.branch_id,NULL,link.created_by,NULL,native.contact_id FROM (${PURCHASE_LINKS}) link JOIN crm_guest_links native ON native.id=link.id`;

/** The thirteen retired inline literals, paired with the body the helper now
 * receives. Expected heads are written out byte-for-byte, not derived from the
 * helper, so a whitespace or head regression fails the oracle. */
const retired: Array<[string, string, string]> = [
  ['sales history fact', crmUnionAllSql(history), `UNION ALL SELECT ${history}`],
  ['sales lead identity', crmUnionAllSql(leadIdentity), `UNION ALL SELECT ${leadIdentity}`],
  ['sales opportunity', crmUnionAllSql(opportunity), `UNION ALL SELECT ${opportunity}`],
  ['sales activity', crmUnionAllSql(activity), `UNION ALL SELECT ${activity}`],
  ['sales account', crmUnionAllSql(account), `UNION ALL SELECT ${account}`],
  ['sales contact', crmUnionAllSql(contact), `UNION ALL SELECT ${contact}`],
  ['sales guest identity', crmUnionAllSql(guestIdentity), `UNION ALL SELECT ${guestIdentity}`],
  ['marketing attribution', crmUnionAllSql(attribution), `UNION ALL SELECT ${attribution}`],
  ['record activity', crmUnionAllSql(recordActivity), `UNION ALL SELECT ${recordActivity}`],
  ['record opportunity', crmUnionAllSql(recordOpportunity), `UNION ALL SELECT ${recordOpportunity}`],
  ['record account', crmUnionAllSql(recordAccount), `UNION ALL SELECT ${recordAccount}`],
  ['record contact', crmUnionAllSql(recordContact), `UNION ALL SELECT ${recordContact}`],
  ['record guest link', crmUnionAllSql(recordGuest), `UNION ALL SELECT ${recordGuest}`],
];

describe('crm read-tail r44 byte-identical UNION-ALL reuse', () => {
  test.each(retired)('%s renders byte-identical read-only SQL', (_name, rendered, expected) => {
    expect(rendered).toBe(expected);
    expect((rendered.match(/UNION ALL SELECT /g) ?? []).length).toBe(1);
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('owns exactly one renderer with a single read token', () => {
    const helper = src('read-tail-r44-sql.ts');
    expect((helper.match(/crmUnionAllSql =/g) ?? []).length).toBe(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);
  });

  test('every retired site uses the renderer and keeps its leading indentation', () => {
    const sales = src('sales-package-obligations.ts');
    const marketing = src('marketing-package-obligations.ts');
    const record = src('record-purchases.ts');
    for (const [file, count] of [
      ['sales-package-obligations.ts', 7],
      ['marketing-package-obligations.ts', 1],
      ['record-purchases.ts', 5],
    ] as const) {
      const text = src(file);
      expect(text).toContain("read-tail-r44-sql");
      expect((text.match(/crmUnionAllSql\(/g) ?? []).length).toBe(count);
      expect((text.match(/\n \$\{crmUnionAllSql\(`/g) ?? []).length).toBe(count - (file === 'sales-package-obligations.ts' ? 1 : 0));
      expect(text).not.toContain('UNION ALL SELECT ');
    }
    expect(sales).toContain("=>crmUnionAllSql(`'${kind}'");
    for (const head of ['lead-identity', 'opportunity', 'activity', 'account', 'contact', 'guest-identity']) {
      expect(sales).toContain("${crmUnionAllSql(`'" + head + "'");
    }
    expect(marketing).toContain("${crmUnionAllSql(`'attribution'");
    for (const head of ['activity.id', 'opportunity.id', 'account.id', 'contact.id', 'link.id']) {
      expect(record).toContain("${crmUnionAllSql(`" + head);
    }
  });

  test('keeps every caller bind order unchanged', () => {
    const s = src('sales-package-obligations.ts'), m = src('marketing-package-obligations.ts'), r = src('record-purchases.ts');
    expect(s).toContain('.bind(input.companyId).all<{kind:string;id:string;fact:unknown;pending:boolean}>()).results;');
    expect(m).toContain('.bind(input.companyId).all<{kind:string;id:string;fact:unknown;pending:boolean}>()).results;');
    expect(r).toContain('.bind(actor.assignmentId,actor.userId,actor.role,actor.userId,actor.userId,actor.companyId,...ids,...visible.args)');
    expect(r).toContain('.bind(id,actor.companyId).first<{kind:CRMPurchaseTarget[\'kind\'];target_id:string}>()');
    expect(r).toContain('`)}`).bind(id).first())');
  });

  test('keeps kind, cross-owner and nested interpolations intact', () => {
    const s = src('sales-package-obligations.ts'), m = src('marketing-package-obligations.ts'), r = src('record-purchases.ts');
    expect(s).toContain("crmUnionAllSql(`'${kind}',event.id,to_jsonb(event),false FROM ${source}`)");
    expect(s).toContain("OR(${financeCustomerMissingSql('l.company_id')})");
    expect(s).toContain("EXISTS(${crmCaseEventReceiptWitnessSql}'-deal'");
    expect(s).toContain("EXISTS(${crmSelectOneSql(`(${sales.orders})");
    expect(s).toContain("EXISTS(${crmHotelReservationProbeSql} JOIN crm_guest_links guest");
    expect(s).toContain("NOT EXISTS(${crmAccountContactProbeSql()})");
    expect(s).toContain("NOT EXISTS(${crmSelectOneSql(`crm_contacts contact JOIN (${crmHotelGuestSources})");
    expect(m).toContain("EXISTS(${crmSelectOneSql(`candidates c WHERE c.id=touch.source_id");
    expect(m).toContain("EXISTS(${crmHotelReservationProbeSql} JOIN crm_opportunities opportunity");
    expect(r).toContain("FROM (${crmPurchaseActivityRows}) activity JOIN (${validLeads}) lead ON lead.id=${crmScalarSubquerySql(");
    expect(r).toContain("FROM (${crmPurchaseGuestLinkRows}) link JOIN crm_guest_links native ON native.id=link.id`)}`;");
  });

  test('keeps r8-r43 helpers intact (r47 routes the MATERIALIZED heads; r45 routes the activity NOT EXISTS)', () => {
    const s = src('sales-package-obligations.ts'), m = src('marketing-package-obligations.ts'), r = src('record-purchases.ts');
    expect(s).toContain("SELECT 'lead' kind,l.id,to_jsonb(l) fact");
    expect(s).toContain("leads ${crmMaterializedScopeSql('l.* FROM crm_leads l,scope WHERE l.company_id=scope.company_id')}");
    expect(m).toContain('candidates ${crmMaterializedScopeSql(`candidate.* FROM (${marketing.candidates})');
    expect(s).toContain('NOT EXISTS(${crmSelectOneSql(`crm_lead_activity_events event JOIN command_receipts receipt ON');
    expect(s).toContain(" AND(o.stage='lost' OR o.win_source='sales' AND EXISTS(${crmSelectOneSql(");
    expect(m).toContain(' FROM (${marketing.touches}) touch CROSS JOIN scope');
    expect(r).toContain("const nativeIds=[crmNativeIdSql(");
    expect(r).toContain("].join(' UNION ALL ')");
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
      ['read-tail-r42-sql.ts', 'export const crmSelectOneSql'],
      ['read-tail-r43-sql.ts', 'export const crmScalarSubquerySql'],
    ] as const) expect(src(file)).toContain(token);
  });

  test('retirement:ts-statements drops from the r44 AFTER=61 to 57 (r45)', () => {
    expect(totalSelects()).toBe(40);
    expect(totalSelects()).toBeLessThan(61);
  });
});
