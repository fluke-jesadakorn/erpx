import { createHash } from 'node:crypto';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { nodeIdentityHeadSql } from '../../../packages/server-organization/src/adapters/organization-read-tail-sources-r63';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const count = (src: string, needle: string) => src.split(needle).length - 1;
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

// Head written out independently of the r63 renderer body.
const HEAD = 'SELECT n.id,n.kind,n.slug,n.display_name,';
const oracle = (tail: string) => `${HEAD}${tail}`;

// Verbatim tails from the two pre-r63 (BEFORE=205) inline call sites. The
// workspace tail keeps its literal company-id placeholder token so the byte
// oracle pins the source template literal, not a runtime-expanded bind list.
const TAIL_WORKSPACE =
  `n.company_id,n.branch_id,b.slug AS branch_slug\n` +
  `         FROM organization_nodes n\n` +
  `         LEFT JOIN organization_nodes b\n` +
  `           ON b.id=n.branch_id AND b.kind='branch' AND b.status='active'\n` +
  `         WHERE n.company_id IN (\${placeholders})\n` +
  `           AND n.kind IN ('branch','department') AND n.status='active'\n` +
  `         ORDER BY n.display_name,n.id`;
const TAIL_EXECUTIVE =
  `n.status,n.hierarchy_revision,p.display_name parent_name,\n` +
  `  b.display_name branch_name FROM organization_nodes n\n` +
  `  LEFT JOIN organization_nodes p ON p.id=n.parent_id AND p.company_id=n.company_id\n` +
  `  LEFT JOIN organization_nodes b ON b.id=CASE WHEN n.kind='branch' THEN n.id ELSE n.branch_id END AND b.company_id=n.company_id\n` +
  `  WHERE n.company_id=? AND n.kind IN ('branch','department') AND n.status='active'\n` +
  `    AND (?::text IS NULL OR n.id=? OR n.branch_id=?) AND (?::text IS NULL OR n.id=?)`;

type Case = { name: string; tail: string; rendered: string; sha: string; len: number; binds: number; selects: number };

// Byte oracles captured from the two pre-r63 inline call sites at HEAD.
const CASES: Case[] = [
  {
    name: 'workspace-choices / workspace node choices',
    tail: TAIL_WORKSPACE,
    rendered: oracle(TAIL_WORKSPACE),
    sha: '57870dbbc288027b8745bcdd57583833ed736dcd52fe3cb6d9648018b0ce86db',
    len: 393,
    binds: 0,
    selects: 1,
  },
  {
    name: 'executive-main-evidence-sources / organization node evidence',
    tail: TAIL_EXECUTIVE,
    rendered: oracle(TAIL_EXECUTIVE),
    sha: '1f2cc5c5dff8a615f55fdfd0d0334c75006d0e69f856f56ec9e121f1c5ae4f0b',
    len: 532,
    binds: 6,
    selects: 1,
  },
];

