import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { crmProjectionFromSql } from '../../../packages/server-crm/src/adapters/read-tail-r46-sql';
import {
  crmCaseEventReadSql,
  crmOpportunityProbeSql,
} from '../../../packages/server-crm/src/adapters/read-tail-r28-sql';
import { crmOpportunitySourceAnchorSql } from '../../../packages/server-crm/src/adapters/read-tail-r36-sql';
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

/** Retired inline SELECT heads and the exact text each one rendered at the
 *  r45 AFTER=57 baseline. Expected strings are written out, not derived from
 *  the helper, so a head, tail, bind or whitespace regression fails the byte
 *  oracle. The renderer only moves the shared "SELECT <projection> FROM" scaffold;
 *  every projection, predicate, ORDER tail and bind stays at its call site. */
const retired = [
  {
    name: 'r28 case-event history head',
    file: 'read-tail-r28-sql.ts',
    rendered: crmCaseEventReadSql('*', 'lead_id=?', ' ORDER BY created_at DESC,id DESC'),
    sql: 'SELECT * FROM crm_case_events WHERE lead_id=? ORDER BY created_at DESC,id DESC',
  },
  {
    name: 'r28 case-event create snapshot head',
    file: 'read-tail-r28-sql.ts',
    rendered: crmCaseEventReadSql('snapshot_json::text snapshot_json', "id=? AND lead_id=? AND action='create' AND actor_id=?"),
    sql: "SELECT snapshot_json::text snapshot_json FROM crm_case_events WHERE id=? AND lead_id=? AND action='create' AND actor_id=?",
  },
  {
    name: 'r28 opportunity evidence source probe',
    file: 'read-tail-r28-sql.ts',
    rendered: crmOpportunityProbeSql('lead_id', 'id=?'),
    sql: 'SELECT lead_id FROM crm_opportunities WHERE id=?',
  },
  {
    name: 'r28 opportunity existence probe',
    file: 'read-tail-r28-sql.ts',
    rendered: crmOpportunityProbeSql('1', 'lead_id=?'),
    sql: 'SELECT 1 FROM crm_opportunities WHERE lead_id=?',
  },
  {
    name: 'r36 opportunity-with-lead source anchor',
    file: 'read-tail-r36-sql.ts',
    rendered: crmOpportunitySourceAnchorSql('1', ' AND lead.company_id=?'),
    sql: 'SELECT 1 FROM crm_opportunities opportunity JOIN crm_leads lead ON lead.id=opportunity.lead_id AND lead.company_id=?',
  },
  {
    name: 'r37 qualified Sales opportunity head',
    file: 'read-tail-r37-sql.ts',
    rendered: crmSalesOpportunitySql('1', 'crm_lead_activities c JOIN crm_leads l ON l.id=c.lead_id', 'c.id='),
    sql: 'SELECT 1 FROM crm_lead_activities c JOIN crm_leads l ON l.id=c.lead_id WHERE c.id=',
  },
] as const;

