import { describe, it, expect } from 'vitest';
import { readFile, readdir } from 'node:fs/promises';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import ts from 'typescript';
import { actorScopeAssignmentSql } from '../../../packages/server-marketing/src/adapters/marketing-actor-scope-guard';
import { marketingActorScopeSql } from '../../../packages/server-marketing/src/adapters/marketing-actor-scope-sql';
import { marketingSelectHeadSql } from '../../../packages/server-marketing/src/adapters/select-head-sql';

const adapter = (name: string) =>
  new URL(`../../../packages/server-marketing/src/adapters/${name}`, import.meta.url);
const src = (name: string) => readFile(adapter(name), 'utf8');
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

// Byte oracles captured at HEAD 1a5cd6cdc2752e2f8477b71c669dad3f889671cb,
// before r50 routed the actor-scope assignment guard's byte-identical terminal
// head through the canonical owner-local head renderer.
const HEAD = '1a5cd6cdc2752e2f8477b71c669dad3f889671cb';

const scopeOracle = '(SELECT ?::text company_id,?::text branch_id,?::text department_id)';
const guardOracle = (predicate: string, alias: string, suffix = '') =>
  `SELECT ${predicate}${suffix} FROM ${scopeOracle} ${alias}`;

/** AST evidence: count head tokens only inside string/template text nodes. */
function headTokens(text: string): number {
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
async function ownerHeadTokens(): Promise<{ spaced: number; bare: number }> {
  let spaced = 0, bare = 0;
  const walk = async (dir: string) => {
    for (const entry of await readdir(dir, { withFileTypes: true })) {
      const path = resolve(dir, entry.name);
      if (entry.isDirectory()) await walk(path);
      else if (entry.name.endsWith('.ts')) {
        const text = await readFile(path, 'utf8');
        spaced += headTokens(text);
        bare += (text.match(/SELECT/g) ?? []).length;
      }
    }
  };
  await walk(resolve('packages/server-marketing/src'));
  return { spaced, bare };
}

describe('marketing read-tail r50 dedupe', () => {
  it(`renders the actor-scope assignment guard byte-identically from ${HEAD}`, () => {
    expect(marketingActorScopeSql).toBe(scopeOracle);
    expect(sha(marketingActorScopeSql)).toBe('1459a347ab36cfdd44cc7cd35ecfcdfb80c5c8b2193b51b14e4ba973885327ec');
    expect(marketingActorScopeSql).toHaveLength(67);

    for (const [name, predicate, alias, suffix, digest, len] of [
      ['plain', 'P', 'a', '', 'ddc78c464a4a17efe820497b2bc57074e2596dd4b81420e980bf2d236eff1ab9', 83],
      ['allowed', 'P', 'a', ' allowed', 'dea539a74f9248328ff7e86267532bc7b9c737aa26d1dcdfe86df14390a6fa4f', 91],
      ['or-guard', '(P1 OR P2)', 'marketing_file_review', '', '7dcc8cc680eb5d2a3958afbfeeb6512edeb005a3b468469c6bbedff3a2e9bae8', 112],
      ['exists-guard', 'P AND EXISTS(mm)', 'marketing_metric_scope', ' allowed', '334387d5b8bc8725ee5b97e5a29f9a885c769cc928cb14cd8bf2ebfd4496ccb2', 127],
      ['real-scope', 'assignmentReadSql("marketing_file_scope","records.write")', 'marketing_file_scope', '', '97db4b474cf30b3d57699f07bebd9cf55e5a4a8da4fd141cb8a25789d2558043', 158],
      ['suffix-alias', 'P', 'marketing_file_review', ' r', 'e0628d023a9aea7ebabe6eb7bed677945bafaaad76e44ca7be13ca4ca4df7798', 105],
    ] as const) {
      const rendered = actorScopeAssignmentSql(predicate, alias, suffix);
      expect(rendered, name).toBe(guardOracle(predicate, alias, suffix));
      expect(sha(rendered), name).toBe(digest);
      expect(rendered, name).toHaveLength(len);
      expect((rendered.match(/\?/g) ?? []).length, name).toBe(3);
    }
  });

  it('routes the guard terminal head through the canonical renderer', async () => {
    const renderer = await src('select-head-sql.ts');
    expect(renderer).toContain('`SELECT ${head}${tail}`');
    expect(headTokens(renderer)).toBe(1);
    expect(marketingSelectHeadSql('t')).toBe('SELECT t');
    expect(marketingSelectHeadSql('t', ' FROM s WHERE id=?')).toBe('SELECT t FROM s WHERE id=?');

    const guard = await src('marketing-actor-scope-guard.ts');
    expect(guard).toContain("import {marketingSelectHeadSql} from './select-head-sql'");
    expect(guard).toContain('`${predicate}${suffix} FROM ${marketingActorScopeSql} ${alias}`');
    expect(guard).toContain("from './marketing-actor-scope-sql'");
    expect(guard).not.toContain('`SELECT ');
    expect(headTokens(guard)).toBe(0);

    for (const name of [
      'purchases.ts',
      'attachment-purchases.ts',
      'metric-purchases.ts',
      'attachment-reviewer-guard.ts',
    ])
      expect(await src(name)).toContain('actorScopeAssignmentSql');
  });

  it('leaves the protected r38/r40/r44/r45 heads and the candidate-adjacent union untouched', async () => {
    for (const name of [
      'form-asset-read-head.ts',
      'owner-point-read-sql.ts',
      'owner-projection-source.ts',
      'marketing-point-exists-sql.ts',
    ])
      expect(headTokens(await src(name)), name).toBe(1);
    expect((await src('read-tail-sql.ts')).match(/SELECT/g)).toHaveLength(1);
    expect(headTokens(await src('marketing-purchase-union-sql.ts'))).toBe(1);
    expect(await src('crm-purchase-sources.ts')).toContain(
      'SELECT candidate.* FROM marketing_lead_candidates candidate',
    );
    expect(await src('marketing-source-id-sql.ts')).toContain('marketingPurchaseUnionSql(sourceIdTables');
  });

  it('leaves settlement, receipts, locks and native ports untouched', async () => {
    expect(await src('purchases.ts')).toContain('SELECT 1 FROM command_receipts WHERE command_id=?');
    expect(await src('package-obligations.ts')).toContain("command.operation='marketing.asset.activate'");
    expect(await src('public-submission.ts')).toContain('SELECT request_hash,candidate_id FROM marketing_public_receipts WHERE key=?');
    expect(await src('form-asset-lock-sql.ts')).toContain('FOR SHARE');
    expect(await src('marketing-local-point-read-sql.ts')).toContain('FOR UPDATE');
    expect(await src('native-marketing-read.ts')).toContain('export');
  });

  it('pins the owner head KPI to 23 (BEFORE=24)', async () => {
    expect(await ownerHeadTokens()).toEqual({ spaced: 23, bare: 23 });
  });
});