describe('Organization read-tail r63 dedupe', () => {
  test('renderer reproduces both pre-r63 SQL strings byte for byte', () => {
    expect(CASES).toHaveLength(2);
    for (const c of CASES) {
      const got = nodeIdentityHeadSql(c.tail);
      expect(got, c.name).toBe(c.rendered);
      expect(got, c.name).toBe(oracle(c.tail));
      expect(Buffer.byteLength(got, 'utf8'), c.name).toBe(c.len);
      expect(sha(got), c.name).toBe(c.sha);
      expect(count(got, 'SELECT '), c.name).toBe(c.selects);
      expect(count(got, HEAD), c.name).toBe(1);
      expect((got.match(/\?/g) ?? []).length, c.name).toBe(c.binds);
      expect(got.startsWith(HEAD), c.name).toBe(true);
      expect(got, c.name).not.toMatch(/FOR (UPDATE|SHARE)|LOCK TABLE|INSERT INTO|DELETE FROM|pg_advisory|native_/i);
    }
    // One head, distinct tails; the workspace placeholder token stays literal
    // and the executive binds stay in their original id/branch/department order.
    expect(CASES[0].tail).not.toBe(CASES[1].tail);
    expect(CASES[0].sha).not.toBe(CASES[1].sha);
    expect(CASES[0].tail).toContain('IN (${placeholders})');
    expect(CASES[1].binds).toBe(6);
    expect(CASES[1].tail).toContain('(?::text IS NULL OR n.id=? OR n.branch_id=?)');
  });

  test('head oracle is stable and read-only', () => {
    expect(HEAD).toHaveLength(41);
    expect(sha(HEAD)).toBe('b4acf27ed1c8ef4475540c6dfe5afc208bae4b9ab058e260c8ac3c312314a799');
    expect(nodeIdentityHeadSql('')).toBe(HEAD);
  });

  test('both call sites delegate with unchanged tails and binds', () => {
    const workspace = read('workspace-choices.ts');
    expect(workspace).toContain("import {nodeIdentityHeadSql} from './organization-read-tail-sources-r63';");
    expect(count(workspace, 'nodeIdentityHeadSql(')).toBe(1);
    expect(workspace).toContain('nodeIdentityHeadSql(`n.company_id,n.branch_id,b.slug AS branch_slug');
    expect(workspace).not.toContain('SELECT n.id,n.kind,n.slug,n.display_name,n.company_id');
    expect(workspace).toContain('IN (${placeholders})');
    expect(workspace).toContain('.bind(...companyIds)');

    const executive = read('executive-main-evidence-sources.ts');
    expect(executive).toContain("import { nodeIdentityHeadSql } from './organization-read-tail-sources-r63';");
    expect(count(executive, 'nodeIdentityHeadSql(')).toBe(1);
    expect(executive).toContain('nodeIdentityHeadSql(`n.status,n.hierarchy_revision,p.display_name parent_name,');
    expect(executive).not.toContain('SELECT n.id,n.kind,n.slug,n.display_name,n.status');
    // The executive bind arguments are unchanged, in original order and arity.
    expect(executive).toContain('const values: unknown[] = [actor.companyId, actor.branchId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId];');
  });

  test('helper owns one read-only head; locks, spaced joins and r50-r62 helpers stay untouched', () => {
    const helper = read('organization-read-tail-sources-r63.ts');
    expect(count(helper, 'export const nodeIdentityHeadSql =')).toBe(1);
    expect(count(helper, 'SELECT ')).toBe(1);
    expect(helper).toContain('`SELECT n.id,n.kind,n.slug,n.display_name,${tail}`');
    expect(helper).not.toMatch(/FOR (UPDATE|SHARE)|LOCK TABLE|pg_advisory|native_/i);

    expect(read('organization-read-tail-sources-r62.ts')).toContain('export const nodeKindDisplayNameHeadSql =');
    expect(read('organization-read-tail-sources-r61.ts')).toContain('export const positionPackageProjectionSql =');
    expect(read('organization-read-tail-sources-r60.ts')).toContain('export const levelHistorySql =');
    expect(read('organization-read-tail-sources-r59.ts')).toContain('export const executiveMetricCountSql =');
    expect(read('organization-read-tail-sources-r57.ts')).toContain('export const authoritySeatExistsSql =');
    expect(read('organization-read-tail-sources-r50.ts')).toContain('SELECT 1 FROM organization_nodes WHERE id=?');
    expect(read('org-sources.ts')).toContain('"SELECT display_name FROM organization_nodes WHERE id=?"');
    // Lock and spaced-join fragments remain byte-identical elsewhere.
    expect(read('people-lifecycle-facts.ts')).toContain('FOR UPDATE');
    expect(read('pms-command-facts.ts')).toContain('FOR SHARE');
    expect(read('organization-drafts.ts')).toContain('FOR UPDATE');
    expect(read('project-worker-startup-sources.ts')).toContain(
      'LOCK TABLE organization_nodes,organization_positions,staff_levels,authority_seats IN SHARE MODE',
    );
    expect(read('organization-nodes-spaced-projections.ts')).toContain('"id", "kind", "slug"');
  });

  test('retirement:ts-statements pins 205 -> 200', () => {
    // Lane 23 moved three directory reads to the native owner; the absolute pin reflects the current owner source.
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r63 folds two node-identity projection head literals into one renderer
    // (-2 literals, +1 declaration): 205 - 2 + 1 = 204.
    expect(CASES).toHaveLength(2);
    expect(total).toBe(200);
    expect(total).toBeLessThan(205);
  });
});
