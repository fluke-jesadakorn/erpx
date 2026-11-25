import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { crmCreatorLeadScopeSql } from '../../../packages/server-crm/src/adapters/read-tail-r24-sql';
import { crmLeadEligibleOwnerSql, crmLeadOwnerGuardSql, crmWorkLeadOwnerProbeSql } from '../../../packages/server-crm/src/adapters/read-tail-r29-sql';
import { crmSelectOneSql } from '../../../packages/server-crm/src/adapters/read-tail-r42-sql';
import { crmLeadSelectSql } from '../../../packages/server-crm/src/adapters/read-tail-r51-sql';

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

/** Retired inline head and the exact rendered SQL each site produced at the
 *  r50 AFTER=48 baseline. Expected strings are written out, not derived from
 *  the helper, so a head, alias, predicate, wrapper or whitespace regression
 *  fails the byte oracle. Only the shared `SELECT 1 FROM crm_leads ` prefix
 *  moved into the renderer; every tail, wrapper and bind stays at its call site. */
const retired = [
  {
    name: 'creator-scope EXISTS guard (no company extra)',
    rendered: crmCreatorLeadScopeSql('acc', 'l', 'l.contact_id=c.id', ''),
    sql: '(?::text IS NULL OR acc.created_by=? OR EXISTS(SELECT 1 FROM crm_leads l WHERE l.contact_id=c.id AND l.branch_id=? AND (?::text IS NULL OR l.department_id=?)))',
    sha: 'e38baaadd55be25670b47bbeafbdc0212b81121c91b7ddd1e04b21ef2899944e',
    binds: 5,
  },
  {
    name: 'creator-scope EXISTS guard (with company extra and tail)',
    rendered: crmCreatorLeadScopeSql('a', 'lead', 'lead.account_id=a.id', 'a.company_id=lead.company_id', ' AND extra'),
    sql: '(?::text IS NULL OR a.created_by=? OR EXISTS(SELECT 1 FROM crm_leads lead WHERE lead.account_id=a.id AND a.company_id=lead.company_id AND lead.branch_id=? AND (?::text IS NULL OR lead.department_id=?) AND extra))',
    sha: '33e6a22e903d7193fcb8bb759a043ee0fd3ffb9570e34c693440ebc892e691d0',
    binds: 5,
  },
  {
    name: 'lead owner version guard',
    rendered: crmLeadOwnerGuardSql('LSCOPE', 'ELIG'),
    sql: 'EXISTS(SELECT 1 FROM crm_leads l WHERE l.id=? AND l.version=? AND l.owner_id=? AND LSCOPE AND ELIG)',
    sha: '18c1ca6d7a702ead80d2674c8ccea9202418230bde9303ad763eff1bfd71d5b2',
    binds: 3,
  },
  {
    name: 'purchase eligible-owner probe',
    rendered: crmLeadEligibleOwnerSql('ELIG'),
    sql: 'SELECT 1 FROM crm_leads l WHERE l.id=? AND l.company_id=? AND ELIG',
    sha: '045492a04b2d7596107c16dd6339793451818c2a6e136c08b62d0fa70187b538',
    binds: 2,
  },
  {
    name: 'work-filter lead owner probe',
    rendered: crmWorkLeadOwnerProbeSql('ELIG'),
    sql: "SELECT 1 FROM crm_leads l LEFT JOIN crm_opportunities opportunity ON opportunity.lead_id=l.id WHERE l.id=crm_work.target_id AND l.company_id=crm_work.company_id AND l.owner_id=? AND l.status<>'disqualified' AND COALESCE(opportunity.stage,'discovery') NOT IN ('won','lost') AND ELIG",
    sha: '00939b932f9acd6744cf690c281294529b42e5e2bdca2d634bd802157754315a',
    binds: 1,
  },
  {
    name: 'the 24-byte shared head',
    rendered: crmLeadSelectSql(''),
    sql: 'SELECT 1 FROM crm_leads ',
    sha: 'd0fe83f8b2d24d4f0d8e016316ffc9af2197e7ecfd06e53d5656c0b41d38e697',
    binds: 0,
  },
] as const;

