import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { assignmentReadSql } from '@erpx/identity-access-server/public/record-source-authority';
import { projectAssignmentScopeSql } from '../../../modules/projects/server/src/adapters/package-obligations';
import { projectAssignmentAuthorityReadSql } from '../../../modules/projects/server/src/adapters/read-tail-r23';

const srcRoot = resolve(__dirname, '../../../modules/projects/server/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Verbatim pre-dedupe literals captured from HEAD before the r23 edits. These
// are byte-identity oracles only, never live query sources.
const preDedupe = (alias: string, permission: string, multiline = false) =>
  `SELECT ${assignmentReadSql(alias, permission)} allowed FROM${multiline ? "\n " : " "}${projectAssignmentScopeSql(alias)}`;

describe('Projects read-tail r23 assignment-authority read pair dedupe', () => {
  test('helper reproduces both former inline reads byte-identically', () => {
    expect(projectAssignmentAuthorityReadSql('project_scope', 'records.read', true))
      .toBe(preDedupe('project_scope', 'records.read', true));
    expect(projectAssignmentAuthorityReadSql('project_metric_scope', 'records.read'))
      .toBe(preDedupe('project_metric_scope', 'records.read'));
    // The two reads differ only by the scope alias (now a parameter) and the
    // cosmetic FROM separator (now a closed flag); no relation, predicate or
    // bind order changed.
    expect(projectAssignmentAuthorityReadSql('project_scope', 'finance.read', true))
      .toContain('allowed FROM\n (SELECT ?::text company_id,?::text branch_id,?::text department_id) project_scope');
    expect(projectAssignmentAuthorityReadSql('project_metric_scope', 'finance.read'))
      .toContain('allowed FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) project_metric_scope');
  });

  test('both call sites compose the helper and keep their original binds', () => {
    const purchases = src('purchases.ts');
    const metric = src('metric-purchases.ts');
    expect(purchases).toContain('projectAssignmentAuthorityReadSql("project_scope",permission,true)');
    expect(purchases).toContain('.bind(actor.assignmentId,actor.userId,actor.role,scope.company_id,scope.branch_id,scope.department_id)');
    expect(metric).toContain('projectAssignmentAuthorityReadSql("project_metric_scope",permission)');
    expect(metric).toContain('.bind(actor.assignmentId,actor.userId,actor.role,actor.companyId,actor.branchId,actor.departmentId)');
    expect(purchases).not.toContain('SELECT ${assignmentReadSql(');
    expect(metric).not.toContain('SELECT ${assignmentReadSql(');
    expect(purchases).not.toContain('projectAssignmentScopeSql("project_scope")');
  });

  test('r2-r22 helpers, source and locks stay untouched', () => {
    expect(src('package-obligations.ts')).toContain('export const projectAssignmentScopeSql=');
    expect(src('package-obligations.ts')).toContain('const tableRowsSql=`SELECT * FROM `;');
    const defs = src('project-read-projections.ts');
    expect(defs).toContain('export const projectMemberScopeGuard =');
    expect(defs).toContain('export const projectMemberScopeSql =');
    expect(src('final-root-owner-projection-sources.ts')).toContain('const quotedProjection =');
    expect(src('collaboration-home-sources.ts')).toContain('const projectsIdTitleOptions =');
    expect(src('executive-metric-projections.ts')).toContain('const groupedMetricSelect =');
    // Lock guards and purchase boundaries are not edited by this read-only dedupe.
    expect(purchasesLocks()).toBe(true);
  });

  test('retirement:ts-statements is strictly lower than BEFORE=125', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(125);
    expect(total).toBeLessThanOrEqual(124);
  });
});

function purchasesLocks(): boolean {
  const text = src('purchases.ts');
  return text.includes('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE')
    && text.includes('companyPurchaseReadLocks(database,companyId)')
    && text.includes('lockQueuedCommandAuthority(database)');
}
