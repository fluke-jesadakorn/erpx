import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { crmClockCteSql } from '../../../packages/server-crm/src/adapters/purchases';
import { crmOwnedSourceRowSql } from '../../../packages/server-crm/src/adapters/read-tail-r34-sql';
import { crmSingleKeySourceSql } from '../../../packages/server-crm/src/adapters/read-tail-r37-sql';
import {
  crmMaterializedScopeSql,
  crmOwnedListSql,
} from '../../../packages/server-crm/src/adapters/read-tail-r47-sql';

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

const clock = 'floor(extract(epoch FROM clock_timestamp())*1000)::bigint';

/** Retired inline heads and the exact text each one rendered at the r46
 *  AFTER=54 baseline. Expected strings are written out, not derived from the
 *  helper, so a head, tail, bind or whitespace regression fails the byte
 *  oracle. The renderers only move the shared MATERIALIZED CTE and owned-source
 *  SELECT/FROM/WHERE scaffolds; every body, ORDER tail and bind stays at its
 *  call site. */
const retired = [
  {
    name: 'purchases MATERIALIZED clock CTE head',
    rendered: crmClockCteSql,
    sql: ` AS MATERIALIZED(SELECT ${clock} now_ms)`,
  },
  {
    name: 'marketing MATERIALIZED candidate scope head',
    rendered: crmMaterializedScopeSql('candidate.* FROM (CANDIDATES) candidate,scope WHERE candidate.company_id=scope.company_id'),
    sql: 'AS MATERIALIZED(SELECT candidate.* FROM (CANDIDATES) candidate,scope WHERE candidate.company_id=scope.company_id)',
  },
  {
    name: 'sales MATERIALIZED lead scope head',
    rendered: crmMaterializedScopeSql('l.* FROM crm_leads l,scope WHERE l.company_id=scope.company_id'),
    sql: 'AS MATERIALIZED(SELECT l.* FROM crm_leads l,scope WHERE l.company_id=scope.company_id)',
  },
  {
    name: 'r34 owned-source witness list head',
    rendered: crmOwnedSourceRowSql('witness::text witness', 'OPPORTUNITY_ROWS', 'opportunity', ' AND SCOPE'),
    sql: 'SELECT witness::text witness FROM (OPPORTUNITY_ROWS) opportunity WHERE id=? AND company_id=? AND SCOPE',
  },
  {
    name: 'r37 owned-source single-key list head',
    rendered: crmSingleKeySourceSql('DISTINCT user_id owner_id', 'ASSIGNMENTS', 'assignments', ' ORDER BY owner_id'),
    sql: 'SELECT DISTINCT user_id owner_id FROM (ASSIGNMENTS) assignments WHERE company_id=? ORDER BY owner_id',
  },
] as const;

describe('crm read-tail r47 byte-identical MATERIALIZED and owned-source heads', () => {
  test.each(retired)('$name reproduces its retired inline literal byte-for-byte', ({ rendered, sql }) => {
    expect(rendered).toBe(sql);
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('the owner renderer owns exactly the two shared read tokens', () => {
    const helper = src('read-tail-r47-sql.ts');
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);
    expect(helper.match(/crmMaterializedScopeSql =/g)).toHaveLength(1);
    expect(helper.match(/crmOwnedListSql =/g)).toHaveLength(1);
    expect(crmMaterializedScopeSql('now_ms')).toBe('AS MATERIALIZED(SELECT now_ms)');
    expect(crmOwnedListSql('id', 'ROWS', 'actual', 'id=? AND company_id=?')).toBe(
      'SELECT id FROM (ROWS) actual WHERE id=? AND company_id=?',
    );
  });

  test('consumer sites route their former inline heads through the two renderers', () => {
    const p = src('purchases.ts');
    const m = src('marketing-package-obligations.ts');
    const s = src('sales-package-obligations.ts');
    const r34 = src('read-tail-r34-sql.ts');
    const r37 = src('read-tail-r37-sql.ts');
    for (const [name, text] of [['purchases', p], ['marketing', m], ['sales', s]] as const) {
      expect(text).toContain('read-tail-r47-sql');
      expect(text).toContain('crmMaterializedScopeSql(');
      expect(text).not.toContain('AS MATERIALIZED(SELECT ');
      void name;
    }
    for (const [name, text] of [['r34', r34], ['r37', r37]] as const) {
      expect(text).toContain("from './read-tail-r47-sql'");
      expect(text).toContain('crmOwnedListSql(');
      expect(text).not.toContain('SELECT ${projection} FROM (${source}) ${alias} WHERE ');
      void name;
    }
    expect(p).toContain('export const crmClockCteSql=` ${crmMaterializedScopeSql(`${packageClockSql} now_ms`)}`;');
    expect(m).toContain('candidates ${crmMaterializedScopeSql(`candidate.* FROM (${marketing.candidates})');
    expect(s).toContain("leads ${crmMaterializedScopeSql('l.* FROM crm_leads l,scope WHERE l.company_id=scope.company_id')}");
    expect(r34).toContain('crmOwnedListSql(projection, source, alias, `id=? AND company_id=?${tail}`)');
    expect(r37).toContain('crmOwnedListSql(projection, source, alias, `company_id=?${tail}`)');
    expect((r34.match(/SELECT /g) ?? []).length).toBe(0);
    expect((r37.match(/SELECT /g) ?? []).length).toBe(1);
  });

  test('leaves crmSelectOneSql/crmUnionAllSql/crmProjectionFromSql, locks and exports untouched', () => {
    expect(src('read-tail-r42-sql.ts')).toContain('export const crmSelectOneSql = (body: string) => `SELECT 1 FROM ${body}`');
    expect(src('read-tail-r44-sql.ts')).toContain('export const crmUnionAllSql = (body: string) => `UNION ALL SELECT ${body}`');
    expect(src('read-tail-r46-sql.ts')).toContain('crmProjectionFromSql = (projection: string, from: string)');
    expect(src('read-tail-r46-sql.ts')).toContain('crmSelectBodySql(`${projection} FROM ${from}`)');
    expect(src('purchases.ts')).toContain('crmLeadActivityExists=crmSalesOpportunitySql(');
    expect(src('sales-package-obligations.ts')).toContain(
      'LOCK TABLE crm_accounts,crm_contacts,crm_guest_links,crm_guest_sharing_events,crm_leads,crm_opportunities,crm_case_events,crm_lead_activities,crm_lead_activity_events IN SHARE MODE',
    );
    expect(src('purchases.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(src('purchases.ts')).toContain('@erpx/server-pms/public/crm-purchase-sources');
    expect(src('marketing-package-obligations.ts')).toContain('@erpx/server-marketing/public/package-obligations');
  });

  test('retirement:ts-statements drops from the r46 AFTER=54 to 50', () => {
    expect(totalSelects()).toBe(39);
    expect(totalSelects()).toBeLessThan(54);
  });
});