describe('crm read-tail r51 byte-identical lead-select head', () => {
  test.each(retired)('$name reproduces its retired inline literal byte-for-byte', ({ rendered, sql, sha: digest, binds }) => {
    expect(rendered).toBe(sql);
    expect(sha(rendered)).toBe(digest);
    expect((rendered.match(/\?/g) ?? [])).toHaveLength(binds);
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('the lead-select head is folded into the r42 select-one renderer with no SQL token of its own', () => {
    const helper = src('read-tail-r51-sql.ts');
    expect((helper.match(/SELECT /g) ?? []).length).toBe(0);
    expect(helper).toContain("import { crmSelectOneSql } from './read-tail-r42-sql';");
    expect(helper.match(/crmLeadSelectSql =/g)).toHaveLength(1);
    expect(crmLeadSelectSql('')).toBe('SELECT 1 FROM crm_leads ');
    expect(crmLeadSelectSql('l WHERE l.id=?')).toBe('SELECT 1 FROM crm_leads l WHERE l.id=?');
  });

  test.each([
    '',
    'l WHERE l.id=?',
    'lead WHERE lead.company_id=? AND (?::text IS NULL OR lead.department_id=?) AND extra',
    'l WHERE l.contact_id=c.id AND l.branch_id=?',
  ])('the fold renders byte-identically to the owner select-one plan for tail %#', (tail) => {
    expect(crmLeadSelectSql(tail)).toBe(crmSelectOneSql(`crm_leads ${tail}`));
    expect(crmLeadSelectSql(tail)).toBe(`SELECT 1 FROM crm_leads ${tail}`);
  });

  test('both retired heads route through the renderer and no inline head remains', () => {
    const r24 = src('read-tail-r24-sql.ts');
    expect(r24).toContain("import {crmLeadSelectSql} from './read-tail-r51-sql';");
    expect(r24).toContain('EXISTS(${crmLeadSelectSql(`${leadAlias} WHERE ${fieldExpr}');
    expect(r24).not.toContain('EXISTS(SELECT 1 FROM crm_leads ${leadAlias} WHERE');
    expect((r24.match(/SELECT /g) ?? [])).toHaveLength(0);

    const r29 = src('read-tail-r29-sql.ts');
    expect(r29).toContain("import {crmLeadSelectSql} from './read-tail-r51-sql';");
    expect(r29).toContain('crmLeadSelectSql(`l${join} WHERE ${predicates} AND ${eligible}`)');
    expect(r29).not.toContain('`SELECT 1 FROM crm_leads l${join} WHERE');
    expect((r29.match(/SELECT /g) ?? [])).toHaveLength(1);
  });

  test('leaves r42-r50 renderers, locks, no-space probe, receipt reads, native kinds and cross-owner exports untouched', () => {
    expect(src('read-tail-r42-sql.ts')).toContain('export const crmSelectOneSql = (body: string) => `SELECT 1 FROM ${body}`');
    expect(src('read-tail-r43-sql.ts')).toContain('export const crmScalarSubquerySql = (projection: string, body: string)');
    expect(src('read-tail-r44-sql.ts')).toContain('export const crmUnionAllSql = (body: string) => `UNION ALL SELECT ${body}`');
    expect(src('read-tail-r46-sql.ts')).toContain('crmProjectionFromSql = (projection: string, from: string)');
    expect(src('read-tail-r47-sql.ts')).toContain('crmOwnedListSql = (');
    expect(src('read-tail-r48-sql.ts')).toContain('crmCompanyScopeSql = (tail: string) => `SELECT ?::text company_id${tail}`');
    expect(src('read-tail-r49-sql.ts')).toContain('crmJsonbAggregateSql = (body: string) => `SELECT jsonb_agg(to_jsonb(${body}`');
    expect(src('read-tail-r50-sql.ts')).toContain('export const crmSelectExprSql = (body: string) => `SELECT (${body}`');
    const a = src('attachment-read.ts');
    expect(a).toContain('`SELECT 1 FROM${actorScopeSql}workforce_actor_scope WHERE ${current.where}`');
    expect(a).toContain('await database.batch([crmAttachmentCustomerSourceLocks(database)');
    expect(a).toContain('await lockQueuedCommandAuthority(database);');
    const p = src('purchases.ts');
    expect(p).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(p).toContain('@erpx/server-pms/public/crm-purchase-sources');
    expect(src('marketing-package-obligations.ts')).toContain('@erpx/server-marketing/public/package-obligations');
    expect(src('native-crm-read.ts')).toContain('export async function nativeCrmReadAll<T>');
    expect(src('native-crm-command.ts')).toContain('export async function nativeCrmExecute(');
  });

  test('retirement:ts-statements folds the r51 head into r42 (41 -> 40)', () => {
    expect(totalSelects()).toBe(38);
    expect(totalSelects()).toBeLessThan(41);
  });
});
