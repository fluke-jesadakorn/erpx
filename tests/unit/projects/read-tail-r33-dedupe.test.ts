import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { projectOwnerOrMemberSql } from '../../../packages/server-projects/src/adapters/project-read-projections';
import { projectPointReadHead } from '../../../packages/server-projects/src/adapters/read-tail-r33';

const srcRoot = resolve(__dirname, '../../../packages/server-projects/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Verbatim pre-dedupe inline point reads from HEAD before the r33 edit.
// These are byte-identity oracles only, never live query sources.
const assignmentVersionRead = 'SELECT version FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role=?';
const descriptorRead = (table: string) =>
  `SELECT company_id,branch_id AS branch_id,department_id AS department_id,version AS version FROM ${table} WHERE id=? AND company_id=?`;
const ownerVersionGuard = (table: string) =>
  `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND company_id=? AND version=?)`;

describe('Projects read-tail r33 point-read head and owner-or-member dedupe', () => {
  test('point-read helper reproduces all three former inline heads byte for byte', () => {
    expect(`${projectPointReadHead('version', 'assignments', 'user_id')} AND company_id=? AND role=?`).toBe(assignmentVersionRead);
    expect(projectPointReadHead('company_id,branch_id AS branch_id,department_id AS department_id,version AS version', 'staff_projects', 'company_id')).toBe(descriptorRead('staff_projects'));
    expect(`EXISTS(${projectPointReadHead('1', 'staff_projects', 'company_id')} AND version=?)`).toBe(ownerVersionGuard('staff_projects'));
    // The closed key union covers exactly the two former second-key columns.
    expect(projectPointReadHead('1', 'staff_projects', 'user_id')).toBe('SELECT 1 FROM staff_projects WHERE id=? AND user_id=?');
    for (const head of [assignmentVersionRead, descriptorRead('staff_projects'), ownerVersionGuard('staff_projects').slice('EXISTS('.length, -1)]) {
      expect(head.match(/^SELECT .+ FROM \S+ WHERE id=\? AND /)).not.toBeNull();
      expect(head.match(/SELECT /g)).toHaveLength(1);
    }
  });

  test('all three call sites compose the helper and keep their tails, binds and locks unchanged', () => {
    const entry = src('product-entry.ts');
    expect(entry).toContain("import {projectPointReadHead} from './read-tail-r33';");
    expect(entry).toContain("${projectPointReadHead('version','assignments','user_id')} AND company_id=? AND role=?");
    expect(entry).not.toContain('SELECT version FROM assignments WHERE id=? AND user_id=?');
    expect(entry).toContain('.bind(actor.assignmentId,actor.userId,actor.companyId,actor.role)');

    const descriptor = src('attachments-policy-facts.ts');
    expect(descriptor).toContain("import {projectPointReadHead} from './read-tail-r33';");
    expect(descriptor).toContain("projectPointReadHead(`company_id,${branch} AS branch_id,${department} AS department_id,${version} AS version`,descriptor.table,'company_id')");
    expect(descriptor).not.toContain('FROM ${descriptor.table} WHERE id=? AND company_id=?${extra}');
    expect(descriptor).toContain('.bind(facts.targetId,facts.companyId)');
    expect(descriptor).toContain('${projectMemberExistsSql()}');

    const service = src('attachments-service-facts.ts');
    expect(service).toContain("import { projectPointReadHead } from './read-tail-r33';");
    expect(service).toContain("EXISTS(${projectPointReadHead('1',item.table,'company_id')} AND version=?${item.extra ? ` AND ${item.extra}` : \"\"})");
    expect(service).not.toContain('EXISTS(SELECT 1 FROM ${item.table} WHERE id=? AND company_id=? AND version=?');
    expect(service).toContain('[targetId, companyId, version]');
    expect(service).toContain('versionStep');
  });

  test('owner-or-member scope now reuses the shared helper with the same two binds', () => {
    expect(projectOwnerOrMemberSql('p')).toBe(
      `(iam_same_person(p.owner_user_id,?,p.company_id) OR EXISTS(SELECT 1 FROM staff_project_members member WHERE member.project_id=p.id AND iam_same_person(member.user_id,?,p.company_id)))`);
    const evidence = src('executive-main-evidence-sources.ts');
    expect(evidence).toContain('import { projectOwnerOrMemberSql } from "@erpx/server-projects/adapters/project-read-projections";');
    expect(evidence).toContain('AND ${projectOwnerOrMemberSql(\'p\')}`');
    expect(evidence).not.toContain('SELECT 1 FROM staff_project_members member WHERE member.project_id=p.id AND iam_same_person(member.user_id,?,p.company_id)');
    expect(evidence).toContain('p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.department_id=?)');
    expect(evidence).toContain('.bind(...projectSource.args,actor.companyId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId, actor.userId, actor.userId, id, filters.from, filters.to, actor.userId, actor.userId)');
  });

  test('locks, spaced JOINs and r2-r32 helpers stay untouched', () => {
    expect(src('clock-cte.ts')).toContain('export const projectsClockCte=');
    expect(src('read-tail-r23.ts')).toContain('projectAssignmentAuthorityReadSql');
    expect(src('read-tail-r27.ts')).toContain('projectMemberExistsSql');
    expect(src('read-tail-r28.ts')).toContain('assignmentViewerSelect');
    expect(src('read-tail-r29.ts')).toContain('orderedImageAggregateHead');
    expect(src('read-tail-r30.ts')).toContain('visibleProjectRowsHead');
    expect(src('read-tail-r32.ts')).toContain('projectPurchaseSourceHead');
    expect(src('project-id-source.ts')).toContain('projectIdSql');
    expect(src('package-obligations.ts')).toContain('const tableRowsSql=`SELECT * FROM `;');
    expect(src('package-obligations.ts')).toContain('LOCK TABLE staff_projects,staff_project_members,staff_project_milestones');
    expect(src('purchases.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE');
    expect(src('social-purchase-sources.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE MODE');
    expect(src('scheduled-tasks.ts')).toContain('FOR UPDATE OF t');
    expect(src('scheduled-tasks.ts')).toContain('FOR SHARE');
    expect(src('task-purchases.ts')).toContain("${lock?' FOR UPDATE':''}");
    expect(src('people-evaluation-facts.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE');
    expect(src('executive-evidence-chains.ts')).toContain('JOIN (${sourceSql}) p ON p.id=m.project_id AND p.company_id=m.company_id');
    expect(src('project-read-projections.ts')).toContain('projectOwnerOrMemberColumnSql');
    expect(src('project-read-projections.ts')).toContain('projectMemberScopeGuard');
  });

  test('retirement:ts-statements is exactly 105 after r33', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(108);
    expect(total).toBe(95); // r35 point-read family merge (projectIdSql -> projectPointReadHead): 96->95 // r32 left 108; r33 point-read head 108->105 (108-3 inline+1 helper) and owner-or-member reuse 106->105. r34 staff_tasks point 4->1 (-3) and staff_projects by-ref 7->1 (-6): 105->96
  });
});
