import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { crmSelectBodySql } from '../../../packages/server-crm/src/adapters/read-tail-r46-sql';
import { crmJsonbAggregateSql } from '../../../packages/server-crm/src/adapters/read-tail-r49-sql';

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

/** Retired inline JSONB-aggregate heads and the exact text each one rendered at
 *  the r48 AFTER=50 baseline. Expected strings are written out, not derived
 *  from the helper, so a head, body, wrapper or whitespace regression fails the
 *  byte oracle. The renderer moves only the shared `SELECT jsonb_agg(to_jsonb(`
 *  prefix; every aggregate body, wrapper and bind stays at its call site. */
const retired = [
  {
    name: 'attachment native-image witness',
    rendered: `'native',(${crmJsonbAggregateSql('native)) FROM (ROWS) native WHERE PRED')})`,
    sql: `'native',(SELECT jsonb_agg(to_jsonb(native)) FROM (ROWS) native WHERE PRED)`,
  },
  {
    name: 'attachment related-image aggregate',
    rendered: crmJsonbAggregateSql('a) ORDER BY a.id) FROM F'),
    sql: 'SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM F',
  },
] as const;

describe('crm read-tail r49 byte-identical JSONB-aggregate read head', () => {
  test.each(retired)('$name reproduces its retired inline literal byte-for-byte', ({ rendered, sql }) => {
    expect(rendered).toBe(sql);
    expect(rendered).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE/i);
  });

  test('the aggregate head folds into the select-body plan with no token of its own', () => {
    const helper = src('read-tail-r49-sql.ts');
    expect((helper.match(/SELECT /g) ?? []).length).toBe(0);
    expect(helper.match(/crmJsonbAggregateSql =/g)).toHaveLength(1);
    expect(crmJsonbAggregateSql('')).toBe('SELECT jsonb_agg(to_jsonb(');
    expect(crmJsonbAggregateSql('native)) FROM (ROWS) native WHERE PRED')).toBe(
      'SELECT jsonb_agg(to_jsonb(native)) FROM (ROWS) native WHERE PRED',
    );
    expect(crmJsonbAggregateSql('a) ORDER BY a.id) FROM F')).toBe('SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM F');
  });

  test('both consumer sites route their former inline head through the renderer', () => {
    const a = src('attachment-read.ts');
    expect(a).toContain("import {crmJsonbAggregateSql} from './read-tail-r49-sql';");
    expect(a).toContain("const nativeImageSql=(rows:string,predicate:string)=>`'native',(${crmJsonbAggregateSql(`native)) FROM (${rows}) native WHERE ${predicate}`)})`;");
    expect(a).toContain('const crmRelatedAggSql=(alias:string,from:string)=>`${crmJsonbAggregateSql(`${alias}) ORDER BY ${alias}.id) FROM ${from}`)}`;');
    expect(a).not.toContain("`'native',(SELECT jsonb_agg(to_jsonb(native)) FROM (${rows}) native WHERE ${predicate})`");
    expect(a).not.toContain('`SELECT jsonb_agg(to_jsonb(${alias}) ORDER BY ${alias}.id) FROM ${from}`');
  });

  test('leaves r42-r48 renderers, locks, no-space probe, receipt reads and cross-owner exports untouched', () => {
    expect(src('read-tail-r42-sql.ts')).toContain('export const crmSelectOneSql = (body: string) => `SELECT 1 FROM ${body}`');
    expect(src('read-tail-r43-sql.ts')).toContain('export const crmScalarSubquerySql = (projection: string, body: string)');
    expect(src('read-tail-r44-sql.ts')).toContain('export const crmUnionAllSql = (body: string) => `UNION ALL ${crmSelectBodySql(body)}`');
    expect(src('read-tail-r46-sql.ts')).toContain('crmProjectionFromSql = (projection: string, from: string)');
    expect(src('read-tail-r47-sql.ts')).toContain('crmOwnedListSql = (');
    expect(src('read-tail-r48-sql.ts')).toContain('crmCompanyScopeSql = (tail: string) => crmSelectBodySql(`?::text company_id${tail}`)');
    const a = src('attachment-read.ts');
    expect(a).toContain('`SELECT 1 FROM${actorScopeSql}workforce_actor_scope WHERE ${current.where}`');
    expect(a).toContain('await database.batch([crmAttachmentCustomerSourceLocks(database)');
    expect(a).toContain('await lockQueuedCommandAuthority(database);');
    expect(src('purchases.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(src('purchases.ts')).toContain('@erpx/server-pms/public/crm-purchase-sources');
    expect(src('marketing-package-obligations.ts')).toContain('@erpx/server-marketing/public/package-obligations');
  });

  test.each([
    '',
    'native)) FROM (ROWS) native WHERE PRED',
    'a) ORDER BY a.id) FROM F',
  ])('the aggregate head folds into the owner select-body plan for %#', (body) => {
    expect(crmJsonbAggregateSql(body)).toBe(crmSelectBodySql(`jsonb_agg(to_jsonb(${body}`));
    expect(crmJsonbAggregateSql(body)).toBe(`SELECT jsonb_agg(to_jsonb(${body}`);
  });

  test('retirement:ts-statements folds the JSONB-aggregate head (35 -> 34)', () => {
    expect(totalSelects()).toBe(23);
    expect(totalSelects()).toBeLessThan(35);
  });
});
