import { createHash } from 'node:crypto';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { nodeKindDisplayNameHeadSql } from '../../../modules/organization/server/src/adapters/organization-read-tail-sources-r62';

const srcRoot = resolve(__dirname, '../../../modules/organization/server/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const count = (src: string, needle: string) => src.split(needle).length - 1;
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

// Head written out independently of the r62 renderer body.
const HEAD = 'SELECT id,kind,display_name,';
const oracle = (tail: string) => `${HEAD}${tail}`;

// Verbatim tails from the two pre-r62 (BEFORE=206) inline call sites. The
// executive tail carries its own nested branch-id SELECT; only the projection
// head moved to the renderer, so every rendered byte stays identical.
const TAIL_COMMERCIAL =
  "company_id FROM organization_nodes WHERE kind IN ('company','branch')";
const TAIL_EXECUTIVE =
  `parent_id,branch_id FROM organization_nodes WHERE company_id=? AND status='active' AND (kind='company' OR ((?::text IS NULL OR id=? OR branch_id=?) AND (?::text IS NULL OR id=? OR (kind='branch' AND id=(SELECT branch_id FROM organization_nodes WHERE id=? AND company_id=?))))) ORDER BY kind,display_name,id LIMIT 501`;

type Case = { name: string; tail: string; rendered: string; sha: string; len: number; binds: number; selects: number };

// Byte oracles captured from the two pre-r62 inline call sites at HEAD.
const CASES: Case[] = [
  {
    name: 'commercial-usage-facts / commercial usage nodes',
    tail: TAIL_COMMERCIAL,
    rendered: oracle(TAIL_COMMERCIAL),
    sha: 'a8d9209bbae4b17335c3f834c1e6a9e13ea74f29c45ed7672953aa1fca39ffd3',
    len: 97,
    binds: 0,
    selects: 1,
  },
  {
    name: 'executive-context-sources / executive organization nodes',
    tail: TAIL_EXECUTIVE,
    rendered: oracle(TAIL_EXECUTIVE),
    sha: '217fcab9daf0f888e232b67e858e8536754a0e5a839b0a21664e969494dd6170',
    len: 344,
    binds: 8,
    selects: 2,
  },
];

describe('Organization read-tail r62 dedupe', () => {
  test('renderer reproduces both pre-r62 SQL strings byte for byte', () => {
    expect(CASES).toHaveLength(2);
    for (const c of CASES) {
      const got = nodeKindDisplayNameHeadSql(c.tail);
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
    // One head, distinct tails; the bind arity and nested SELECT stay as before.
    expect(CASES[0].tail).not.toBe(CASES[1].tail);
    expect(CASES[0].sha).not.toBe(CASES[1].sha);
    expect(CASES[0].binds).toBe(0);
    expect(CASES[1].binds).toBe(8);
    expect(CASES[1].tail).toContain('(SELECT branch_id FROM organization_nodes');
  });

  test('head oracle is stable and read-only', () => {
    expect(HEAD).toHaveLength(28);
    expect(sha(HEAD)).toBe('8750f2c146a54eceae70164609295d3ff6c9a2a11fe3cd872cae49212648a48a');
    expect(nodeKindDisplayNameHeadSql('')).toBe(HEAD);
  });

  test('both call sites delegate with unchanged tails and binds', () => {
    const commercial = read('commercial-usage-facts.ts');
    expect(commercial).toContain("import {nodeKindDisplayNameHeadSql} from './organization-read-tail-sources-r62';");
    expect(count(commercial, 'nodeKindDisplayNameHeadSql(')).toBe(1);
    expect(commercial).toContain("nodeKindDisplayNameHeadSql(\"company_id FROM organization_nodes WHERE kind IN ('company','branch')\")");
    expect(commercial).not.toContain('SELECT id,kind,display_name,company_id');

    const executive = read('executive-context-sources.ts');
    expect(executive).toContain("import {nodeKindDisplayNameHeadSql} from './organization-read-tail-sources-r62';");
    expect(count(executive, 'nodeKindDisplayNameHeadSql(')).toBe(1);
    expect(executive).not.toContain('SELECT id,kind,display_name,parent_id');
    expect(executive).toContain('nodeKindDisplayNameHeadSql(`parent_id,branch_id FROM organization_nodes WHERE company_id=?');
    expect(executive).toContain('(SELECT branch_id FROM organization_nodes WHERE id=? AND company_id=?)');
    // The executive bind arguments are unchanged.
    expect(executive).toContain('.bind(a.companyId,a.branchId,a.branchId,a.branchId,a.departmentId,a.departmentId,a.departmentId,a.companyId)');
  });

  test('helper owns one read-only head; locks, spaced joins and r50-r61 helpers stay untouched', () => {
    const helper = read('organization-read-tail-sources-r62.ts');
    expect(count(helper, 'export const nodeKindDisplayNameHeadSql =')).toBe(1);
    expect(count(helper, 'SELECT ')).toBe(1);
    expect(helper).toContain('`SELECT id,kind,display_name,${tail}`');
    expect(helper).not.toMatch(/FOR (UPDATE|SHARE)|LOCK TABLE|pg_advisory|native_/i);

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

  test('retirement:ts-statements pins 206 -> 196', () => {
    // Wave 2 moved three node-reference reads to the native owner (six lane-23 reads total); the absolute pin reflects the current owner source.
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r62 folds two node kind/display-name projection head literals into one
    // renderer (-2 literals, +1 declaration): 206 - 2 + 1 = 205.
    expect(CASES).toHaveLength(2);
    expect(total).toBe(196);
    expect(total).toBeLessThan(206);
  });
});
