import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { crmHotelReservationSources } from '@erpx/server-pms/public/crm-purchase-sources';
import { crmCaseEventReceiptWitnessSql, crmHotelReservationProbeSql, crmPurchaseOpportunityRows } from '../../../packages/server-crm/src/adapters/purchases';
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

/** The four owner-local probes and the SQL they rendered at the r44 AFTER=61
 *  baseline. Expected strings are written out, not derived from the helper, so
 *  a head, trailing-tail or whitespace regression fails the byte oracle. */
const retired = [
  {
    name: 'purchases guest-link EXISTS',
    file: 'purchases.ts',
    prefix: ' AND EXISTS(${crmSelectOneSql(`',
    body: 'crm_guest_links guest_link WHERE guest_link.contact_id=native.contact_id AND guest_link.company_id=lead.company_id AND guest_link.guest_id=reservation.guest_id AND guest_link.property_id=reservation.property_id',
    sql: 'SELECT 1 FROM crm_guest_links guest_link WHERE guest_link.contact_id=native.contact_id AND guest_link.company_id=lead.company_id AND guest_link.guest_id=reservation.guest_id AND guest_link.property_id=reservation.property_id',
    rendered: 'EXISTS(SELECT 1 FROM crm_guest_links guest_link WHERE guest_link.contact_id=native.contact_id AND guest_link.company_id=lead.company_id AND guest_link.guest_id=reservation.guest_id AND guest_link.property_id=reservation.property_id)',
    old: 'EXISTS(SELECT 1 FROM crm_guest_links guest_link WHERE guest_link.contact_id=',
  },
  {
    name: 'hotel reservation probe (callers append their own tail)',
    file: 'purchases.ts',
    prefix: 'export const crmHotelReservationProbeSql=crmSelectOneSql(`',
    body: '(${crmHotelReservationSources}) reservation',
    sql: 'SELECT 1 FROM (${crmHotelReservationSources}) reservation',
    rendered: 'SELECT 1 FROM (${crmHotelReservationSources}) reservation',
    old: '`SELECT 1 FROM (${crmHotelReservationSources}) reservation`',
  },
  {
    name: 'case-event receipt witness (trailing || kept verbatim)',
    file: 'purchases.ts',
    prefix: 'export const crmCaseEventReceiptWitnessSql=crmSelectOneSql(`',
    body: "crm_case_events event JOIN command_receipts receipt ON event.id='crm-event-'||receipt.command_id||",
    sql: "SELECT 1 FROM crm_case_events event JOIN command_receipts receipt ON event.id='crm-event-'||receipt.command_id||",
    rendered: "SELECT 1 FROM crm_case_events event JOIN command_receipts receipt ON event.id='crm-event-'||receipt.command_id||",
    old: '`SELECT 1 FROM crm_case_events event JOIN command_receipts receipt ON',
  },
  {
    name: 'sales lead-activity NOT EXISTS',
    file: 'sales-package-obligations.ts',
    prefix: 'OR NOT EXISTS(${crmSelectOneSql(`',
    body: `crm_lead_activity_events event JOIN command_receipts receipt ON event.id='crm-activity-event-'||receipt.command_id AND receipt.record_id=activity.id AND receipt.operation='crm.activity.'||event.action AND receipt.actor_id=event.actor_id AND receipt.authority_json::jsonb->>'companyId'=activity.company_id WHERE event.activity_id=activity.id AND event.version=activity.version AND event.action=CASE activity.status WHEN 'completed' THEN 'complete' WHEN 'cancelled' THEN 'cancel' END AND event.owner_id=activity.owner_id AND event.due_date=activity.due_date AND receipt.result_json::jsonb->>'version'=activity.version::text`,
    sql: `SELECT 1 FROM crm_lead_activity_events event JOIN command_receipts receipt ON event.id='crm-activity-event-'||receipt.command_id AND receipt.record_id=activity.id AND receipt.operation='crm.activity.'||event.action AND receipt.actor_id=event.actor_id AND receipt.authority_json::jsonb->>'companyId'=activity.company_id WHERE event.activity_id=activity.id AND event.version=activity.version AND event.action=CASE activity.status WHEN 'completed' THEN 'complete' WHEN 'cancelled' THEN 'cancel' END AND event.owner_id=activity.owner_id AND event.due_date=activity.due_date AND receipt.result_json::jsonb->>'version'=activity.version::text`,
    rendered: `NOT EXISTS(SELECT 1 FROM crm_lead_activity_events event JOIN command_receipts receipt ON event.id='crm-activity-event-'||receipt.command_id AND receipt.record_id=activity.id AND receipt.operation='crm.activity.'||event.action AND receipt.actor_id=event.actor_id AND receipt.authority_json::jsonb->>'companyId'=activity.company_id WHERE event.activity_id=activity.id AND event.version=activity.version AND event.action=CASE activity.status WHEN 'completed' THEN 'complete' WHEN 'cancelled' THEN 'cancel' END AND event.owner_id=activity.owner_id AND event.due_date=activity.due_date AND receipt.result_json::jsonb->>'version'=activity.version::text)`,
    old: 'NOT EXISTS(SELECT 1 FROM crm_lead_activity_events event JOIN command_receipts receipt ON',
  },
] as const;

