import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { crmCompanyScopeSql } from '../../../packages/server-crm/src/adapters/read-tail-r48-sql';

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

/** Retired inline company-scope heads and the exact text each one rendered at
 *  the r47 AFTER=51 baseline. Expected strings are written out, not derived
 *  from the helper, so a head, tail, bind or whitespace regression fails the
 *  byte oracle. The renderer moves only the shared `SELECT ?::text company_id`
 *  prefix; every column tail, wrapper and bind stays at its call site. */
const retired = [
  {
    name: 'purchases actor scope row',
    rendered: `(${crmCompanyScopeSql(',?::text branch_id,?::text department_id')})`,
    sql: '(SELECT ?::text company_id,?::text branch_id,?::text department_id)',
    binds: 3,
  },
  {
    name: 'purchases obligation scope CTE',
    rendered: `WITH scope AS(${crmCompanyScopeSql('')}),`,
    sql: 'WITH scope AS(SELECT ?::text company_id),',
    binds: 1,
  },
] as const;

describe('crm read-tail r48 byte-identical company-scope read head', () => {
  test.each(retired)('$name reproduces its retired inline literal byte-for-byte', ({ rendered, sql, binds }) => {
    expect(rendered).toBe(sql);
    expect(rendered.match(/\?/g)).toHaveLength(binds);
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('the owner renderer owns exactly the one shared read token', () => {
    const helper = src('read-tail-r48-sql.ts');
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);
    expect(helper.match(/crmCompanyScopeSql =/g)).toHaveLength(1);
    expect(crmCompanyScopeSql('')).toBe('SELECT ?::text company_id');
    expect(crmCompanyScopeSql(',?::text branch_id,?::text department_id')).toBe(
      'SELECT ?::text company_id,?::text branch_id,?::text department_id',
    );
  });

  test('both consumer sites route their former inline head through the renderer', () => {
    const p = src('purchases.ts');
    expect(p).toContain('read-tail-r48-sql');
    expect(p).toContain('crmCompanyScopeSql(');
    expect(p).toContain("actorScopeSql=`(${crmCompanyScopeSql(',?::text branch_id,?::text department_id')})`;");
    expect(p).toContain("crmObligationScopeSql=`WITH scope AS(${crmCompanyScopeSql('')}),`;");
    expect(p).not.toContain('`SELECT ?::text company_id,?::text branch_id,?::text department_id`');
    expect(p).not.toContain('`WITH scope AS(SELECT ?::text company_id),`');
  });

  test('leaves r42-r47 renderers, locks, receipt reads and cross-owner exports untouched', () => {
    expect(src('read-tail-r42-sql.ts')).toContain('export const crmSelectOneSql = (body: string) => `SELECT 1 FROM ${body}`');
    expect(src('read-tail-r43-sql.ts')).toContain('export const crmScalarSubquerySql = (projection: string, body: string)');
    expect(src('read-tail-r44-sql.ts')).toContain('export const crmUnionAllSql = (body: string) => `UNION ALL SELECT ${body}`');
    expect(src('read-tail-r46-sql.ts')).toContain('crmProjectionFromSql = (projection: string, from: string)');
    expect(src('read-tail-r47-sql.ts')).toContain('crmOwnedListSql = (');
    expect(src('attachment-read.ts')).toContain('`SELECT 1 FROM${actorScopeSql}workforce_actor_scope WHERE ${current.where}`');
    expect(src('attachment-purchases.ts')).toContain(
      'crmScalarSubquerySql(assignmentReadSql("crm_file_scope","commerce.sales"),`${actorScopeSql} crm_file_scope`)',
    );
    expect(src('sales-package-obligations.ts')).toContain(
      'LOCK TABLE crm_accounts,crm_contacts,crm_guest_links,crm_guest_sharing_events,crm_leads,crm_opportunities,crm_case_events,crm_lead_activities,crm_lead_activity_events IN SHARE MODE',
    );
    expect(src('purchases.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(src('purchases.ts')).toContain('@erpx/server-finance/public/crm-purchase-sources');
    expect(src('marketing-package-obligations.ts')).toContain('@erpx/server-marketing/public/package-obligations');
  });

  test('retirement:ts-statements drops from the r48 AFTER=50 to 49', () => {
    expect(totalSelects()).toBe(41);
    expect(totalSelects()).toBeLessThan(51);
  });
});
