import { createHash } from 'node:crypto';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { notExistsOrgNodeSql } from '../../../modules/organization/server/src/adapters/organization-read-tail-sources-r56';

const srcRoot = resolve(__dirname, '../../../modules/organization/server/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const count = (src: string, needle: string) => src.split(needle).length - 1;
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

const HEAD = 'NOT EXISTS(SELECT 1 FROM organization_nodes';
const IMPORT = "import {notExistsOrgNodeSql} from './organization-read-tail-sources-r56';";

type Case = { label: string; body: string; sha: string; binds: number };

// Byte oracles recorded verbatim from the pre-r56 (BEFORE=216) source. Only the
// body after `organization_nodes` varies; the head and closing paren are
// rendered byte-identically by the helper and all bind arrays stay at callers.
const CASES: Case[] = [
  {
    label: 'empty bootstrap forest',
    body: '',
    sha: '6e093dbfc0207cb957da33e1a52c941970c0068402d997985235c1b980efcbe2',
    binds: 0,
  },
  {
    label: 'relocation no children',
    body: " WHERE company_id=? AND parent_id=? AND status<>'archived'",
    sha: 'f1d90511a940cdf45aecb296500b968ff687657323ee9bf8758de825bd861ffc',
    binds: 2,
  },
  {
    label: 'relocation slug available',
    body: ' destination JOIN organization_nodes source ON source.id=? AND source.company_id=destination.company_id WHERE destination.company_id=? AND destination.parent_id=? AND destination.slug=source.slug',
    sha: '122afca2c5518c0ff0536cff4e0353bd77315d8042eb9992d6c6ef3a31338fa1',
    binds: 3,
  },
];

// Reconstructed independently of the helper body: the closed byte oracle.
const oracle = (body: string) => `${HEAD}${body})`;

describe('Organization read-tail r56 dedupe', () => {
  test('r56 helper renders byte-identically to every retired literal', () => {
    expect(CASES).toHaveLength(3);
    for (const c of CASES) {
      const got = notExistsOrgNodeSql(c.body);
      expect(got).toBe(oracle(c.body));
      expect(sha(got)).toBe(c.sha);
      expect(count(got, 'SELECT ')).toBe(1);
      expect((got.match(/\?/g) ?? []).length).toBe(c.binds);
      expect(got.startsWith(HEAD)).toBe(true);
      expect(got.endsWith(')')).toBe(true);
    }

    // Owner-supplied bodies keep their own predicate and join tails verbatim.
    expect(CASES[0].body).toBe('');
    expect(CASES[1].body).toContain("status<>'archived'");
    expect(CASES[2].body).toContain('destination JOIN organization_nodes source ON');
    for (const c of CASES) expect(c.body).not.toMatch(/FOR (UPDATE|SHARE)/);
    expect(notExistsOrgNodeSql('')).not.toBe(notExistsOrgNodeSql(CASES[2].body));
  });

  test('all three call sites delegate with unchanged bodies, binds and lock-free tails', () => {
    const beta = read('beta-management-sources.ts');
    const org = read('organization.ts');

    expect(beta).toContain(IMPORT);
    expect(org).toContain(IMPORT);
    expect(beta).toContain("emptyBootstrapForest:notExistsOrgNodeSql(''),");
    expect(org).toContain('notExistsOrgNodeSql(" WHERE company_id=? AND parent_id=? AND status<>\'archived\'")');
    expect(org).toContain('notExistsOrgNodeSql(" destination JOIN organization_nodes source ON');

    expect(beta).not.toContain("'NOT EXISTS(SELECT 1 FROM organization_nodes)'");
    expect(org).not.toContain('"NOT EXISTS(SELECT 1 FROM organization_nodes WHERE');
    expect(org).not.toContain('"NOT EXISTS(SELECT 1 FROM organization_nodes destination');

    expect(count(beta, 'notExistsOrgNodeSql(')).toBe(1);
    expect(count(org, 'notExistsOrgNodeSql(')).toBe(2);

    // Assertion bind arrays remain exactly where they were.
    expect(org).toContain('[actor.companyId,change.target_node_id]),');
    expect(org).toContain('[change.target_node_id,actor.companyId,change.branch_id]),');
  });

  test('r50/r53/r54/r55 helpers, locks and spaced joins stay untouched', () => {
    const r56 = read('organization-read-tail-sources-r56.ts');
    expect(count(r56, 'export const notExistsOrgNodeSql =')).toBe(1);
    expect(count(r56, 'SELECT ')).toBe(1);
    for (const name of ['nodeExistsGuard', 'activePositionLookup', 'activeNodeByKind', 'latestEmployeeEngagementId', 'personEngagementJoin']) {
      expect(r56).not.toContain(`export const ${name} =`);
    }

    for (const [file, names] of [
      ['organization-read-tail-sources-r50.ts', ['nodeExistsGuard', 'activePositionLookup']],
      ['organization-column-alias-families-r27.ts', ['activeNodeByKind', 'twoColumnLookup']],
      ['organization-read-tail-sources-r55.ts', ['latestEmployeeEngagementId', 'personEngagementJoin']],
    ] as [string, string[]][]) {
      const source = read(file);
      for (const name of names) expect(count(source, `export const ${name} =`)).toBe(1);
    }

    // Lock tails and spaced-join projections are unchanged.
    expect(read('pms-command-facts.ts')).toContain('FOR SHARE');
    expect(read('people-lifecycle-facts.ts')).toContain('FOR UPDATE');
    expect(read('organization-drafts.ts')).toContain('FOR UPDATE');
    expect(read('project-worker-startup-sources.ts')).toContain(
      'LOCK TABLE organization_nodes,organization_positions,staff_levels,authority_seats IN SHARE MODE',
    );
    expect(read('organization-nodes-spaced-projections.ts')).toContain('"id", "kind", "slug"');

    const imported = [read('beta-management-sources.ts'), read('organization.ts')];
    for (const src of imported) expect(src).not.toContain('FOR UPDATE');
  });

  test('retirement:ts-statements pins 216 -> 196', () => {
    // Wave 2 moved three node-reference reads to the native owner (six lane-23 reads total); the absolute pin reflects the current owner source.
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(path, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r56 folds the three absent-organization-node guard literals into one
    // renderer (-3 literals, +1 declaration): 216 - 3 + 1 = 214. r57 folds the
    // authority-seat EXISTS head (-4 sites, +1 declaration): 214 - 4 + 1 = 211.
    // r58 folds the two approvals-facts heads (-2 sites): 211 - 2 = 209.
    // r59 folds two executive-metric count heads (-2 literals, +1 renderer): 209 - 1 = 208.
    // r60 folds two level-history head literals (-2 literals, +1 renderer): 208 - 1 = 207.
    // r61 folds two position-package projection heads (-2 literals, +1 renderer): 207 - 1 = 206.
    expect(CASES).toHaveLength(3);
    expect(total).toBe(196);
    expect(total).toBeLessThan(216);
  });
});
