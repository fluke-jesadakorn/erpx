import { createHash } from 'node:crypto';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { executiveMetricCountSql } from '../../../packages/server-organization/src/adapters/organization-read-tail-sources-r59';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const count = (src: string, needle: string) => src.split(needle).length - 1;
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

// Head parts written out independently of the r59 renderer body.
const HEAD_OPEN = 'SELECT ';
const HEAD_MID = ' label,count(*)::text value FROM ';
const oracle = (label: string, body: string) => HEAD_OPEN + label + HEAD_MID + body;

const NODES_LABEL = 'n.kind';
const NODES_BRANCH_LABEL = "COALESCE((SELECT b.display_name FROM organization_nodes b WHERE b.id=CASE WHEN n.kind='branch' THEN n.id ELSE n.branch_id END AND b.company_id=n.company_id),'Branch')";
const CHANGES_LABEL = 'c.node_kind';
const CHANGES_BRANCH_LABEL = "COALESCE((SELECT display_name FROM organization_nodes WHERE id=c.branch_id AND company_id=c.company_id),'Company-wide')";

const NODES_BODY = "organization_nodes n WHERE n.company_id=? AND n.kind IN ('branch','department') AND n.status='active' AND (?::text IS NULL OR n.id=? OR n.branch_id=?) AND (?::text IS NULL OR n.id=?) GROUP BY 1 ORDER BY 1";
const CHANGES_BODY = "organization_node_changes c WHERE c.company_id=? AND c.state='submitted' AND (?::text IS NULL OR c.branch_id=?) GROUP BY 1 ORDER BY 1";

type Case = { name: string; label: string; body: string; rendered: string; sha: string; binds: number };

// Byte oracles copied verbatim from the two pre-r59 (BEFORE=209) inline call
// sites. Only the head moves to the renderer; each label, body, ? order and
// bind arity is preserved exactly.
const CASES: Case[] = [
  {
    name: 'active-organization-nodes / status-or-default',
    label: NODES_LABEL,
    body: NODES_BODY,
    rendered: "SELECT n.kind label,count(*)::text value FROM organization_nodes n WHERE n.company_id=? AND n.kind IN ('branch','department') AND n.status='active' AND (?::text IS NULL OR n.id=? OR n.branch_id=?) AND (?::text IS NULL OR n.id=?) GROUP BY 1 ORDER BY 1",
    sha: '7a39305fee290c9e8c66f5ea5b534c6fd40d39652a7c755af662942df84d4ccc',
    binds: 6,
  },
  {
    name: 'active-organization-nodes / branch',
    label: NODES_BRANCH_LABEL,
    body: NODES_BODY,
    rendered: "SELECT COALESCE((SELECT b.display_name FROM organization_nodes b WHERE b.id=CASE WHEN n.kind='branch' THEN n.id ELSE n.branch_id END AND b.company_id=n.company_id),'Branch') label,count(*)::text value FROM organization_nodes n WHERE n.company_id=? AND n.kind IN ('branch','department') AND n.status='active' AND (?::text IS NULL OR n.id=? OR n.branch_id=?) AND (?::text IS NULL OR n.id=?) GROUP BY 1 ORDER BY 1",
    sha: 'c5d7e64c41e00871bc0383c5003d89181553e22c0712d1db61a760d8ab5d4cfb',
    binds: 6,
  },
  {
    name: 'pending-structure-changes / status-or-default',
    label: CHANGES_LABEL,
    body: CHANGES_BODY,
    rendered: "SELECT c.node_kind label,count(*)::text value FROM organization_node_changes c WHERE c.company_id=? AND c.state='submitted' AND (?::text IS NULL OR c.branch_id=?) GROUP BY 1 ORDER BY 1",
    sha: '3ff317a5d08873dfe87c50066f47b95088a98935206f28b3abf45493286f3601',
    binds: 3,
  },
  {
    name: 'pending-structure-changes / branch',
    label: CHANGES_BRANCH_LABEL,
    body: CHANGES_BODY,
    rendered: "SELECT COALESCE((SELECT display_name FROM organization_nodes WHERE id=c.branch_id AND company_id=c.company_id),'Company-wide') label,count(*)::text value FROM organization_node_changes c WHERE c.company_id=? AND c.state='submitted' AND (?::text IS NULL OR c.branch_id=?) GROUP BY 1 ORDER BY 1",
    sha: 'ef64c25c56aadabe82693acc88d1f68df012bfcde8b554f8a445b05cd30246ab',
    binds: 3,
  },
];

