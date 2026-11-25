import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { visibleProjectRowsHead } from '../../../packages/server-projects/src/adapters/read-tail-r30';
import { projectMemberScopeSql } from '../../../packages/server-projects/src/adapters/project-read-projections';

const srcRoot = resolve(__dirname, '../../../packages/server-projects/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Verbatim pre-dedupe visible-row heads from HEAD before the r30 edit.
// These are byte-identity oracles only, never live query sources.
const byId = `SELECT p.* FROM staff_projects p WHERE p.id=? AND p.company_id=? AND ${projectMemberScopeSql}`;
const list = `SELECT p.* FROM staff_projects p WHERE p.company_id=? AND ${projectMemberScopeSql} ORDER BY p.updated_at DESC`;
const sharedHead = `SELECT p.* FROM staff_projects p WHERE `;

describe('Projects read-tail r30 visible project rows select-head dedupe', () => {
  test('helper reproduces both former inline heads byte for byte', () => {
    expect(visibleProjectRowsHead(`p.id=? AND p.company_id=? AND ${projectMemberScopeSql}`)).toBe(byId);
    expect(`${visibleProjectRowsHead(`p.company_id=? AND ${projectMemberScopeSql}`)} ORDER BY p.updated_at DESC`).toBe(list);
    expect(visibleProjectRowsHead('p.company_id=?')).toBe(`${sharedHead}p.company_id=?`);
    for (const h of [byId, list]) expect(h.startsWith(sharedHead)).toBe(true);
  });

  test('both call sites compose the helper and keep their outer SQL unchanged', () => {
    const staff = src('staff-projects.ts');
    expect(staff).toContain("import {visibleProjectRowsHead} from './read-tail-r30';");
    expect(staff).toContain('visibleProjectRowsHead(`p.id=? AND p.company_id=? AND ${projectMemberScopeSql}`)');
    expect(staff).toContain('${visibleProjectRowsHead(`p.company_id=? AND ${projectMemberScopeSql}`)} ORDER BY p.updated_at DESC');
    expect(staff).not.toContain('`SELECT p.* FROM staff_projects p WHERE p.id=?');
    expect(staff).not.toContain('`SELECT p.* FROM staff_projects p WHERE p.company_id=?');
    expect(staff).toContain("import {projectMemberScopeSql} from '@erpx/server-projects/adapters/project-read-projections';");
    expect(locksUntouched()).toBe(true);
  });

  test('locks, r2-r29 helpers and spaced JOIN constants stay untouched', () => {
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
    expect(src('read-tail-r27.ts')).toContain('projectMemberExistsSql');
    expect(src('read-tail-r28.ts')).toContain('assignmentViewerSelect');
    expect(src('read-tail-r29.ts')).toContain('orderedImageAggregateHead');
    expect(src('package-obligations.ts')).toContain('export const projectPackageActualProof=');
    expect(src('package-obligations.ts')).toContain('export const projectAssignmentScopeSql=');
    expect(src('package-obligations.ts')).toContain('const tableRowsSql=`SELECT * FROM `;');
    expect(src('executive-metric-projections.ts')).toContain('const groupedMetricSelect =');
    expect(src('executive-main-evidence-sources.ts')).toContain('const projectDependencyBlockerLateral =');
    expect(src('final-root-owner-projection-sources.ts')).toContain('const quotedProjection =');
    const chains = src('executive-evidence-chains.ts');
    expect(chains).toContain('JOIN (${sourceSql}) p ON p.id=m.project_id AND p.company_id=m.company_id');
    expect(chains).toContain('JOIN (${sourceSql}) p ON p.id=l.project_id AND p.company_id=l.company_id');
    expect(src('social-purchase-sources.ts')).toContain(
      "export const membersJsonSql=`'members',(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.user_id) FROM staff_project_members m WHERE m.project_id=p.id)`;");
    expect(src('social-purchase-sources.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE MODE');
    expect(src('social-purchase-sources.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE');
    expect(src('purchases.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE');
    expect(src('scheduled-tasks.ts')).toContain('FOR UPDATE OF t');
    expect(src('scheduled-tasks.ts')).toContain('FOR SHARE');
    expect(src('task-purchases.ts')).toContain("${lock?' FOR UPDATE':''}");
    expect(untouchedInvariants()).toBe(true);
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
    expect(total).toBe(69); // lane 27 moved the two actuals reads to native Rust (-2) // r37 four-arm fact UNION closed helper (-3) + 3-arm NOT EXISTS proof group (-2): 93->88 // r36 package-actual source + event-fact arms: 95->93 // r35 point-read family merge (projectIdSql -> projectPointReadHead): 96->95 // r29 left 113; r30 visible-project-rows head 113->112; r31 receipt-event proof 112->111; r31 owner-or-member column 111->110; r32 purchase-source head 110->108; r33 point-read head and owner-or-member reuse 108->105. r34 staff_tasks point 4->1 (-3) and staff_projects by-ref 7->1 (-6): 105->96 // r38 member EXISTS closed helper (-4) + evidence super-head (-3): 88->81
  });
});

function locksUntouched(): boolean {
  const staff = src('staff-projects.ts');
  return staff.includes('requireProjectPurchasedRead') && staff.includes('createStaffProjectsApplication');
}

function untouchedInvariants(): boolean {
  const alert = src('scheduled-tasks.ts');
  return alert.includes("projectPurchaseSourceHead('project_alert_clock','project_alert_source','1')")
    && !alert.includes('SELECT 1 FROM staff_projects project_alert_source CROSS JOIN project_alert_clock')
    && !alert.includes('WITH project_alert_clock AS MATERIALIZED (SELECT')
    && alert.includes("purchasedCapabilityPredicate('project_alert_source.company_id',capability,'settle-existing','project_alert_clock.now_ms')");
}