describe('crm read-tail r46 byte-identical projection/FROM head', () => {
  test.each(retired)('$name reproduces its retired inline literal byte-for-byte', ({ rendered, sql }) => {
    expect(rendered).toBe(sql);
    expect((rendered.match(/SELECT /g) ?? []).length).toBe(1);
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('crmProjectionFromSql owns exactly one read token and preserves projection/FROM verbatim', () => {
    const helper = src('read-tail-r46-sql.ts');
    expect(helper.match(/crmProjectionFromSql =/g)).toHaveLength(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);
    expect(crmProjectionFromSql('id', 'accounts a WHERE a.id=?')).toBe('SELECT id FROM accounts a WHERE a.id=?');
    expect(crmProjectionFromSql('1', 'crm_leads lead WHERE lead.id=? ORDER BY lead.id')).toBe(
      'SELECT 1 FROM crm_leads lead WHERE lead.id=? ORDER BY lead.id',
    );
  });

  test('consumer helpers route their former inline heads through the one renderer', () => {
    const r28 = src('read-tail-r28-sql.ts'), r36 = src('read-tail-r36-sql.ts'), r37 = src('read-tail-r37-sql.ts');
    for (const [name, text] of [['r28', r28], ['r36', r36], ['r37', r37]] as const) {
      expect(text).toContain("import { crmProjectionFromSql } from './read-tail-r46-sql'");
      expect(text).toContain('crmProjectionFromSql(');
      expect(text).not.toContain('`SELECT ${projection} FROM crm_case_events WHERE');
      expect(text).not.toContain('`SELECT ${projection} FROM crm_opportunities WHERE');
      expect(text).not.toContain('`SELECT ${projection} FROM crm_opportunities opportunity JOIN crm_leads');
      expect(text).not.toContain('`SELECT ${projection} FROM ${from} WHERE ${tail}`');
      void name;
    }
    expect(r28.match(/crmProjectionFromSql\(/g)).toHaveLength(2);
    expect(r36.match(/crmProjectionFromSql\(/g)).toHaveLength(1);
    expect(r37.match(/crmProjectionFromSql\(/g)).toHaveLength(1);
    // r37 keeps the two read heads it did not retire this round.
    expect((r28.match(/SELECT /g) ?? []).length).toBe(0);
    expect((r36.match(/SELECT /g) ?? []).length).toBe(0);
    expect((r37.match(/SELECT /g) ?? []).length).toBe(1);
  });

  test('every retired call site keeps its projection, tail, binds and ORDER BY verbatim', () => {
    const leads = src('leads.ts'), purchases = src('purchases.ts');
    expect(leads).toContain("crmCaseEventReadSql('*','lead_id=?',' ORDER BY created_at DESC,id DESC')");
    expect(purchases).toContain("crmCaseEventReadSql('snapshot_json::text snapshot_json',");
    expect(leads).toContain("crmOpportunityProbeSql('lead_id','id=?')");
    expect(purchases).toContain("crmOpportunityProbeSql('1','lead_id=?')");
    expect(leads).not.toContain('SELECT * FROM crm_case_events WHERE lead_id=?');
    expect(purchases).not.toContain('SELECT snapshot_json::text snapshot_json FROM crm_case_events');
    expect(leads).not.toContain('SELECT lead_id FROM crm_opportunities WHERE id=?');
    expect(purchases).not.toContain('SELECT 1 FROM crm_opportunities WHERE lead_id=?');

    expect(src('attachment-purchases.ts')).toContain('crmOpportunitySourceAnchorSql(');
    expect(src('attachment-read.ts')).toContain('crmOpportunitySourceAnchorSql(');
    expect(purchases).toContain('crmSalesOpportunitySql(');
    expect(src('sales-projection.ts')).toContain('crmSalesOpportunitySql(');
    expect(src('record-purchases.ts')).toContain('crmSalesOpportunitySql(');
  });

  test('leaves crmSelectOneSql/crmUnionAllSql, locks, receipts and cross-owner exports untouched', () => {
    expect(src('read-tail-r42-sql.ts')).toContain('export const crmSelectOneSql = (body: string) => `SELECT 1 FROM ${body}`');
    expect(src('read-tail-r44-sql.ts')).toContain('export const crmUnionAllSql = (body: string) => `UNION ALL SELECT ${body}`');
    expect(src('sales-package-obligations.ts')).toContain('crm_lead_activity_events event JOIN command_receipts receipt');
    expect(src('purchases.ts')).toContain(
      'LOCK TABLE crm_accounts,crm_contacts,crm_guest_links,crm_guest_sharing_events,crm_leads,crm_opportunities,crm_case_events,crm_lead_activities,crm_lead_activity_events IN SHARE ROW EXCLUSIVE MODE',
    );
    expect(src('purchases.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(src('purchases.ts')).toContain('@erpx/server-pms/public/crm-purchase-sources');
  });

  test('retirement:ts-statements drops from the r45 AFTER=57 to 54', () => {
    expect(totalSelects()).toBe(49);
    expect(totalSelects()).toBeLessThan(57);
  });
});
