import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { describe, expect, test, vi } from 'vitest';
vi.mock('server-only', () => ({}));
import { commercialOwnerProjectionSources as commerceProjectionSources } from '../../../packages/server-commerce/src/public/commercial-owner-projection-sources';
import {
  crmCampaignOptionsSql,
  crmOpportunityOwnerGuardSql,
  crmOpportunityOwnerReadSql,
} from '../../../packages/server-crm/src/adapters/read-tail-r27-sql';
import { crmCommerceOrderReadSql } from '../../../packages/server-crm/src/adapters/read-tail-r32-sql';
import { crmContactAccountReadSql } from '../../../packages/server-crm/src/adapters/read-tail-r35-sql';
import { crmProjectionFromSql } from '../../../packages/server-crm/src/adapters/read-tail-r46-sql';
import {
  crmMaterializedScopeSql,
  crmOwnedListSql,
} from '../../../packages/server-crm/src/adapters/read-tail-r47-sql';
import { crmOwnedSourceRowSql } from '../../../packages/server-crm/src/adapters/read-tail-r34-sql';
import { crmSingleKeySourceSql } from '../../../packages/server-crm/src/adapters/read-tail-r37-sql';

const owner = resolve(__dirname, '../../../packages/server-crm/src');
const src = (name: string) => readFileSync(resolve(owner, 'adapters', name), 'utf8');
const sha = (value: string) => createHash('sha256').update(value).digest('hex');
const totalSelects = () => {
  let total = 0;
  for (const rel of readdirSync(owner, { recursive: true })) {
    if (!String(rel).endsWith('.ts')) continue;
    total += readFileSync(resolve(owner, String(rel)), 'utf8').match(/SELECT /g)?.length ?? 0;
  }
  return total;
};

const E = 'ELIG';
const ORDERS = commerceProjectionSources.commerce_orders;

/** Retired inline projection/FROM heads and the exact text each one rendered at
 *  the r51 AFTER=47 baseline. Expected strings are written out, not derived from
 *  the renderer, so a head, tail, alias, whitespace or bind regression fails the
 *  byte oracle. Only the shared `SELECT <projection> FROM ` prefix moved into the
 *  owner renderer; every projection, join, predicate, ORDER tail and bind stays
 *  at its call site. */
const retired = [
  {
    name: 'opportunity owner policy read',
    rendered: crmOpportunityOwnerReadSql(E),
    sql: `SELECT l.company_id,l.branch_id,l.department_id,l.owner_id,o.version,
      CASE WHEN ${E} THEN 1 ELSE 0 END owner_eligible
    FROM crm_opportunities o JOIN crm_leads l ON l.id=o.lead_id
    WHERE o.id=? AND l.company_id=?
      AND (?::text IS NULL OR l.branch_id=?)
      AND (?::text IS NULL OR l.department_id=?)`,
    binds: 6,
  },
  {
    name: 'opportunity owner version guard',
    rendered: crmOpportunityOwnerGuardSql(E),
    sql: `EXISTS(SELECT 1 FROM crm_opportunities o JOIN crm_leads l ON l.id=o.lead_id
      WHERE o.id=? AND l.company_id=? AND o.version=? AND l.owner_id=?
        AND (?::text IS NULL OR l.branch_id=?) AND (?::text IS NULL OR l.department_id=?)
        AND ${E})`,
    binds: 8,
  },
  {
    name: 'quotes commerce-order read',
    rendered: crmCommerceOrderReadSql('o.id,o.number,o.version,o.state', 'o', `WHERE o.company_id=? AND o.party_id=? AND o.branch_id=? AND o.department_id IS NOT DISTINCT FROM ?::text AND o.kind='quote' AND o.state='confirmed' ORDER BY o.updated_at DESC`),
    sql: `SELECT o.id,o.number,o.version,o.state FROM (${ORDERS}) o WHERE o.company_id=? AND o.party_id=? AND o.branch_id=? AND o.department_id IS NOT DISTINCT FROM ?::text AND o.kind='quote' AND o.state='confirmed' ORDER BY o.updated_at DESC`,
    binds: 4,
  },
  {
    name: 'identities contact list',
    rendered: crmContactAccountReadSql('c.id,c.account_id,c.name,c.detail,c.version', 'a',
      "WHERE c.company_id=? AND ${await crmNativeReadFilter('contact','c')} AND ${accountOrContactLeadScopeSql('contact_id','c.id','c.company_id')} ORDER BY c.updated_at DESC LIMIT 300"),
    sql: "SELECT c.id,c.account_id,c.name,c.detail,c.version FROM crm_contacts c JOIN crm_accounts a ON a.id=c.account_id WHERE c.company_id=? AND ${await crmNativeReadFilter('contact','c')} AND ${accountOrContactLeadScopeSql('contact_id','c.id','c.company_id')} ORDER BY c.updated_at DESC LIMIT 300",
    binds: 1,
  },
  {
    name: 'r34 owned-source witness list head',
    rendered: crmOwnedSourceRowSql('witness::text witness', 'OPPORTUNITY_ROWS', 'opportunity', ' AND SCOPE'),
    sql: 'SELECT witness::text witness FROM (OPPORTUNITY_ROWS) opportunity WHERE id=? AND company_id=? AND SCOPE',
    binds: 2,
  },
  {
    name: 'r37 owned-source single-key list head',
    rendered: crmSingleKeySourceSql('DISTINCT user_id owner_id', 'ASSIGNMENTS', 'assignments', ' ORDER BY owner_id'),
    sql: 'SELECT DISTINCT user_id owner_id FROM (ASSIGNMENTS) assignments WHERE company_id=? ORDER BY owner_id',
    binds: 1,
  },
  {
    name: 'r47 owned-list projection head',
    rendered: crmOwnedListSql('id', 'ROWS', 'actual', 'id=? AND company_id=?'),
    sql: 'SELECT id FROM (ROWS) actual WHERE id=? AND company_id=?',
    binds: 2,
  },
  {
    name: 'r27 campaign options projection',
    rendered: crmCampaignOptionsSql('SOURCE', 'campaign', true),
    sql: 'SELECT id,code name FROM (SOURCE) campaign WHERE company_id=? ORDER BY code',
    binds: 1,
  },
  {
    name: 'r47 MATERIALIZED scope head (unrelated head retained)',
    rendered: crmMaterializedScopeSql('l.* FROM crm_leads l,scope WHERE l.company_id=scope.company_id'),
    sql: 'AS MATERIALIZED(SELECT l.* FROM crm_leads l,scope WHERE l.company_id=scope.company_id)',
    binds: 0,
  },
] as const;

