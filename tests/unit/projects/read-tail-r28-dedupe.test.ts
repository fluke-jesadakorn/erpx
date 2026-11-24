import { describe, expect, test } from 'vitest';
import { readFileSync, readdirSync, type Dirent } from 'node:fs';
import { resolve } from 'node:path';
import { assignmentViewerSelect } from '../../../packages/server-projects/src/adapters/read-tail-r28';

const srcRoot = resolve(__dirname, '../../../packages/server-projects/src');
const src = (name: string) => readFileSync(resolve(srcRoot, 'adapters', name), 'utf8');

// Verbatim pre-dedupe viewer heads from HEAD before the r28 edit. These are
// byte-identity oracles only, never live query sources.
const head = `SELECT g.id assignment_id,g.user_id,g.role,`;
const productEntry = `SELECT g.id assignment_id,g.user_id,g.role,clock.now_ms FROM assignments g CROSS JOIN project_entry_clock clock`;
const alertLateral = `SELECT g.id assignment_id,g.user_id,g.role,project_alert_clock.now_ms) project_alert_viewer`;
const alertAuthority = `SELECT g.id assignment_id,g.user_id,g.role,\${packageClockSql} now_ms FROM assignments g WHERE g.id=?`;

describe('Projects read-tail r28 assignment-viewer select-head dedupe', () => {
  test('helper reproduces all three former inline viewer heads byte for byte', () => {
    expect(assignmentViewerSelect).toBe(head);
    expect(`${assignmentViewerSelect}clock.now_ms FROM assignments g CROSS JOIN project_entry_clock clock`).toBe(productEntry);
    expect(`${assignmentViewerSelect}project_alert_clock.now_ms) project_alert_viewer`).toBe(alertLateral);
    expect(`${assignmentViewerSelect}\${packageClockSql} now_ms FROM assignments g WHERE g.id=?`).toBe(alertAuthority);
  });

  test('all three call sites compose the helper and keep their outer SQL unchanged', () => {
    const entry = src('product-entry.ts');
    expect(entry).toContain("import {assignmentViewerSelect} from './read-tail-r28';");
    expect(entry).toContain('(${assignmentViewerSelect}clock.now_ms FROM assignments g CROSS JOIN project_entry_clock clock');
    expect(entry).not.toContain('(SELECT g.id assignment_id,g.user_id,g.role,clock.now_ms');

    const alert = src('scheduled-tasks.ts');
    expect(alert).toContain("import {assignmentViewerSelect} from './read-tail-r28';");
    expect(alert).toContain('LATERAL (${assignmentViewerSelect}project_alert_clock.now_ms) project_alert_viewer');
    expect(alert).toContain('MATERIALIZED (${assignmentViewerSelect}${packageClockSql} now_ms FROM assignments g');
    expect(alert).not.toContain('LATERAL (SELECT g.id assignment_id,g.user_id,g.role,project_alert_clock.now_ms');
    expect(alert).not.toContain('MATERIALIZED (SELECT g.id assignment_id,g.user_id,g.role,${packageClockSql} now_ms');
  });

  test('locks, r2-r27 helpers and spaced JOIN constants stay untouched', () => {
    const defs = src('project-read-projections.ts');
    for (const kept of ['projectTaskOwnerSql', 'projectTaskMemberSql', 'projectOwnerOrMemberSql',
      'projectOwnerOrMemberColumnSql', 'projectMemberScopeSql', 'projectMemberScopeGuard',
      'projectHandoverSourceSql', 'socialProjectAudienceSql']) {
      expect(defs).toContain(kept);
    }
    expect(defs).toContain('(p.owner_user_id=? OR EXISTS(SELECT 1 FROM staff_project_members m WHERE m.project_id=p.id AND m.user_id=?))');
    expect(src('clock-cte.ts')).toContain('export const projectsClockCte=');
    expect(src('read-tail-r23.ts')).toContain('projectAssignmentAuthorityReadSql');
    expect(src('read-tail-r27.ts')).toContain('projectMemberExistsSql');
    expect(src('package-obligations.ts')).toContain('export const projectPackageActualProof=');
    expect(src('package-obligations.ts')).toContain('export const projectAssignmentScopeSql=');
    expect(src('package-obligations.ts')).toContain('const tableRowsSql=`SELECT * FROM `;');
    expect(src('executive-metric-projections.ts')).toContain('const groupedMetricSelect =');
    expect(src('executive-main-evidence-sources.ts')).toContain('const projectDependencyBlockerLateral =');
    expect(src('final-root-owner-projection-sources.ts')).toContain('const quotedProjection =');
    const chains = src('executive-evidence-chains.ts');
    expect(chains).toContain('JOIN (${sourceSql}) p ON p.id=m.project_id AND p.company_id=m.company_id');
    expect(chains).toContain('JOIN (${sourceSql}) p ON p.id=l.project_id AND p.company_id=l.company_id');
    expect(src('social-purchase-sources.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE MODE');
    expect(src('purchases.ts')).toContain('LOCK TABLE staff_projects,staff_project_members IN SHARE ROW EXCLUSIVE MODE');
    expect(src('scheduled-tasks.ts')).toContain('FOR UPDATE OF t');
    expect(src('scheduled-tasks.ts')).toContain('FOR SHARE');
    expect(src('task-purchases.ts')).toContain("${lock?' FOR UPDATE':''}");
    expect(untouchedInvariants()).toBe(true);
  });

  test('retirement:ts-statements stays strictly below the r27 BEFORE=117 baseline', () => {
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
    expect(total).toBe(115); // r27 left 117; r28 assignment-viewer head family 117->115.
  });
});

function untouchedInvariants(): boolean {
  const alert = src('scheduled-tasks.ts');
  return alert.includes("projectsClockCte('project_alert_clock')")
    && alert.includes('SELECT 1 FROM staff_projects project_alert_source CROSS JOIN project_alert_clock')
    && !alert.includes('WITH project_alert_clock AS MATERIALIZED (SELECT')
    && alert.includes("purchasedCapabilityPredicate('project_alert_source.company_id',capability,'settle-existing','project_alert_clock.now_ms')");
}
