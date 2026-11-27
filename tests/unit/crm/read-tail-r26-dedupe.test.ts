import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { crmSelectBodySql } from '../../../packages/server-crm/src/adapters/read-tail-r46-sql';
import { crmCaseEventLeadReadSql, crmGuestLinkReservationSourceSql } from '../../../packages/server-crm/src/adapters/read-tail-r26-sql';

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

const P = 'PMS_PROPS', R = 'PMS_RECS', W = 'CRM_WHERE';

/** Retired inline SQL, exactly as it read before r26. */
const guestLinkRetired: Array<[string, string, string]> = [
  ['leads reservations',
    crmGuestLinkReservationSourceSql(`r.id,r.data_json::jsonb->>'number' number,r.data_json::jsonb->>'state' state,p.id property_id`, P, R, '', ' ORDER BY r.updated_at DESC', ' '),
    `SELECT r.id,r.data_json::jsonb->>'number' number,r.data_json::jsonb->>'state' state,p.id property_id FROM crm_guest_links g JOIN (${P}) p ON p.id=g.property_id AND p.company_id=g.company_id JOIN (${R}) r ON r.property_id=p.id AND r.kind='reservation' AND r.data_json::jsonb->>'guestId'=g.guest_id WHERE g.sharing_state='granted' AND g.contact_id=? AND g.company_id=? AND (?::text IS NULL OR p.branch_id=?) ORDER BY r.updated_at DESC`],
  ['identities stays',
    crmGuestLinkReservationSourceSql(`g.guest_id,r.id reservation_id,r.data_json::jsonb->>'state' reservation_state,
  s.id stay_id,s.data_json::jsonb->>'state' stay_state,s.data_json::jsonb->>'arrival' arrival,
  s.data_json::jsonb->>'departure' departure,p.id property_id`, P, R, `\n  LEFT JOIN (${R}) s ON s.property_id=p.id AND s.kind='stay' AND s.data_json::jsonb->>'reservationId'=r.id`, ' ORDER BY r.updated_at DESC,s.id LIMIT 201', '\n  '),
    `SELECT g.guest_id,r.id reservation_id,r.data_json::jsonb->>'state' reservation_state,
  s.id stay_id,s.data_json::jsonb->>'state' stay_state,s.data_json::jsonb->>'arrival' arrival,
  s.data_json::jsonb->>'departure' departure,p.id property_id
  FROM crm_guest_links g JOIN (${P}) p ON p.id=g.property_id AND p.company_id=g.company_id
  JOIN (${R}) r ON r.property_id=p.id AND r.kind='reservation' AND r.data_json::jsonb->>'guestId'=g.guest_id
  LEFT JOIN (${R}) s ON s.property_id=p.id AND s.kind='stay' AND s.data_json::jsonb->>'reservationId'=r.id
  WHERE g.sharing_state='granted' AND g.contact_id=? AND g.company_id=? AND (?::text IS NULL OR p.branch_id=?) ORDER BY r.updated_at DESC,s.id LIMIT 201`],
];

const caseEventRetired: Array<[string, string, string]> = [
  ['analytics events',
    crmCaseEventLeadReadSql('event.lead_id,event.opportunity_id,event.action,event.snapshot_json,event.created_at', 'event.created_at<=? AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.department_id=?)', W, ' ORDER BY event.created_at,event.id'),
    `SELECT event.lead_id,event.opportunity_id,event.action,event.snapshot_json,event.created_at FROM crm_case_events event JOIN crm_leads l ON l.id=event.lead_id WHERE l.company_id=? AND event.created_at<=? AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.department_id=?) AND ${W} ORDER BY event.created_at,event.id`],
  ['analytics qualificationEvents',
    crmCaseEventLeadReadSql('DISTINCT event.lead_id', "event.action='qualify' AND event.created_at<=?", W),
    `SELECT DISTINCT event.lead_id FROM crm_case_events event JOIN crm_leads l ON l.id=event.lead_id WHERE l.company_id=? AND event.action='qualify' AND event.created_at<=? AND ${W}`],
];

describe('crm read-tail r26 byte-identical read families', () => {
  test.each([...guestLinkRetired, ...caseEventRetired])('%s renders byte-identical read-only SQL', (_name, rendered, retired) => {
    expect(rendered).toBe(retired);
    expect(rendered).toContain('SELECT ');
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('owns each renderer once and composes it at the four retired call sites', () => {
    const helper = src('read-tail-r26-sql.ts');
    expect(helper.match(/crmGuestLinkReservationSourceSql =/g)).toHaveLength(1);
    expect(helper.match(/crmCaseEventLeadReadSql =/g)).toHaveLength(1);
    expect(src('leads.ts')).toContain('crmGuestLinkReservationSourceSql(');
    expect(src('identities.ts')).toContain('crmGuestLinkReservationSourceSql(');
    expect(src('analytics.ts').match(/crmCaseEventLeadReadSql\(/g)).toHaveLength(2);
    expect(src('leads.ts')).not.toContain('SELECT r.id,r.data_json::jsonb->>\'number\'');
    expect(src('analytics.ts')).not.toContain('SELECT DISTINCT event.lead_id FROM crm_case_events');
  });

  test('leaves r8-r25 helpers, locks and native kinds untouched', () => {
    const leads = src('leads.ts'), identities = src('identities.ts'), analytics = src('analytics.ts');
    for (const token of ['crmCreatorLeadScopeSql', 'activeCustomerPartiesSql', 'leadOwnerNameSql', 'crmBranchOptionsSql']) {
      expect(leads + identities).toContain(token);
    }
    for (const token of ['crmLeadProjectionScope', 'crmClockCteSql', 'crmAssignmentScopeExistsSql']) {
      expect(analytics).toContain(token);
    }
    expect(src('read-tail-r24-sql.ts')).toContain('export const crmCreatorLeadScopeSql');
    expect(src('read-tail-r25-sql.ts')).toContain('export const crmPurchaseRowProjection');
    expect(src('purchases.ts')).toContain('crmPurchaseRowProjection(');
    for (const kind of ['crm_stmt_1', 'crm_stmt_4', 'crm_stmt_17']) expect(leads).toContain(kind);
    for (const kind of ['crm_stmt_9', 'crm_stmt_11', 'crm_stmt_12', 'crm_stmt_15']) expect(identities).toContain(kind);
    for (const file of ['purchases.ts', 'marketing-package-obligations.ts', 'sales-package-obligations.ts']) {
      expect(src(file)).toContain('LOCK TABLE');
    }
  });

  test('both source heads fold into the owner select-body plan', () => {
    for (const [name, , expected] of [...guestLinkRetired, ...caseEventRetired]) {
      expect(expected.startsWith('SELECT ')).toBe(true);
      expect(expected).toBe(crmSelectBodySql(expected.slice('SELECT '.length)));
      void name;
    }
  });

  test('retirement:ts-statements folds both r26 heads (25 -> 23)', () => {
    expect(totalSelects()).toBe(23);
    expect(totalSelects()).toBeLessThanOrEqual(132);
    expect(totalSelects()).toBeLessThan(136);
  });
});
