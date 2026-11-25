import { describe, expect, test } from 'vitest';
import { existsSync, readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';

const srcRoot = resolve(__dirname, '../../../packages/server-projects/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

describe('Projects read-tail r30 visible project rows retired to native Rust', () => {
  test('the TS select head is deleted and the Rust owner owns the fixed projection', () => {
    expect(existsSync(resolve(srcRoot, 'adapters/read-tail-r30.ts'))).toBe(false);
    const staff = src('staff-projects.ts');
    expect(staff).not.toContain('visibleProjectRowsHead');
    expect(staff).not.toContain('SELECT p.*');
    expect(staff).toContain('nativeProjectVisibleProject');
    expect(staff).toContain('nativeProjectVisibleProjects');
    const sql = src('native_lane_27/postgres.rs');
    expect(sql).toContain('const VISIBLE_PROJECT: &str = "SELECT p.id,p.company_id');
    expect(sql).toContain('const VISIBLE_PROJECTS: &str = "SELECT p.id,p.company_id');
    expect(sql).toContain('p.owner_user_id=$3');
    expect(sql).toContain('m.user_id=$4');
    expect(sql).toContain('assert!(!sql.contains("SELECT p.*"))');
  });

  test('locks, r2-r29 helpers and spaced JOIN constants stay untouched', () => {
    const defs = src('project-read-projections.ts');
    for (const kept of ['projectTaskOwnerSql', 'projectTaskMemberSql', 'projectOwnerOrMemberSql',
      'projectOwnerOrMemberColumnSql', 'projectMemberScopeSql', 'projectMemberScopeGuard',
      'projectHandoverSourceSql', 'socialProjectAudienceSql']) {
      expect(defs).toContain(kept);
    }
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

  test('retirement:ts-statements drops to 66 (BEFORE=71 for lane 27)', () => {
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
    // r30 visible-row head retired to native Rust (-1): 67 -> 66.
    expect(total).toBe(65); // r44 final-root projection family folded into read-tail-r40 (66->65)
  });
});

function untouchedInvariants(): boolean {
  const alert = src('scheduled-tasks.ts');
  return alert.includes("projectPurchaseSourceHead('project_alert_clock','project_alert_source','1')")
    && !alert.includes('SELECT 1 FROM staff_projects project_alert_source CROSS JOIN project_alert_clock')
    && !alert.includes('WITH project_alert_clock AS MATERIALIZED (SELECT')
    && alert.includes("purchasedCapabilityPredicate('project_alert_source.company_id',capability,'settle-existing','project_alert_clock.now_ms')");
}