const site = (name: string) => {
  const found = retired.find(value => value.name === name);
  if (!found) throw Error(`missing oracle: ${name}`);
  return found;
};

/** Body between a unique call prefix and the closing backtick of its argument. */
const bodyAt = (text: string, prefix: string) => {
  const i = text.indexOf(prefix);
  if (i < 0) throw Error(`missing call prefix: ${prefix}`);
  const start = i + prefix.length;
  const end = text.indexOf('`)', start);
  if (end < 0) throw Error('missing call close');
  return text.slice(start, end);
};

describe('crm read-tail r45 byte-identical SELECT 1 probe reuse', () => {
  test.each(retired)('$name reproduces its r44 BEFORE literal byte-for-byte', ({ body, sql, rendered }) => {
    expect(crmSelectOneSql(body)).toBe(sql);
    const wrapper = rendered.match(/^(NOT )?EXISTS\(/)?.[0] ?? '';
    expect(rendered).toBe(wrapper + sql + (wrapper ? ')' : ''));
    expect((rendered.match(/SELECT /g) ?? []).length).toBe(1);
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('owns exactly one renderer with a single read token', () => {
    const helper = src('read-tail-r42-sql.ts');
    expect((helper.match(/crmSelectOneSql =/g) ?? []).length).toBe(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);
  });

  test('each source body is verbatim and its inline literal is retired', () => {
    for (const { file, prefix, body, rendered, old } of retired) {
      const text = src(file);
      expect(bodyAt(text, prefix)).toBe(body);
      expect(text).toContain(prefix + body + '`)');
      expect(text).not.toContain(old);
      expect(text).not.toContain(rendered.replace(/^(NOT )?EXISTS\(/, '').replace(/\)$/, ''));
    }
  });

  test('exported probes render through the helper at runtime', () => {
    const witness = site('case-event receipt witness (trailing || kept verbatim)');
    const guest = site('purchases guest-link EXISTS');
    expect(crmHotelReservationProbeSql.startsWith('SELECT 1 FROM (')).toBe(true);
    expect(crmHotelReservationProbeSql.endsWith(') reservation')).toBe(true);
    expect(crmHotelReservationProbeSql).toBe(crmSelectOneSql(`(${crmHotelReservationSources}) reservation`));
    expect(crmCaseEventReceiptWitnessSql).toBe(witness.rendered);
    expect(crmCaseEventReceiptWitnessSql.endsWith('||')).toBe(true);
    expect(crmPurchaseOpportunityRows.match(/EXISTS\(SELECT 1 FROM crm_guest_links guest_link[^)]*\)/)?.[0]).toBe(guest.rendered);
  });

  test('keeps caller tails, the trailing || and every bind order verbatim', () => {
    const sales = src('sales-package-obligations.ts');
    expect(sales).toContain("EXISTS(${crmCaseEventReceiptWitnessSql}'-lead' AND receipt.record_id=l.id AND receipt.operation='crm.lead.disqualify'");
    expect(sales).toContain("EXISTS(${crmCaseEventReceiptWitnessSql}'-deal' AND receipt.record_id=l.id AND receipt.actor_id=event.actor_id");
    expect(sales).toContain('EXISTS(${crmHotelReservationProbeSql} JOIN crm_guest_links guest ON guest.guest_id=reservation.guest_id');
    expect(src('marketing-package-obligations.ts')).toContain('EXISTS(${crmHotelReservationProbeSql} JOIN crm_opportunities opportunity');
    expect(sales).toContain('.bind(input.companyId).all<{kind:string;id:string;fact:unknown;pending:boolean}>()).results;');
    expect(src('purchases.ts')).toContain('(kind===\'lead\'?` AND (NOT EXISTS(${crmSelectOneSql(');
  });

  test('leaves the no-space attachment probe and direct command-receipt reads untouched', () => {
    expect(src('attachment-read.ts')).toContain('`SELECT 1 FROM${actorScopeSql}workforce_actor_scope WHERE ${current.where}`');
    expect(src('purchases.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
  });

  test('reuses the one renderer at every retired call site', () => {
    expect((src('purchases.ts').match(/crmSelectOneSql\(/g) ?? []).length).toBe(6);
    expect((src('sales-package-obligations.ts').match(/crmSelectOneSql\(/g) ?? []).length).toBe(4);
  });

  test('retirement:ts-statements drops from the r44 AFTER=61 to 57', () => {
    expect(totalSelects()).toBe(30);
    expect(totalSelects()).toBeLessThan(61);
  });
});
