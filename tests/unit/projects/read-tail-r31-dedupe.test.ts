import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { projectReceiptEventProof } from '../../../packages/server-projects/src/adapters/package-obligations';
import { projectOwnerOrMemberColumnSql } from '../../../packages/server-projects/src/adapters/project-read-projections';
import { projectMilestoneOverdueCohort } from '../../../packages/server-projects/src/adapters/executive-milestone-cohort';

const srcRoot = resolve(__dirname, '../../../packages/server-projects/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Verbatim pre-dedupe inline fragments from HEAD before the r31 edit.
// These are byte-identity oracles only, never live query sources.
const workLogArm = `(SELECT 1 FROM staff_project_work_log_events event JOIN command_receipts command ON command.record_id=l.id
 AND command.operation='project.actual-time.'||CASE WHEN l.state='approved' THEN 'approve' ELSE 'reject' END
 AND command.actor_id=event.actor_id AND command.authority_json::jsonb->>'companyId'=l.company_id
 AND command.result_json::jsonb->>'id'=l.id AND command.result_json::jsonb->>'version'=l.version::text
 AND command.result_json::jsonb->>'state'=l.state WHERE event.work_log_id=l.id AND event.work_log_version=l.version AND event.action=l.state)`;
const costAllocationArm = `(SELECT 1 FROM staff_project_cost_allocation_events event JOIN command_receipts command ON command.record_id=c.id
 AND command.operation='project.cost-allocation.'||CASE WHEN c.state='approved' THEN 'approve' ELSE 'reject' END
 AND command.actor_id=event.actor_id AND command.authority_json::jsonb->>'companyId'=c.company_id
 AND command.result_json::jsonb->>'id'=c.id AND command.result_json::jsonb->>'version'=c.version::text
 AND command.result_json::jsonb->>'state'=c.state WHERE event.allocation_id=c.id AND event.allocation_version=c.version AND event.action=c.state)`;
const ownerOrMemberP = '(p.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members member WHERE member.project_id=p.id AND member.user_id=?))';
const cohortPrefix = `p.company_id=? AND (?::text IS NULL OR p.branch_id=?) AND (?::text IS NULL OR p.department_id=?)`;
const cohortSuffix = `m.company_id=p.company_id AND m.state IN ('planned','in_progress')`;

describe('Projects read-tail r31 receipt-event and owner-or-member dedupe', () => {
  test('helper reproduces both former inline receipt-event arms byte for byte', () => {
    expect(projectReceiptEventProof('staff_project_work_log_events', 'l', 'project.actual-time.', 'work_log_id', 'work_log_version')).toBe(workLogArm);
    expect(projectReceiptEventProof('staff_project_cost_allocation_events', 'c', 'project.cost-allocation.', 'allocation_id', 'allocation_version')).toBe(costAllocationArm);
    // Table, alias, op and the two FK columns are the only degrees of freedom.
    for (const arm of [workLogArm, costAllocationArm]) {
      expect(arm.startsWith('(SELECT 1 FROM ')).toBe(true);
      expect(arm.endsWith(')')).toBe(true);
      expect(arm.split('\n')).toHaveLength(5);
      expect(arm.split('\n').slice(1).every(line => line.startsWith(' '))).toBe(true);
    }
  });

  test('executive cohort keeps its former SQL and binds via the column helper', () => {
    expect(projectOwnerOrMemberColumnSql('p', '?', 'member')).toBe(ownerOrMemberP);
    expect(projectOwnerOrMemberColumnSql('prj', '?', 'member')).toBe(
      '(prj.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members member WHERE member.project_id=prj.id AND member.user_id=?))');
    const actor = { companyId: 'co', branchId: 'br', departmentId: 'dp', userId: 'u' } as any;
    const filters = { from: '2026-01-01', to: '2026-01-31' } as any;
    const cohort = projectMilestoneOverdueCohort(actor, filters, 123, 'm', 'p');
    expect(cohort.sql).toBe(`${cohortPrefix}
      AND ${ownerOrMemberP}
      AND ${cohortSuffix}
      AND m.due_at<? AND m.due_at>=? AND m.due_at<?`);
    expect(cohort.values).toEqual(['co', 'br', 'br', 'dp', 'dp', 'u', 'u', 123,
      Date.parse('2026-01-01'), Date.parse('2026-01-31') + 86_400_000]);
    expect(projectMilestoneOverdueCohort(actor, filters, 123, 'm', 'prj').sql).toContain(
      'AND (prj.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members member WHERE member.project_id=prj.id AND member.user_id=?))');
  });

  test('both call sites compose the helpers and keep their outer SQL unchanged', () => {
    const obl = src('package-obligations.ts');
    expect(obl).toContain('export const projectReceiptEventProof=');
    // r36 folds both call sites into eventFactArm; the receipt-event tuple now
    // lives in its closed args and the helper delegates with the same params.
    expect(obl).toContain("table:'staff_project_work_log_events',op:'project.actual-time.',fkId:'work_log_id',fkVer:'work_log_version'");
    expect(obl).toContain("table:'staff_project_cost_allocation_events',op:'project.cost-allocation.',fkId:'allocation_id',fkVer:'allocation_version'");
    expect(obl).toContain('${projectReceiptEventProof(a.table,a.alias,a.op,a.fkId,a.fkVer)}');
    expect(obl).not.toContain('SELECT 1 FROM staff_project_work_log_events event JOIN');
    expect(obl).not.toContain('SELECT 1 FROM staff_project_cost_allocation_events event JOIN');

    const exec = src('executive-milestone-cohort.ts');
    expect(exec).toContain('projectOwnerOrMemberColumnSql, projectOwnerOrMemberSql');
    expect(exec).toContain("AND ${projectOwnerOrMemberColumnSql(project,'?','member')}");
    expect(exec).not.toContain('AND (${project}.owner_user_id=? OR EXISTS(');
    expect(exec).toContain('${projectOwnerOrMemberSql(project)}');
  });

  test('locks, r2-r30 helpers and spaced JOIN constants stay untouched', () => {
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
    expect(src('read-tail-r30.ts')).toContain('visibleProjectRowsHead');
    const obl = src('package-obligations.ts');
    expect(obl).toContain('export const projectPackageActualProof=');
    expect(obl).toContain('export const projectAssignmentScopeSql=');
    expect(obl).toContain('const tableRowsSql=`SELECT * FROM `;');
    expect(obl).toContain('LOCK TABLE staff_projects,staff_project_members,staff_project_milestones,staff_project_milestone_events,');
    expect(obl).toContain('staff_project_handoffs,command_receipts IN SHARE MODE');
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

  test('retirement:ts-statements is 81 after r38 (BEFORE=88)', () => {
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
    expect(total).toBe(81); // r37 four-arm fact UNION closed helper (-3) + 3-arm NOT EXISTS proof group (-2): 93->88 // r36 package-actual source + event-fact arms: 95->93 // r35 point-read family merge (projectIdSql -> projectPointReadHead): 96->95 // r30 left 112; r31 receipt-event proof 112->111; owner-or-member column 111->110; r32 purchase-source head 110->108; r33 point-read head and owner-or-member reuse 108->105. r34 staff_tasks point 4->1 (-3) and staff_projects by-ref 7->1 (-6): 105->96 // r38 member EXISTS closed helper (-4) + evidence super-head (-3): 88->81
  });
});

function untouchedInvariants(): boolean {
  const alert = src('scheduled-tasks.ts');
  return alert.includes("projectPurchaseSourceHead('project_alert_clock','project_alert_source','1')")
    && !alert.includes('SELECT 1 FROM staff_projects project_alert_source CROSS JOIN project_alert_clock')
    && !alert.includes('WITH project_alert_clock AS MATERIALIZED (SELECT')
    && alert.includes("purchasedCapabilityPredicate('project_alert_source.company_id',capability,'settle-existing','project_alert_clock.now_ms')");
}
