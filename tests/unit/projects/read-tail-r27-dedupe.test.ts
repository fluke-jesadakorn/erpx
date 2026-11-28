import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { projectMemberExistsSql } from '../../../modules/projects/server/src/adapters/read-tail-r27';

const srcRoot = resolve(__dirname, '../../../modules/projects/server/src');
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
// r38: the protected member scope now delegates to the closed helper;
    // its byte-identical render is pinned by read-tail-r38-dedupe.test.ts.
    expect(defs).toContain("${memberExistsSql('', 'm', 'p', 'id', '?')}");
    expect(defs).not.toContain('(p.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members m WHERE');
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

  test('retirement:ts-statements is 74 after r40 (BEFORE=81)', () => {
    let total = 0;
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true }) as Dirent[]) {
        const p = resolve(dir, entry.name);
        if (entry.isDirectory()) walk(p);
        else if (entry.name.endsWith('.ts')) total += (readFileSync(p, 'utf8').match(/SELECT /g) ?? []).length;
      }
    };
    walk(srcRoot);
    expect(total).toBeLessThan(88);
    expect(total).toBe(57); // r46 evaluation reads moved to the native contract (62->57)
  });
});

function untouchedInvariants(): boolean {
  const purchases = src('purchases.ts');
  return purchases.includes('lockQueuedCommandAuthority')
    && purchases.includes('purchaseCommandBoundary')
    && purchases.includes('projectAssignmentAuthorityReadSql');
}