describe('crm read-tail r21 byte-identical projection/FROM head', () => {
  test.each(retired)('$name reproduces its retired inline literal byte-for-byte', ({ rendered, sql, binds }) => {
    expect(rendered).toBe(sql);
    expect(sha(rendered)).toBe(sha(sql));
    expect((rendered.match(/\?/g) ?? [])).toHaveLength(binds);
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('the owner renderer folds projection/FROM into the one shared select-body token', () => {
    const helper = src('read-tail-r46-sql.ts');
    expect(helper.match(/crmProjectionFromSql =/g)).toHaveLength(1);
    expect((helper.match(/SELECT /g) ?? []).length).toBe(1);
    expect(crmProjectionFromSql('id', 'accounts a WHERE a.id=?')).toBe('SELECT id FROM accounts a WHERE a.id=?');
    expect(crmProjectionFromSql('1', 'crm_leads lead WHERE lead.id=? ORDER BY lead.id')).toBe(
      'SELECT 1 FROM crm_leads lead WHERE lead.id=? ORDER BY lead.id',
    );
  });

  test('all four retired heads route through the renderer and no inline head remains', () => {
    for (const name of ['read-tail-r27-sql.ts', 'read-tail-r32-sql.ts', 'read-tail-r35-sql.ts', 'read-tail-r47-sql.ts']) {
      const text = src(name);
      expect(text).toContain('crmProjectionFromSql');
      expect(text).toContain("'./read-tail-r46-sql'");
      expect(text).toContain('crmProjectionFromSql(');
    }
    const r27 = src('read-tail-r27-sql.ts');
    const r32 = src('read-tail-r32-sql.ts');
    const r35 = src('read-tail-r35-sql.ts');
    const r47 = src('read-tail-r47-sql.ts');
    expect(r27).not.toContain('`SELECT ${projection} FROM crm_opportunities o JOIN crm_leads');
    expect(r32).not.toContain('`SELECT ${projection} FROM (${commerceProjectionSources.commerce_orders})');
    expect(r35).not.toContain('`SELECT ${projection} FROM crm_contacts c JOIN crm_accounts');
    expect(r47).not.toContain('`SELECT ${projection} FROM (${source}) ${alias} WHERE');
    expect((r27.match(/SELECT /g) ?? []).length).toBe(0);
    expect((r32.match(/SELECT /g) ?? []).length).toBe(0);
    expect((r35.match(/SELECT /g) ?? []).length).toBe(0);
    expect((r47.match(/SELECT /g) ?? []).length).toBe(0);
  });

  test('leaves r42-r51 renderers, locks, no-space probe, receipt reads, native kinds and cross-owner exports untouched', () => {
    expect(src('read-tail-r42-sql.ts')).toContain('export const crmSelectOneSql = (body: string) => `SELECT 1 FROM ${body}`');
    expect(src('read-tail-r43-sql.ts')).toContain('export const crmScalarSubquerySql = (projection: string, body: string)');
    expect(src('read-tail-r44-sql.ts')).toContain('export const crmUnionAllSql = (body: string) => `UNION ALL ${crmSelectBodySql(body)}`');
    expect(src('read-tail-r48-sql.ts')).toContain('crmCompanyScopeSql = (tail: string) => `SELECT ?::text company_id${tail}`');
    expect(src('read-tail-r49-sql.ts')).toContain('crmJsonbAggregateSql = (body: string) => crmSelectBodySql(`jsonb_agg(to_jsonb(${body}`)');
    expect(src('read-tail-r50-sql.ts')).toContain('export const crmSelectExprSql = (body: string) => crmSelectBodySql(`(${body}`)');
    expect(src('read-tail-r51-sql.ts')).toContain("export const crmLeadSelectSql = (tail: string) => crmSelectOneSql(`crm_leads ${tail}`)");
    expect(src('attachment-read.ts')).toContain('`SELECT 1 FROM${actorScopeSql}workforce_actor_scope WHERE ${current.where}`');
    expect(src('attachment-read.ts')).toContain('await lockQueuedCommandAuthority(database);');
    expect(src('purchases.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(src('purchases.ts')).toContain('@erpx/server-pms/public/crm-purchase-sources');
    expect(src('native-crm-read.ts')).toContain('export async function nativeCrmReadAll<T>');
    expect(src('native-crm-command.ts')).toContain('export async function nativeCrmExecute(');
  });

  test('retirement:ts-statements drops from the r51 AFTER=47 to 43', () => {
    expect(totalSelects()).toBe(34);
    expect(totalSelects()).toBeLessThan(47);
  });
});
