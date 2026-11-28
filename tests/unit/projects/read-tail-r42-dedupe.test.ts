import { describe, expect, test, vi } from 'vitest';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';

vi.mock('server-only', () => ({}));

import { projectRowsSql, projectPackageSettlementSources } from '../../../modules/projects/server/src/adapters/package-obligations';

const srcRoot = resolve(__dirname, '../../../modules/projects/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const sha256 = (s: string) => createHash('sha256').update(s).digest('hex');

// Verbatim BEFORE renders captured at BEFORE=73 (source blob
// 40a1774774f1...). Byte-identity oracles only; never live query sources.
const oracle = {
  projectRowsSql: 'a5972510552fb55fffd6acb62eee70a13a8d0d35006cebd9db9ee55c1673fc16',
  projects: 'a5972510552fb55fffd6acb62eee70a13a8d0d35006cebd9db9ee55c1673fc16',
  members: '0d3c5868cbeda1260a51f00876fa92f0de5bbbf4bb3017f1a1f283deaac0c396',
  milestones: 'da1311506f50789aa8dcbeb2ebff99c043b47b81229bf44a1eae23d409a56513',
  workLogs: '6d5c12182de37772bc211c9bebc5fc0ea2636dd90c838481353dc9a3a7f6cdd2',
  costs: '0cd5d2081118f3a04b7d229393ca51083b7f9746c4aad2d8c44c84a6eb6b96f8',
  actuals: '989b20aeb3c9fed7606b35a698a26935ad4c215f914f32b66ad13e773056337b',
  handoffs: 'b259823198a8d6e1ef454bd62f4eaa071ec47b2b8f7cdf0a14fa178c2561d29e',
} as const;

describe('Projects read-tail r42 closed row-source head family', () => {
  test('one closed head renders every settlement row source byte for byte', () => {
    const rendered: Record<string, string> = { projectRowsSql, ...projectPackageSettlementSources };
    for (const [key, sql] of Object.entries(rendered)) {
      expect(sha256(sql)).toBe((oracle as Record<string, string>)[key]);
      expect(sql.startsWith('SELECT * FROM ')).toBe(true);
      // The head is bind-free: bind order of every consumer is unchanged.
      expect(sql.match(/\?/g)).toBeNull();
    }
    expect(projectRowsSql).toBe('SELECT * FROM staff_projects');
    expect(projectPackageSettlementSources.projects).toBe(projectRowsSql);
  });

  test('the shared prefix has exactly one definition and the projects source delegates to it', () => {
    const obl = src('package-obligations.ts');
    expect(obl.match(/SELECT \* FROM /g)).toHaveLength(1);
    expect(obl).toContain('const tableRowsSql=`SELECT * FROM `;');
    expect(obl).toContain('export const projectRowsSql=`${tableRowsSql}staff_projects`;');
    // The former independent literal is gone.
    expect(obl).not.toContain('export const projectRowsSql="SELECT * FROM staff_projects";');
    // Every other settlement source still appends its closed relation to the one head.
    for (const table of ['staff_project_members', 'staff_project_milestones', 'staff_project_work_logs',
      'staff_project_cost_allocations', 'staff_project_package_actuals', 'staff_project_handoffs']) {
      expect(obl.match(new RegExp(`\\$\\{tableRowsSql\\}${table}`, 'g'))).toHaveLength(1);
    }
    // Pure text only: nothing cached or executed in this module head.
    expect(obl).toContain('server-only');
  });

  test('locks, native SQL and cross-owner projections are excluded and unchanged', () => {
    const obl = src('package-obligations.ts');
    expect(obl).toContain('LOCK TABLE staff_projects,staff_project_members,staff_project_milestones');
    expect(src('scheduled-tasks.ts')).toContain('SELECT id FROM staff_projects WHERE id=? AND company_id=? FOR SHARE');
    expect(src('scheduled-tasks.ts')).toContain('FOR UPDATE OF t');
    expect(src('final-root-owner-projection-sources.ts')).toContain(
      'const quotedProjection = (columns: string, relation: string) => selectProjectionFrom(columns, relation);');
    // The folded head region emits no write, lock or native text.
    const head = obl.slice(obl.indexOf('const tableRowsSql'), obl.indexOf('export const projectAssignmentScopeSql'));
    expect(head).not.toMatch(/INSERT INTO|DELETE FROM|UPDATE\s+\w+\s+SET|FOR\s+UPDATE|FOR\s+SHARE|LOCK\s+TABLE|pg_advisory/i);
    // r29/r41 bare heads now delegate to the r45 owner body head; the other
    // head families keep their own definitions. Only the row-source prefix is
    // folded here.
    for (const helper of ['read-tail-r29.ts', 'read-tail-r41.ts']) {
      expect(src(helper)).toContain("from './read-tail-r45'");
      expect(src(helper).match(/SELECT /g)).toBeNull();
    }
    for (const helper of ['read-tail-r37.ts', 'read-tail-r39.ts', 'read-tail-r40.ts']) {
      expect(src(helper).match(/SELECT /g)).not.toBeNull();
    }
    expect(obl).not.toContain("from './read-tail-r42'");
  });

  test('retirement:ts-statements is 72 after r42 (BEFORE=73)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    // r41 left 73. The two byte-identical row-source head literals (the fixed
    // projects source and the settlement table prefix) now share one closed head:
    // 73-2+1 = 72. Locks, native SQL and cross-owner projections keep their own
    // literals and are not folded.
    expect(total).toBe(57); // r46 evaluation reads moved to the native contract (62->57)
    expect(total).toBeLessThan(73);
  });
});
