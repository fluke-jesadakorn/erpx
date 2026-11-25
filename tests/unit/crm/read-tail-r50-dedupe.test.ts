import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { crmSelectBodySql } from '../../../packages/server-crm/src/adapters/read-tail-r46-sql';
import { crmSelectExprSql } from '../../../packages/server-crm/src/adapters/read-tail-r50-sql';

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

/** Retired inline parenthesized-select heads and the exact text each one
 *  rendered at the r49 AFTER=49 baseline. Expected strings are written out, not
 *  derived from the helper, so a head, body, wrapper or whitespace regression
 *  fails the byte oracle. The renderer moves only the shared `SELECT (` prefix;
 *  every expression, wrapper, predicate and bind stays at its call site. */
const retired = [
  {
    name: 'attachment target witness',
    body: 'CRM_SOURCE_QUERY) witness',
    rendered: crmSelectExprSql('CRM_SOURCE_QUERY) witness'),
    sql: 'SELECT (CRM_SOURCE_QUERY) witness',
    binds: 0,
  },
  {
    name: 'marketing assignment guard',
    body: "jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array('marketing'::text)) allowed FROM assignments WHERE id=?",
    rendered: crmSelectExprSql("jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array('marketing'::text)) allowed FROM assignments WHERE id=?"),
    sql: "SELECT (jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array('marketing'::text)) allowed FROM assignments WHERE id=?",
    binds: 1,
  },
] as const;

describe('crm read-tail r50 byte-identical parenthesized-select head', () => {
  test.each(retired)('$name reproduces its retired inline literal byte-for-byte', ({ rendered, sql, binds }) => {
    expect(rendered).toBe(sql);
    expect((rendered.match(/\?/g) ?? [])).toHaveLength(binds);
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('the parenthesized head folds into the select-body plan with no token of its own', () => {
    const helper = src('read-tail-r50-sql.ts');
    expect((helper.match(/SELECT /g) ?? []).length).toBe(0);
    expect(helper.match(/crmSelectExprSql =/g)).toHaveLength(1);
    expect(crmSelectExprSql('')).toBe('SELECT (');
    expect(crmSelectExprSql('CRM_SOURCE_QUERY) witness')).toBe('SELECT (CRM_SOURCE_QUERY) witness');
    expect(crmSelectExprSql("jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array('marketing'::text)) allowed FROM assignments WHERE id=?")).toBe(
      "SELECT (jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array('marketing'::text)) allowed FROM assignments WHERE id=?",
    );
  });

  test('both consumer sites route their former inline head through the renderer', () => {
    const a = src('attachment-purchases.ts');
    expect(a).toContain("import {crmSelectExprSql} from './read-tail-r50-sql';");
    expect(a).toContain('database.prepare(crmSelectExprSql(`${sourceQuery(target.kind)}) witness`))');
    expect(a).toContain('.bind(...sourceArgs(actor,target)).first<{witness:string|null}>()');
    expect(a).not.toContain('`SELECT (${sourceQuery(target.kind)}) witness`');
    const p = src('purchases.ts');
    expect(p).toContain("import {crmSelectExprSql} from './read-tail-r50-sql';");
    expect(p).toContain("database.prepare(crmSelectExprSql(\"jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array('marketing'::text)) allowed FROM assignments WHERE id=?\"))");
    expect(p).toContain('.bind(actor.assignmentId).first<{allowed:boolean}>()');
    expect(p).not.toContain('"SELECT (jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @>');
  });

  test('leaves r42-r49 renderers, locks, no-space probe, receipt reads, native registries and cross-owner exports untouched', () => {
    expect(src('read-tail-r42-sql.ts')).toContain('export const crmSelectOneSql = (body: string) => `SELECT 1 FROM ${body}`');
    expect(src('read-tail-r43-sql.ts')).toContain('export const crmScalarSubquerySql = (projection: string, body: string)');
    expect(src('read-tail-r44-sql.ts')).toContain('export const crmUnionAllSql = (body: string) => `UNION ALL ${crmSelectBodySql(body)}`');
    expect(src('read-tail-r46-sql.ts')).toContain('crmProjectionFromSql = (projection: string, from: string)');
    expect(src('read-tail-r47-sql.ts')).toContain('crmOwnedListSql = (');
    expect(src('read-tail-r48-sql.ts')).toContain('crmCompanyScopeSql = (tail: string) => crmSelectBodySql(`?::text company_id${tail}`)');
    expect(src('read-tail-r49-sql.ts')).toContain('crmJsonbAggregateSql = (body: string) => crmSelectBodySql(`jsonb_agg(to_jsonb(${body}`)');
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

  test.each([
    '',
    'CRM_SOURCE_QUERY) witness',
    "jsonb_array_length(modules_json::jsonb)=0 OR modules_json::jsonb @> jsonb_build_array('marketing'::text)) allowed FROM assignments WHERE id=?",
  ])('the parenthesized head folds into the owner select-body plan for %#', (body) => {
    expect(crmSelectExprSql(body)).toBe(crmSelectBodySql(`(${body}`));
    expect(crmSelectExprSql(body)).toBe(`SELECT (${body}`);
  });

  test('retirement:ts-statements folds the select-expression head (36 -> 35)', () => {
    expect(totalSelects()).toBe(23);
    expect(totalSelects()).toBeLessThan(36);
  });
});