describe('Organization read-tail r59 dedupe', () => {
  test('renderer reproduces all four pre-r59 SQL strings byte for byte', () => {
    expect(CASES).toHaveLength(4);
    for (const c of CASES) {
      const got = executiveMetricCountSql(c.label, c.body);
      expect(got, c.name).toBe(c.rendered);
      expect(got, c.name).toBe(oracle(c.label, c.body));
      expect(sha(got), c.name).toBe(c.sha);
      expect((got.match(/\?/g) ?? []).length, c.name).toBe(c.binds);
      expect(got.startsWith(HEAD_OPEN)).toBe(true);
      expect(got).toContain(HEAD_MID);
      expect(got.endsWith(c.body)).toBe(true);
      expect(got).not.toMatch(/FOR (UPDATE|SHARE)|LOCK TABLE|INSERT INTO|DELETE FROM/i);
    }
    // One head, distinct rendered tails; the body carries every bind.
    expect(new Set(CASES.map(c => c.body)).size).toBe(2);
    expect(new Set(CASES.map(c => c.sha)).size).toBe(4);
    expect(NODES_BODY).not.toBe(CHANGES_BODY);
  });

  test('both call sites delegate with unchanged bodies and binds', () => {
    const caller = read('executive-metric-projections.ts');
    expect(count(caller, "import { executiveMetricCountSql } from './organization-read-tail-sources-r59';")).toBe(1);
    expect(count(caller, 'executiveMetricCountSql(')).toBe(2);
    for (const body of [NODES_BODY, CHANGES_BODY]) {
      expect(caller).toContain(`executiveMetricCountSql(label, \`${body}\`)`);
      expect(caller).not.toContain(`label,count(*)::text value FROM ${body.slice(0, 24)}`);
    }
    expect(caller).not.toContain('label,count(*)::text value FROM organization_nodes n WHERE');
    expect(caller).not.toContain('label,count(*)::text value FROM organization_node_changes c WHERE');
    // Bind arrays stay at the call sites, in original order and arity.
    expect(caller).toContain('values = [a.companyId, a.branchId, a.branchId, a.branchId, a.departmentId, a.departmentId];');
    expect(caller).toContain('values = [a.companyId, a.branchId, a.branchId];');
    // The branch-label subqueries remain inline at the call sites.
    expect(caller).toContain('COALESCE((SELECT b.display_name FROM organization_nodes b');
    expect(caller).toContain('COALESCE((SELECT display_name FROM organization_nodes WHERE id=c.branch_id');
    expect(caller).not.toMatch(/FOR (UPDATE|SHARE)/);
  });

  test('helper owns one read-only head; locks, spaced joins and r48-r58 helpers stay untouched', () => {
    const helper = read('organization-read-tail-sources-r59.ts');
    expect(count(helper, 'export const executiveMetricCountSql =')).toBe(1);
    expect(count(helper, 'SELECT ')).toBe(1);
    expect(helper).toContain('`SELECT ${label} label,count(*)::text value FROM ${body}`');
    expect(helper).not.toMatch(/FOR (UPDATE|SHARE)|LOCK TABLE|pg_advisory/i);

    expect(read('organization-read-tail-sources-r57.ts')).toContain('export const authoritySeatExistsSql =');
    expect(read('organization-read-tail-sources-r55.ts')).toContain('export const personEngagementJoin =');
    expect(read('organization-read-tail-sources-r56.ts')).toContain('export const notExistsOrgNodeSql =');
    expect(read('pms-command-facts.ts')).toContain('FOR SHARE');
    expect(read('people-lifecycle-facts.ts')).toContain('FOR UPDATE');
    expect(read('organization-drafts.ts')).toContain('FOR UPDATE');
    expect(read('project-worker-startup-sources.ts')).toContain(
      'LOCK TABLE organization_nodes,organization_positions,staff_levels,authority_seats IN SHARE MODE',
    );
    expect(read('organization-nodes-spaced-projections.ts')).toContain('"id", "kind", "slug"');
  });

  test('retirement:ts-statements pins 209 -> 200', () => {
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
    // r59 folds two executive-metric count heads into one renderer
    // (-2 literals, +1 declaration): 209 - 2 + 1 = 208.
    // r60 folds two level-history heads (-2 literals, +1 renderer): 208 - 1 = 207.
    // r61 folds two position-package projection heads (-2 literals, +1 renderer): 207 - 1 = 206.
    expect(CASES).toHaveLength(4);
    expect(total).toBe(200);
    expect(total).toBeLessThan(209);
  });
});
