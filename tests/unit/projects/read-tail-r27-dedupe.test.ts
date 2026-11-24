import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { projectMemberExistsSql } from '../../../packages/server-projects/src/adapters/read-tail-r27';

const srcRoot = resolve(__dirname, '../../../packages/server-projects/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Verbatim pre-dedupe guards from HEAD before the r27 edit. These are
// byte-identity oracles only, never live query sources.
const purchaseGuard = `NOT ?::boolean OR p.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?)`;
const attachmentGuard = `iam_same_person(p.owner_user_id,?,p.company_id) OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?)`;
const guardHead = `EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?)`;

describe('Projects read-tail r27 member EXISTS family dedupe', () => {
  test('helper reproduces both former inline member guards byte for byte', () => {
    expect(projectMemberExistsSql()).toBe(guardHead);
    expect(`NOT ?::boolean OR p.owner_user_id=? OR ${projectMemberExistsSql()}`).toBe(purchaseGuard);
    expect(`iam_same_person(p.owner_user_id,?,p.company_id) OR ${projectMemberExistsSql()}`).toBe(attachmentGuard);
    // Only aliases vary; relation, equality and bind order stay identical.
    expect(projectMemberExistsSql('pr', 'pm')).toBe(
      `EXISTS(SELECT 1 FROM staff_project_members pm WHERE pm.project_id=pr.id AND pm.user_id=?)`);
    expect(guardHead).toContain('m.project_id=p.id AND m.user_id=?');
  });

  test('both call sites compose the helper and keep their outer predicates unchanged', () => {
    const purchases = src('purchases.ts');
    expect(purchases).toContain("import {projectMemberExistsSql} from './read-tail-r27';");
    expect(purchases).toContain("NOT ?::boolean OR p.owner_user_id=? OR ${projectMemberExistsSql()}");
    expect(purchases).not.toContain('NOT ?::boolean OR p.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members m');

    const attachments = src('attachments-policy-facts.ts');
    expect(attachments).toContain("import {projectMemberExistsSql} from './read-tail-r27';");
    expect(attachments).toContain('iam_same_person(p.owner_user_id,?,p.company_id) OR ${projectMemberExistsSql()}');
    expect(attachments).not.toContain('iam_same_person(p.owner_user_id,?,p.company_id) OR EXISTS(SELECT 1 FROM staff_project_members m');

    expect(untouchedInvariants()).toBe(true);
  });

  test('locks, r2-r26 helpers and spaced JOIN constants stay untouched', () => {
    const defs = src('project-read-projections.ts');
    for (const kept of ['projectTaskOwnerSql', 'projectTaskMemberSql', 'projectOwnerOrMemberSql',
      'projectOwnerOrMemberColumnSql', 'projectMemberScopeSql', 'projectMemberScopeGuard',
      'projectHandoverSourceSql', 'socialProjectAudienceSql']) {
      expect(defs).toContain(kept);
    }
    // The protected r17 helper still owns its own byte-identical copy.
    expect(defs).toContain('(p.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?))');
    expect(src('clock-cte.ts')).toContain('export const projectsClockCte=');
    expect(src('read-tail-r23.ts')).toContain('projectAssignmentAuthorityReadSql');
    expect(src('package-obligations.ts')).toContain('export const projectPackageActualProof=');
    expect(src('package-obligations.ts')).toContain('export const projectAssignmentScopeSql=');
    expect(src('executive-metric-projections.ts')).toContain('const groupedMetricSelect =');
    expect(src('executive-main-evidence-sources.ts')).toContain('const projectDependencyBlockerLateral =');
    const chains = src('executive-evidence-chains.ts');
    expect(chains).toContain('JOIN (${sourceSql}) p ON p.id=m.project_id AND p.company_id=m.company_id');
    expect(chains).toContain('JOIN (${sourceSql}) p ON p.id=l.project_id AND p.company_id=l.company_id');
    expect(src('social-purchase-sources.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE MODE');
    expect(src('purchases.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE');
    expect(src('scheduled-tasks.ts')).toContain('FOR UPDATE OF t');
    expect(src('scheduled-tasks.ts')).toContain('FOR SHARE');
    expect(src('task-purchases.ts')).toContain("${lock?' FOR UPDATE':''}");
  });

  test('retirement:ts-statements stays strictly below the r26 BEFORE=118 baseline', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(117);
    expect(total).toBe(88); // r37 four-arm fact UNION closed helper (-3) + 3-arm NOT EXISTS proof group (-2): 93->88 // r36 package-actual source + event-fact arms: 95->93 // r35 point-read family merge (projectIdSql -> projectPointReadHead): 96->95 // r28 assignment-viewer head family 117->115; r29 ordered-image head 115->113; r30 visible-project-rows head 113->112; r31 receipt-event proof 112->111; r31 owner-or-member column 111->110; r32 purchase-source head 110->108; r33 point-read head and owner-or-member reuse 108->105. r34 staff_tasks point 4->1 (-3) and staff_projects by-ref 7->1 (-6): 105->96
  });
});

function untouchedInvariants(): boolean {
  const purchases = src('purchases.ts');
  return purchases.includes('lockQueuedCommandAuthority')
    && purchases.includes('purchaseCommandBoundary')
    && purchases.includes('projectAssignmentAuthorityReadSql');
}
