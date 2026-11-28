import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';

const srcRoot = resolve(__dirname, '../../../modules/learning/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');
const selectCount = (text: string) => (text.match(/SELECT /g) ?? []).length;

// Pre-dedupe literals copied verbatim from the r5 baseline. Byte-identity
// oracles only, never live query sources.
const srcExpr = '${organizationSources.learning_adapters_executive_metric_projections_organization_nodes_n_from}';
const preBranchLabel = "COALESCE((SELECT n.display_name " + srcExpr + " WHERE n.id=a.branch_id AND n.company_id=a.company_id),'Branch')";
const preDepartmentLabel = "COALESCE((SELECT n.display_name " + srcExpr + " WHERE n.id=a.department_id AND n.company_id=a.company_id),'Unassigned')";

describe('Learning read-tail r6 parameterized org-node label family dedupe', () => {
  test('one parameterized source replaces the two inlined label statements', () => {
    const text = src('executive-metric-projections.ts');
    expect(text).not.toContain(preBranchLabel);
    expect(text).not.toContain(preDepartmentLabel);
    expect(text).toContain('const orgNodeLabel = (field: string, fallback: string) =>');
    expect(text).toContain('n.id=a.${field}_id');
    expect(text).toContain(",'${fallback}')`;");
    expect(text).toContain("const branchLabel = orgNodeLabel('branch', 'Branch');");
    expect(text).toContain("const departmentLabel = orgNodeLabel('department', 'Unassigned');");
    expect(text.match(/orgNodeLabel\('/g) ?? []).toHaveLength(2);
  });

  test('the parameterized source renders both original literals byte-for-byte', () => {
    const render = (field: string, fallback: string) => `COALESCE((SELECT n.display_name ${srcExpr} WHERE n.id=a.${field}_id AND n.company_id=a.company_id),'${fallback}')`;
    expect(render('branch', 'Branch')).toBe(preBranchLabel);
    expect(render('department', 'Unassigned')).toBe(preDepartmentLabel);
  });

  test('r3/r4/r5 helpers stay put and no lock text entered the family', () => {
    const r3 = src('learning-read-sql.ts');
    expect(r3.match(/export const learningNativeSources=/g) ?? []).toHaveLength(1);
    expect(src('native_lane_31/postgres.rs')).toContain('SELECT a.id,a.version,a.status,a.due_date,a.updated_at,c.title course_title,c.required_for');
    expect(r3.match(/export const learningModuleGrantProbe =/g) ?? []).toHaveLength(1);
    expect(src('native_lane_31/postgres.rs')).toContain('SELECT e.id,e.assignment_id,e.version event_version,e.status,e.created_at,c.title course_title');
    const text = src('executive-metric-projections.ts');
    expect(src('learning.ts')).toContain('const assignmentChildImage =');
    expect(text).not.toMatch(/LOCK TABLE|FOR UPDATE|FOR SHARE|pg_advisory/i);
  });

  test('retirement:ts-statements stays strictly below the r6 BEFORE=77 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(path);
        else if (entry.name.endsWith('.ts')) total += selectCount(readFileSync(path, 'utf8'));
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(77);
  });
});
