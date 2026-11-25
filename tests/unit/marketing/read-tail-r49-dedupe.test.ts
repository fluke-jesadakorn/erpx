import { describe, it, expect } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import ts from 'typescript';
import { marketingActorScopeSql } from '../../../packages/server-marketing/src/adapters/marketing-actor-scope-sql';
import { marketingRecordScopeAuthoritySql } from '../../../packages/server-marketing/src/adapters/record-scope-authority';
import { marketingClockBody } from '../../../packages/server-marketing/src/adapters/materialized-clock-body-sql';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);
const src = (name: string) => readFile(adapter(name), 'utf8');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

// Byte oracles captured at HEAD 02df0198169327083838b0f773e78a853650800b,
// before r49 routed each parenthesized subquery head through marketingSelectHeadSql.
const HEAD = '02df0198169327083838b0f773e78a853650800b';

const actorOracle = '(SELECT ?::text company_id,?::text branch_id,?::text department_id)';

const recordOracle = (source: string, authority: string) =>
  `EXISTS(SELECT 1 FROM (SELECT ${source}.company_id,COALESCE(${source}.branch_id,?::text) branch_id,COALESCE(${source}.department_id,?::text) department_id) ${authority} WHERE EXISTS(SELECT 1 FROM assignments g WHERE g.id=? AND g.user_id=? AND g.role=? AND g.company_id=${authority}.company_id\n    AND iam_assignment_active(g.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)\n    AND iam_principal_active(g.user_id,g.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)\n    AND iam_role_allows(g.role,g.company_id,'records.read')\n    AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=${authority}.branch_id OR g.scope_kind='department' AND g.scope_node_id=${authority}.department_id)))`;

const clockOracle = (gap: '' | ' ') =>
  `${gap}(SELECT floor(extract(epoch FROM clock_timestamp())*1000)::bigint now_ms)`;

/** AST evidence: count SELECT tokens only inside string/template text nodes. */
function selectTokens(text: string): number {
  const file = ts.createSourceFile('owner.ts', text, ts.ScriptTarget.Latest, true);
  let total = 0;
  const visit = (node: ts.Node) => {
    if (
      ts.isStringLiteral(node) ||
      ts.isNoSubstitutionTemplateLiteral(node) ||
      ts.isTemplateHead(node) ||
      ts.isTemplateMiddle(node) ||
      ts.isTemplateTail(node)
    )
      total += (node.text.match(/SELECT /g) ?? []).length;
    ts.forEachChild(node, visit);
  };
  visit(file);
  return total;
}
async function ownerSelectTokens(): Promise<{ spaced: number; bare: number }> {
  let spaced = 0, bare = 0;
  const walk = async (dir: string) => {
    for (const entry of await readdir(dir, { withFileTypes: true })) {
      const path = resolve(dir, entry.name);
      if (entry.isDirectory()) await walk(path);
      else if (entry.name.endsWith('.ts')) {
        const text = await readFile(path, 'utf8');
        spaced += selectTokens(text);
        bare += (text.match(/SELECT/g) ?? []).length;
      }
    }
  };
  await walk(resolve('packages/server-marketing/src'));
  return { spaced, bare };
}

describe('marketing read-tail r49 dedupe', () => {
  it(`renders the actor-scope subquery byte-identically from ${HEAD}`, () => {
    expect(marketingActorScopeSql).toBe(actorOracle);
    expect(sha(marketingActorScopeSql)).toBe('1459a347ab36cfdd44cc7cd35ecfcdfb80c5c8b2193b51b14e4ba973885327ec');
    expect(marketingActorScopeSql).toHaveLength(67);
    expect((marketingActorScopeSql.match(/\?/g) ?? []).length).toBe(3);
  });

  it(`renders the record-scope authority tuple byte-identically from ${HEAD}`, () => {
    for (const [source, authority, digest, len] of [
      ['marketing_source', 'marketing_authority', '4d3dc60fa9ef307d5c43264906095c3bced70a875306f9ceab7ef353dd039962', 784],
      ['src', 'auth', 'bae8aa1b03bc9476f29424a088d8d2949c2171ba12277efde33380d372d1df25', 685],
    ] as const) {
      const rendered = marketingRecordScopeAuthoritySql(source, authority);
      expect(rendered).toBe(recordOracle(source, authority));
      expect(sha(rendered)).toBe(digest);
      expect(rendered).toHaveLength(len);
      expect((rendered.match(/\?/g) ?? []).length).toBe(5);
    }
  });

  it(`renders the MATERIALIZED clock body byte-identically from ${HEAD}`, () => {
    for (const gap of ['', ' '] as const) {
      const rendered = marketingClockBody(gap);
      expect(rendered).toBe(clockOracle(gap));
      expect(sha(rendered)).toBe(
        gap === ''
          ? 'a5dea4c73fef1e89fddea6b18e85750c34e8e5d77420571b343e4a2fd790f8f8'
          : '427c851c1a18d1d9d575bd512530d7dae7bc8c8271e6ac53c89d9709bebfa869',
      );
      expect(rendered).toHaveLength(gap === '' ? 73 : 74);
      expect(rendered.startsWith(`${gap}(`)).toBe(true);
      expect(rendered.endsWith(' now_ms)')).toBe(true);
    }
  });

  it('routes all three heads through the shared renderer with caller-owned parentheses', async () => {
    const helper = await src('select-head-sql.ts');
    expect(helper).toContain('`SELECT ${head}${tail}`');
    expect(selectTokens(helper)).toBe(1);

    expect(await src('marketing-actor-scope-sql.ts')).toContain(
      "`(${marketingSelectHeadSql('?::text company_id,?::text branch_id,?::text department_id')})`",
    );
    expect(await src('record-scope-authority.ts')).toContain(
      '`(${marketingSelectHeadSql(`${source}.company_id,COALESCE(${source}.branch_id,?::text) branch_id,COALESCE(${source}.department_id,?::text) department_id`)}) ${authority}`',
    );
    expect(await src('materialized-clock-body-sql.ts')).toContain(
      '`${gap}(${marketingSelectHeadSql(`${packageClockSql} now_ms`)})`',
    );
    for (const name of ['marketing-actor-scope-sql.ts', 'record-scope-authority.ts', 'materialized-clock-body-sql.ts'])
      expect(selectTokens(await src(name))).toBe(0);
  });

  it('leaves protected r38/r40/r45-r48 heads, locks and settlement untouched', async () => {
    expect((await src('form-asset-read-head.ts')).match(/SELECT /g)).toHaveLength(1);
    expect((await src('owner-point-read-sql.ts')).match(/SELECT /g)).toHaveLength(1);
    expect((await src('owner-projection-source.ts')).match(/SELECT /g)).toHaveLength(1);
    expect(await src('read-tail-sql.ts')).toContain('`SELECT * FROM ${table}${tail}`');
    expect((await src('read-tail-sql.ts')).match(/SELECT/g)).toHaveLength(1);
    expect(await src('purchases.ts')).toContain('nativeMarketingReadFirst<Record<string,unknown>>(MARKETING_RD_COMMAND_RECEIPT_PRIOR');
    expect(await src('form-asset-lock-sql.ts')).toContain('FOR SHARE');
    expect(await src('marketing-local-point-read-sql.ts')).toContain('FOR UPDATE');
    expect(await src('native-marketing-read.ts')).toContain('export');
  });

  it('pins the owner SELECT KPI to 12 (lane 40 retired 3) (BEFORE=27)', async () => {
    expect(await ownerSelectTokens()).toEqual({ spaced: 12, bare: 12 });
  });
});
