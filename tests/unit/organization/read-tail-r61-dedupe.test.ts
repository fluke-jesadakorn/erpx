import { createHash } from 'node:crypto';
import { readdirSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, test } from 'vitest';
import { platformOwnerProjectionSources as people_payrollPlatformSources } from '@erpx/server-people_payroll/public/platform-owner-projection-sources';
import { positionPackageProjectionSql } from '../../../packages/server-organization/src/adapters/organization-read-tail-sources-r61';

const srcRoot = resolve(__dirname, '../../../packages/server-organization/src');
const adapters = resolve(srcRoot, 'adapters');
const read = (name: string) => readFileSync(resolve(adapters, name), 'utf8');
const count = (src: string, needle: string) => src.split(needle).length - 1;
const sha = (s: string) => createHash('sha256').update(s).digest('hex');

// Head written out independently of the r61 renderer body.
const HEAD = 'SELECT p.id,p.title,p.version';
const oracle = (tail: string) => `${HEAD}${tail}`;

// The owner projection fragment is expanded at runtime exactly as it was in
// the two pre-r61 inline literals; only the head moves to the renderer.
const PROJ = people_payrollPlatformSources.people_position_packages;
const TAIL_POSITIONS = `,p.requirements,p.branch_id,p.department_id,p.level_id,p.active,b.package_id,b.revision package_revision FROM organization_positions p LEFT JOIN (${PROJ}) b ON b.company_id=p.company_id AND b.position_id=p.id WHERE p.company_id=? ORDER BY p.title,p.id`;
const TAIL_PACKAGE_IMPACT = ` FROM (${PROJ}) b JOIN organization_positions p ON p.id=b.position_id AND p.company_id=b.company_id WHERE b.company_id=? AND b.package_id=? ORDER BY p.id`;

type Case = { name: string; tail: string; rendered: string; sha: string; binds: number };

// Byte oracles captured from the two pre-r61 (BEFORE=207) inline call sites.
// Every tail, owner projection, ? order and bind arity is preserved exactly.
const CASES: Case[] = [
  {
    name: 'read-people-lifecycle-data-positions / package projection',
    tail: TAIL_POSITIONS,
    rendered: oracle(TAIL_POSITIONS),
    sha: 'c1f4f97df9e595a47ebb70da7cc777ec5c0a64e36aefda1692cc73ff6df38bcc',
    binds: 1,
  },
  {
    name: 'read-package-impact-positions / package join',
    tail: TAIL_PACKAGE_IMPACT,
    rendered: oracle(TAIL_PACKAGE_IMPACT),
    sha: '70e8c5787c0f9e291a5ff832935e4c041e33247575e8ed9c67dbdf741d899a08',
    binds: 2,
  },
];

describe('Organization read-tail r61 dedupe', () => {
  test('renderer reproduces both pre-r61 SQL strings byte for byte', () => {
    expect(CASES).toHaveLength(2);
    for (const c of CASES) {
      const got = positionPackageProjectionSql(c.tail);
      expect(got, c.name).toBe(c.rendered);
      expect(got, c.name).toBe(oracle(c.tail));
      expect(sha(got), c.name).toBe(c.sha);
      // One folded head plus the one SELECT inside the owner projection.
      expect(count(got, 'SELECT ')).toBe(2);
      expect(count(got, HEAD)).toBe(1);
      expect(got, c.name).toContain(`(${PROJ})`);
      expect((got.match(/\?/g) ?? []).length, c.name).toBe(c.binds);
      expect(got.startsWith(HEAD)).toBe(true);
      expect(got).not.toMatch(/FOR (UPDATE|SHARE)|LOCK TABLE|INSERT INTO|DELETE FROM/i);
    }
    // One head, distinct rendered tails; the bind arity is unchanged.
    expect(CASES[0].tail).not.toBe(CASES[1].tail);
    expect(CASES[0].sha).not.toBe(CASES[1].sha);
    expect(CASES[0].binds).toBe(1);
    expect(CASES[1].binds).toBe(2);
  });

  test('both call sites delegate with unchanged tails and binds', () => {
    const src = read('people-lifecycle-facts.ts');
    expect(src).toContain("import { positionPackageProjectionSql } from './organization-read-tail-sources-r61';");
    expect(count(src, 'positionPackageProjectionSql(')).toBe(2);
    for (const c of CASES) {
      expect(src).not.toContain(c.rendered);
    }
    expect(src).not.toContain('SELECT p.id,p.title,p.version,p.requirements');
    expect(src).not.toContain('SELECT p.id,p.title,p.version FROM (');
    expect(src).toContain("positionPackageProjectionSql(`,p.requirements,p.branch_id,p.department_id,p.level_id,p.active,b.package_id,b.revision package_revision FROM organization_positions p LEFT JOIN (${people_payrollPlatformSources.people_position_packages})");
    expect(src).toContain("positionPackageProjectionSql(` FROM (${people_payrollPlatformSources.people_position_packages}) b JOIN organization_positions p");
    // Bind arguments stay at the call sites, in original order and arity.
    expect(count(src, '.bind(companyId)')).toBeGreaterThanOrEqual(2);
    expect(src).toContain('.bind(companyId, id)');
  });

  test('helper owns one read-only head; locks, spaced joins and r48-r60 helpers stay untouched', () => {
    const helper = read('organization-read-tail-sources-r61.ts');
    expect(count(helper, 'export const positionPackageProjectionSql =')).toBe(1);
    expect(count(helper, 'SELECT ')).toBe(1);
    expect(helper).toContain('`SELECT p.id,p.title,p.version${tail}`');
    expect(helper).not.toMatch(/FOR (UPDATE|SHARE)|LOCK TABLE|pg_advisory/i);

    expect(read('organization-read-tail-sources-r60.ts')).toContain('export const levelHistorySql =');
    expect(read('organization-read-tail-sources-r59.ts')).toContain('export const executiveMetricCountSql =');
    expect(read('organization-read-tail-sources-r57.ts')).toContain('export const authoritySeatExistsSql =');
    // Lock and spaced-join fragments remain byte-identical elsewhere.
    const lifecycle = read('people-lifecycle-facts.ts');
    expect(lifecycle).toContain('FOR UPDATE');
    expect(read('pms-command-facts.ts')).toContain('FOR SHARE');
    expect(read('organization-drafts.ts')).toContain('FOR UPDATE');
    expect(read('project-worker-startup-sources.ts')).toContain(
      'LOCK TABLE organization_nodes,organization_positions,staff_levels,authority_seats IN SHARE MODE',
    );
    expect(read('organization-nodes-spaced-projections.ts')).toContain('"id", "kind", "slug"');
  });

  test('retirement:ts-statements pins 207 -> 196', () => {
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
    // r61 folds two position-package projection head literals into one
    // renderer (-2 literals, +1 declaration): 207 - 2 + 1 = 206.
    expect(CASES).toHaveLength(2);
    expect(total).toBe(196);
    expect(total).toBeLessThan(207);
  });
});
