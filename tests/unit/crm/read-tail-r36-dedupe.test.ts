import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { crmOpportunitySourceAnchorSql } from '../../../packages/server-crm/src/adapters/read-tail-r36-sql';

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

/** Retired inline SQL, exactly as it read before r36 (interpolations kept as the
 * concrete fragments the call sites pass). */
const retired: Array<[string, string, string]> = [
  ['attachment source opportunity witness',
    crmOpportunitySourceAnchorSql(
      "jsonb_build_object('opportunity',to_jsonb(opportunity)-'version'-'updated_at','lead',to_jsonb(lead)-'version'-'updated_at','native',native.witness)::text\n",
      " JOIN (${crmPurchaseOpportunityRows}) native ON native.id=lead.id AND native.company_id=lead.company_id\n WHERE opportunity.id=? AND lead.company_id=? AND (?::text IS NULL OR lead.branch_id=?) AND (?::text IS NULL OR lead.department_id=?)",
    ),
    "SELECT jsonb_build_object('opportunity',to_jsonb(opportunity)-'version'-'updated_at','lead',to_jsonb(lead)-'version'-'updated_at','native',native.witness)::text\n FROM crm_opportunities opportunity JOIN crm_leads lead ON lead.id=opportunity.lead_id JOIN (${crmPurchaseOpportunityRows}) native ON native.id=lead.id AND native.company_id=lead.company_id\n WHERE opportunity.id=? AND lead.company_id=? AND (?::text IS NULL OR lead.branch_id=?) AND (?::text IS NULL OR lead.department_id=?)",
  ],
  ['attachment private opportunity image',
    crmOpportunitySourceAnchorSql(
      "jsonb_build_object('opportunity',to_jsonb(opportunity),'lead',to_jsonb(lead),'account',to_jsonb(account),'contact',to_jsonb(contact),\n  'guestLinks',(${crmRelatedAggSql('link','crm_guest_links link WHERE link.company_id=lead.company_id AND link.contact_id=lead.contact_id')}),\n  ${nativeImageSql(crmPurchaseOpportunityRows,'native.id=lead.id AND native.company_id=lead.company_id')})::text image,\n  opportunity.customer_id,opportunity.quote_id,opportunity.sales_id,opportunity.reservation_id",
      "\n  LEFT JOIN crm_accounts account ON account.id=lead.account_id AND account.company_id=lead.company_id\n  LEFT JOIN crm_contacts contact ON contact.id=lead.contact_id AND contact.account_id=account.id AND contact.company_id=lead.company_id\n  WHERE opportunity.id=? AND lead.company_id=?",
    ),
    "SELECT jsonb_build_object('opportunity',to_jsonb(opportunity),'lead',to_jsonb(lead),'account',to_jsonb(account),'contact',to_jsonb(contact),\n  'guestLinks',(${crmRelatedAggSql('link','crm_guest_links link WHERE link.company_id=lead.company_id AND link.contact_id=lead.contact_id')}),\n  ${nativeImageSql(crmPurchaseOpportunityRows,'native.id=lead.id AND native.company_id=lead.company_id')})::text image,\n  opportunity.customer_id,opportunity.quote_id,opportunity.sales_id,opportunity.reservation_id FROM crm_opportunities opportunity JOIN crm_leads lead ON lead.id=opportunity.lead_id\n  LEFT JOIN crm_accounts account ON account.id=lead.account_id AND account.company_id=lead.company_id\n  LEFT JOIN crm_contacts contact ON contact.id=lead.contact_id AND contact.account_id=account.id AND contact.company_id=lead.company_id\n  WHERE opportunity.id=? AND lead.company_id=?",
  ],
];

describe('crm read-tail r36 byte-identical opportunity source anchor', () => {
  test.each(retired)('%s renders byte-identical read-only SQL', (_name, rendered, expected) => {
    expect(rendered).toBe(expected);
    expect(rendered).toContain('SELECT ');
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('owns the renderer once and composes it at both retired call sites', () => {
    const helper = src('read-tail-r36-sql.ts');
    expect(helper.match(/crmOpportunitySourceAnchorSql =/g)).toHaveLength(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);

    const purchase = src('attachment-purchases.ts'), image = src('attachment-read.ts');
    expect(purchase.match(/crmOpportunitySourceAnchorSql\(/g)).toHaveLength(1);
    expect(image.match(/crmOpportunitySourceAnchorSql\(/g)).toHaveLength(1);
    expect(purchase).not.toContain("SELECT jsonb_build_object('opportunity',to_jsonb(opportunity)-'version'");
    expect(image).not.toContain("SELECT jsonb_build_object('opportunity',to_jsonb(opportunity),");
  });

  test('keeps bind order unchanged at every retired site', () => {
    const purchase = src('attachment-purchases.ts'), image = src('attachment-read.ts');
    expect(purchase).toContain('`SELECT (${sourceQuery(target.kind)}) witness`).bind(...sourceArgs(actor,target))');
    expect(purchase).toContain('`(${sourceQuery(scope.target.kind)})::jsonb=?::jsonb`,[...sourceArgs(actor,scope.target),scope.witness]');
    expect(image).toContain('crmOpportunitySourceAnchorSql(');
    expect(image).toContain(')).bind(id,companyId).first<Source>();');
  });

  test('leaves locks, command receipts, native kinds and r8-r35 helpers untouched', () => {
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
    ] as const) expect(src(file)).toContain(token);
  });

  test('retirement:ts-statements stays strictly below the r35 BEFORE=114 baseline', () => {
    expect(totalSelects()).toBe(95);
    expect(totalSelects()).toBeLessThan(114);
  });
});
